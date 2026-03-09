use axum::extract::{Form, Path, Query, State};
use axum::response::{Html, IntoResponse};
use axum::response::Redirect;
use axum::routing::{get, post};
use axum::Router;
use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use minijinja::{context, Environment};
use serde::Deserialize;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Simple per-IP rate limiter for login attempts.
/// Tracks (attempt_count, window_start) per IP.
pub struct RateLimiter {
    attempts: Mutex<HashMap<String, (u32, std::time::Instant)>>,
    max_attempts: u32,
    window: std::time::Duration,
}

impl RateLimiter {
    pub fn new(max_attempts: u32, window_secs: u64) -> Self {
        Self {
            attempts: Mutex::new(HashMap::new()),
            max_attempts,
            window: std::time::Duration::from_secs(window_secs),
        }
    }

    /// Returns true if the request should be rejected (rate limited).
    pub async fn check_limited(&self, key: &str) -> bool {
        let mut map = self.attempts.lock().await;
        let now = std::time::Instant::now();

        if let Some((count, start)) = map.get_mut(key) {
            if now.duration_since(*start) > self.window {
                // Window expired, reset
                *count = 1;
                *start = now;
                false
            } else if *count >= self.max_attempts {
                true
            } else {
                *count += 1;
                false
            }
        } else {
            map.insert(key.to_string(), (1, now));
            false
        }
    }
}

pub struct AppState {
    pub pool: SqlitePool,
    pub templates: Environment<'static>,
    pub login_limiter: RateLimiter,
}

pub fn create_router(pool: SqlitePool) -> Router {
    let mut env = Environment::new();
    env.set_loader(minijinja::path_loader("templates"));

    let state = Arc::new(AppState {
        pool,
        templates: env,
        // 10 login attempts per IP per 15 minutes
        login_limiter: RateLimiter::new(10, 900),
    });

    Router::new()
        .merge(crate::auth::auth_router())
        .route("/", get(root_redirect))
        .route("/dashboard", get(dashboard))
        .route("/dashboard/bookings/{id}/cancel", post(cancel_booking))
        .route("/dashboard/bookings/{id}/confirm", post(confirm_booking))
        .route("/dashboard/event-types/new", get(new_event_type_form).post(create_event_type))
        .route("/dashboard/event-types/{slug}/edit", get(edit_event_type_form).post(update_event_type))
        .route("/dashboard/event-types/{slug}/toggle", post(toggle_event_type))
        // Calendar source management
        .route("/dashboard/sources/new", get(new_source_form).post(create_source))
        .route("/dashboard/sources/{id}/remove", post(remove_source))
        .route("/dashboard/sources/{id}/test", post(test_source))
        .route("/dashboard/sources/{id}/sync", post(sync_source))
        .route("/dashboard/sources/{id}/write-calendar", post(set_write_calendar))
        // Troubleshoot
        .route("/dashboard/troubleshoot", get(troubleshoot))
        // Admin routes
        .route("/dashboard/admin", get(admin_dashboard))
        .route("/dashboard/admin/users/{id}/toggle-role", post(admin_toggle_role))
        .route("/dashboard/admin/users/{id}/toggle-enabled", post(admin_toggle_enabled))
        .route("/dashboard/admin/auth", post(admin_update_auth))
        .route("/dashboard/admin/oidc", post(admin_update_oidc))
        // Group event type management
        .route("/dashboard/group-event-types/new", get(new_group_event_type_form).post(create_group_event_type))
        // Group public routes (before the catch-all)
        .route("/g/{group_slug}", get(group_profile))
        .route("/g/{group_slug}/{slug}", get(show_group_slots))
        .route("/g/{group_slug}/{slug}/book", get(show_group_book_form).post(handle_group_booking))
        // User-scoped public booking routes
        .route("/u/{username}", get(user_profile))
        .route("/u/{username}/{slug}", get(show_slots_for_user))
        .route("/u/{username}/{slug}/book", get(show_book_form_for_user).post(handle_booking_for_user))
        // Legacy single-user routes (kept for backward compatibility)
        .route("/{slug}", get(show_slots))
        .route("/{slug}/book", get(show_book_form).post(handle_booking))
        .with_state(state)
}

// --- Root redirect ---

async fn root_redirect() -> impl IntoResponse {
    Redirect::to("/auth/login")
}

// --- Dashboard ---

async fn dashboard(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
) -> impl IntoResponse {
    let user = &auth_user.0;

    let event_types: Vec<(String, String, i32, bool, i32)> = sqlx::query_as(
        "SELECT et.slug, et.title, et.duration_min, et.enabled, et.requires_confirmation
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         WHERE a.user_id = ? AND et.group_id IS NULL
         ORDER BY et.created_at",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    // Query group event types the user can manage
    let group_event_types: Vec<(String, String, i32, bool, String, String)> = sqlx::query_as(
        "SELECT et.slug, et.title, et.duration_min, et.enabled, g.name, g.slug
         FROM event_types et
         JOIN groups g ON g.id = et.group_id
         JOIN user_groups ug ON ug.group_id = g.id
         WHERE ug.user_id = ?
         ORDER BY g.name, et.created_at",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    // Check if user belongs to any groups (for showing "+ New" link)
    let user_has_groups: bool = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM user_groups WHERE user_id = ?",
    )
    .bind(&user.id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0) > 0;

    let pending_bookings: Vec<(String, String, String, String, String, String)> = sqlx::query_as(
        "SELECT b.id, b.guest_name, b.guest_email, b.start_at, b.end_at, et.title
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         WHERE a.user_id = ? AND b.status = 'pending'
         ORDER BY b.start_at",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let upcoming_bookings: Vec<(String, String, String, String, String, String)> = sqlx::query_as(
        "SELECT b.id, b.guest_name, b.guest_email, b.start_at, b.end_at, et.title
         FROM bookings b
         JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id
         WHERE a.user_id = ? AND b.status = 'confirmed' AND b.start_at >= datetime('now')
         ORDER BY b.start_at
         LIMIT 10",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    // Fetch CalDAV sources
    let sources: Vec<(String, String, String, String, Option<String>, bool, Option<String>)> = sqlx::query_as(
        "SELECT cs.id, cs.name, cs.url, cs.username, cs.last_synced, cs.enabled, cs.write_calendar_href
         FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ?
         ORDER BY cs.created_at",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let tmpl = match state.templates.get_template("dashboard.html") {
        Ok(t) => t,
        Err(e) => return Html(format!("Template error: {}", e)),
    };

    let et_ctx: Vec<minijinja::Value> = event_types
        .iter()
        .map(|(slug, title, duration, enabled, req_conf)| {
            context! { slug => slug, title => title, duration_min => duration, enabled => enabled, requires_confirmation => *req_conf != 0 }
        })
        .collect();

    let group_et_ctx: Vec<minijinja::Value> = group_event_types
        .iter()
        .map(|(slug, title, duration, enabled, group_name, group_slug)| {
            context! { slug => slug, title => title, duration_min => duration, enabled => enabled, group_name => group_name, group_slug => group_slug }
        })
        .collect();

    let pending_ctx: Vec<minijinja::Value> = pending_bookings
        .iter()
        .map(|(id, name, email, start, end, title)| {
            context! { id => id, guest_name => name, guest_email => email, start_at => start, end_at => end, event_title => title }
        })
        .collect();

    let bookings_ctx: Vec<minijinja::Value> = upcoming_bookings
        .iter()
        .map(|(id, name, email, start, end, title)| {
            context! { id => id, guest_name => name, guest_email => email, start_at => start, end_at => end, event_title => title }
        })
        .collect();

    // Fetch calendars for write-back selector
    let all_calendars: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT c.source_id, c.href, c.display_name
         FROM calendars c
         JOIN caldav_sources cs ON cs.id = c.source_id
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ?
         ORDER BY c.display_name",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let sources_ctx: Vec<minijinja::Value> = sources
        .iter()
        .map(|(id, name, url, username, last_synced, enabled, write_cal)| {
            let cals: Vec<minijinja::Value> = all_calendars
                .iter()
                .filter(|(sid, _, _)| sid == id)
                .map(|(_, href, display)| {
                    context! {
                        href => href,
                        name => display.as_deref().unwrap_or(href),
                    }
                })
                .collect();
            context! {
                id => id,
                id_short => &id[..8.min(id.len())],
                name => name,
                url => url,
                username => username,
                last_synced => last_synced.as_deref().unwrap_or("never"),
                enabled => enabled,
                write_calendar_href => write_cal.as_deref().unwrap_or(""),
                calendars => cals,
            }
        })
        .collect();

    Html(
        tmpl.render(context! {
            user_name => user.name,
            user_email => user.email,
            user_role => user.role,
            username => user.username,
            event_types => et_ctx,
            group_event_types => group_et_ctx,
            user_has_groups => user_has_groups,
            pending_bookings => pending_ctx,
            bookings => bookings_ctx,
            sources => sources_ctx,
        })
        .unwrap_or_else(|e| format!("Template error: {}", e)),
    )
}

// --- Cancel booking ---

#[derive(Deserialize)]
struct CancelForm {
    reason: Option<String>,
}

async fn cancel_booking(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(booking_id): Path<String>,
    Form(form): Form<CancelForm>,
) -> impl IntoResponse {
    let user = &auth_user.0;

    // Verify the booking belongs to this user and is confirmed
    let booking: Option<(String, String, String, String, String, String, String, String)> =
        sqlx::query_as(
            "SELECT b.id, b.uid, b.guest_name, b.guest_email, b.start_at, b.end_at, et.title, a.id
             FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             JOIN accounts a ON a.id = et.account_id
             WHERE b.id = ? AND a.user_id = ? AND b.status = 'confirmed'",
        )
        .bind(&booking_id)
        .bind(&user.id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

    let (bid, uid, guest_name, guest_email, start_at, end_at, event_title, _account_id) =
        match booking {
            Some(b) => b,
            None => return Redirect::to("/dashboard").into_response(),
        };

    // Cancel the booking
    let _ = sqlx::query("UPDATE bookings SET status = 'cancelled' WHERE id = ?")
        .bind(&bid)
        .execute(&state.pool)
        .await;

    // Delete from CalDAV calendar
    caldav_delete_booking(&state.pool, &user.id, &uid).await;

    // Send cancellation emails
    if let Ok(Some(smtp_config)) = crate::email::load_smtp_config(&state.pool).await {
        // Extract date and times from start_at/end_at
        let date = if start_at.len() >= 10 { &start_at[..10] } else { &start_at };
        let start_time = if start_at.len() >= 16 { &start_at[11..16] } else { "00:00" };
        let end_time = if end_at.len() >= 16 { &end_at[11..16] } else { "00:00" };

        let reason = form.reason.filter(|r| !r.trim().is_empty());

        let details = crate::email::CancellationDetails {
            event_title: event_title.clone(),
            date: date.to_string(),
            start_time: start_time.to_string(),
            end_time: end_time.to_string(),
            guest_name,
            guest_email,
            host_name: user.name.clone(),
            host_email: user.email.clone(),
            uid,
            reason,
        };

        let _ = crate::email::send_guest_cancellation(&smtp_config, &details).await;
        let _ = crate::email::send_host_cancellation(&smtp_config, &details).await;
    }

    Redirect::to("/dashboard").into_response()
}

// --- Confirm pending booking ---

async fn confirm_booking(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(booking_id): Path<String>,
) -> impl IntoResponse {
    let user = &auth_user.0;

    // Verify the booking belongs to this user and is pending
    let booking: Option<(String, String, String, String, String, String, String, Option<String>)> =
        sqlx::query_as(
            "SELECT b.id, b.uid, b.guest_name, b.guest_email, b.start_at, b.end_at, et.title, et.location_value
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

    let (bid, uid, guest_name, guest_email, start_at, end_at, event_title, location_value) =
        match booking {
            Some(b) => b,
            None => return Redirect::to("/dashboard").into_response(),
        };

    // Confirm the booking
    let _ = sqlx::query("UPDATE bookings SET status = 'confirmed' WHERE id = ?")
        .bind(&bid)
        .execute(&state.pool)
        .await;

    let date = if start_at.len() >= 10 { start_at[..10].to_string() } else { start_at.clone() };
    let start_time = if start_at.len() >= 16 { start_at[11..16].to_string() } else { "00:00".to_string() };
    let end_time = if end_at.len() >= 16 { end_at[11..16].to_string() } else { "00:00".to_string() };

    let details = crate::email::BookingDetails {
        event_title,
        date,
        start_time,
        end_time,
        guest_name,
        guest_email,
        guest_timezone: "UTC".to_string(),
        host_name: user.name.clone(),
        host_email: user.email.clone(),
        uid: uid.clone(),
        notes: None,
        location: location_value,
    };

    // Push to CalDAV calendar
    caldav_push_booking(&state.pool, &user.id, &uid, &details).await;

    // Send confirmation emails
    if let Ok(Some(smtp_config)) = crate::email::load_smtp_config(&state.pool).await {
        let _ = crate::email::send_guest_confirmation(&smtp_config, &details).await;
    }

    Redirect::to("/dashboard").into_response()
}

// --- Event type CRUD ---

#[derive(Deserialize)]
struct EventTypeForm {
    title: String,
    slug: String,
    description: Option<String>,
    duration_min: i32,
    buffer_before: Option<i32>,
    buffer_after: Option<i32>,
    min_notice_min: Option<i32>,
    requires_confirmation: Option<String>, // checkbox: "on" or absent
    location_type: Option<String>, // "link", "phone", "in_person", "custom"
    location_value: Option<String>,
    // Availability schedule
    avail_days: Option<String>, // comma-separated: "1,2,3,4,5"
    avail_start: Option<String>, // "09:00"
    avail_end: Option<String>, // "17:00"
    // Group (optional)
    group_id: Option<String>,
}

async fn new_event_type_form(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
) -> impl IntoResponse {
    let user = &auth_user.0;

    // Get groups the user belongs to
    let groups: Vec<(String, String)> = sqlx::query_as(
        "SELECT g.id, g.name FROM groups g JOIN user_groups ug ON ug.group_id = g.id WHERE ug.user_id = ? ORDER BY g.name",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let groups_ctx: Vec<minijinja::Value> = groups
        .iter()
        .map(|(id, name)| context! { id => id, name => name })
        .collect();

    let tmpl = match state.templates.get_template("event_type_form.html") {
        Ok(t) => t,
        Err(e) => return Html(format!("Template error: {}", e)),
    };

    Html(
        tmpl.render(context! {
            editing => false,
            groups => groups_ctx,
            form_title => "",
            form_slug => "",
            form_description => "",
            form_duration => 30,
            form_buffer_before => 0,
            form_buffer_after => 0,
            form_min_notice => 60,
            form_requires_confirmation => false,
            form_location_type => "link",
            form_location_value => "",
            form_avail_days => "1,2,3,4,5",
            form_avail_start => "09:00",
            form_avail_end => "17:00",
            error => "",
        })
        .unwrap_or_else(|e| format!("Template error: {}", e)),
    )
}

async fn create_event_type(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Form(form): Form<EventTypeForm>,
) -> impl IntoResponse {
    let user = &auth_user.0;

    // Find the user's account
    let account_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM accounts WHERE user_id = ? LIMIT 1",
    )
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let account_id = match account_id {
        Some(id) => id,
        None => return Redirect::to("/dashboard").into_response(),
    };

    // Validate slug
    let slug = form.slug.trim().to_lowercase().replace(' ', "-");
    if slug.is_empty() {
        return render_event_type_form_error(&state, "Slug is required.", &form, false).into_response();
    }

    // Check uniqueness
    let existing: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM event_types WHERE account_id = ? AND slug = ?",
    )
    .bind(&account_id)
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    if existing.is_some() {
        return render_event_type_form_error(&state, "An event type with this slug already exists.", &form, false).into_response();
    }

    let et_id = uuid::Uuid::new_v4().to_string();
    let requires_confirmation = form.requires_confirmation.as_deref() == Some("on");

    let location_type = form.location_type.as_deref().unwrap_or("link");
    let location_value = form.location_value.as_deref().filter(|s| !s.trim().is_empty());

    // Check if a group_id was provided and it's non-empty
    let group_id = form.group_id.as_deref().filter(|s| !s.trim().is_empty());

    let _ = sqlx::query(
        "INSERT INTO event_types (id, account_id, slug, title, description, duration_min, buffer_before, buffer_after, min_notice_min, requires_confirmation, location_type, location_value, group_id, created_by_user_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&et_id)
    .bind(&account_id)
    .bind(&slug)
    .bind(form.title.trim())
    .bind(form.description.as_deref().filter(|s| !s.trim().is_empty()))
    .bind(form.duration_min)
    .bind(form.buffer_before.unwrap_or(0))
    .bind(form.buffer_after.unwrap_or(0))
    .bind(form.min_notice_min.unwrap_or(60))
    .bind(requires_confirmation as i32)
    .bind(location_type)
    .bind(location_value)
    .bind(group_id)
    .bind(if group_id.is_some() { Some(&user.id) } else { None })
    .execute(&state.pool)
    .await;

    // Create availability rules
    let avail_days = form.avail_days.as_deref().unwrap_or("1,2,3,4,5");
    let avail_start = form.avail_start.as_deref().unwrap_or("09:00");
    let avail_end = form.avail_end.as_deref().unwrap_or("17:00");

    for day_str in avail_days.split(',') {
        if let Ok(day) = day_str.trim().parse::<i32>() {
            if (0..=6).contains(&day) {
                let rule_id = uuid::Uuid::new_v4().to_string();
                let _ = sqlx::query(
                    "INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time) VALUES (?, ?, ?, ?, ?)",
                )
                .bind(&rule_id)
                .bind(&et_id)
                .bind(day)
                .bind(avail_start)
                .bind(avail_end)
                .execute(&state.pool)
                .await;
            }
        }
    }

    Redirect::to("/dashboard").into_response()
}

async fn edit_event_type_form(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(slug): Path<String>,
) -> impl IntoResponse {
    let user = &auth_user.0;

    let et: Option<(String, String, String, Option<String>, i32, i32, i32, i32, i32, String, Option<String>)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.description, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min, et.requires_confirmation, et.location_type, et.location_value
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         WHERE a.user_id = ? AND et.slug = ?",
    )
    .bind(&user.id)
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (et_id, et_slug, et_title, et_desc, duration, buf_before, buf_after, min_notice, requires_conf, loc_type, loc_value) = match et {
        Some(e) => e,
        None => return Html("Event type not found.".to_string()),
    };

    // Get current availability rules
    let rules: Vec<(i32, String, String)> = sqlx::query_as(
        "SELECT day_of_week, start_time, end_time FROM availability_rules WHERE event_type_id = ? ORDER BY day_of_week LIMIT 1",
    )
    .bind(&et_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let all_rules: Vec<(i32,)> = sqlx::query_as(
        "SELECT DISTINCT day_of_week FROM availability_rules WHERE event_type_id = ? ORDER BY day_of_week",
    )
    .bind(&et_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let avail_days: String = all_rules.iter().map(|(d,)| d.to_string()).collect::<Vec<_>>().join(",");
    let (avail_start, avail_end) = rules.first()
        .map(|(_, s, e)| (s.clone(), e.clone()))
        .unwrap_or_else(|| ("09:00".to_string(), "17:00".to_string()));

    let tmpl = match state.templates.get_template("event_type_form.html") {
        Ok(t) => t,
        Err(e) => return Html(format!("Template error: {}", e)),
    };

    Html(
        tmpl.render(context! {
            editing => true,
            original_slug => et_slug,
            form_title => et_title,
            form_slug => et_slug,
            form_description => et_desc.unwrap_or_default(),
            form_duration => duration,
            form_buffer_before => buf_before,
            form_buffer_after => buf_after,
            form_min_notice => min_notice,
            form_requires_confirmation => requires_conf != 0,
            form_location_type => loc_type,
            form_location_value => loc_value.unwrap_or_default(),
            form_avail_days => avail_days,
            form_avail_start => avail_start,
            form_avail_end => avail_end,
            error => "",
        })
        .unwrap_or_else(|e| format!("Template error: {}", e)),
    )
}

async fn update_event_type(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(slug): Path<String>,
    Form(form): Form<EventTypeForm>,
) -> impl IntoResponse {
    let user = &auth_user.0;

    let et: Option<(String, String)> = sqlx::query_as(
        "SELECT et.id, et.account_id
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         WHERE a.user_id = ? AND et.slug = ?",
    )
    .bind(&user.id)
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (et_id, account_id) = match et {
        Some(e) => e,
        None => return Redirect::to("/dashboard").into_response(),
    };

    let new_slug = form.slug.trim().to_lowercase().replace(' ', "-");
    let requires_confirmation = form.requires_confirmation.as_deref() == Some("on");

    // Check slug uniqueness if changed
    if new_slug != slug {
        let existing: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM event_types WHERE account_id = ? AND slug = ?",
        )
        .bind(&account_id)
        .bind(&new_slug)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

        if existing.is_some() {
            return render_event_type_form_error(&state, "An event type with this slug already exists.", &form, true).into_response();
        }
    }

    let location_type = form.location_type.as_deref().unwrap_or("link");
    let location_value = form.location_value.as_deref().filter(|s| !s.trim().is_empty());

    let _ = sqlx::query(
        "UPDATE event_types SET slug = ?, title = ?, description = ?, duration_min = ?, buffer_before = ?, buffer_after = ?, min_notice_min = ?, requires_confirmation = ?, location_type = ?, location_value = ? WHERE id = ?",
    )
    .bind(&new_slug)
    .bind(form.title.trim())
    .bind(form.description.as_deref().filter(|s| !s.trim().is_empty()))
    .bind(form.duration_min)
    .bind(form.buffer_before.unwrap_or(0))
    .bind(form.buffer_after.unwrap_or(0))
    .bind(form.min_notice_min.unwrap_or(60))
    .bind(requires_confirmation as i32)
    .bind(location_type)
    .bind(location_value)
    .bind(&et_id)
    .execute(&state.pool)
    .await;

    // Update availability rules: delete old, insert new
    let _ = sqlx::query("DELETE FROM availability_rules WHERE event_type_id = ?")
        .bind(&et_id)
        .execute(&state.pool)
        .await;

    let avail_days = form.avail_days.as_deref().unwrap_or("1,2,3,4,5");
    let avail_start = form.avail_start.as_deref().unwrap_or("09:00");
    let avail_end = form.avail_end.as_deref().unwrap_or("17:00");

    for day_str in avail_days.split(',') {
        if let Ok(day) = day_str.trim().parse::<i32>() {
            if (0..=6).contains(&day) {
                let rule_id = uuid::Uuid::new_v4().to_string();
                let _ = sqlx::query(
                    "INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time) VALUES (?, ?, ?, ?, ?)",
                )
                .bind(&rule_id)
                .bind(&et_id)
                .bind(day)
                .bind(avail_start)
                .bind(avail_end)
                .execute(&state.pool)
                .await;
            }
        }
    }

    Redirect::to("/dashboard").into_response()
}

async fn toggle_event_type(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(slug): Path<String>,
) -> impl IntoResponse {
    let user = &auth_user.0;

    let _ = sqlx::query(
        "UPDATE event_types SET enabled = CASE WHEN enabled = 1 THEN 0 ELSE 1 END
         WHERE slug = ? AND account_id IN (SELECT id FROM accounts WHERE user_id = ?)",
    )
    .bind(&slug)
    .bind(&user.id)
    .execute(&state.pool)
    .await;

    Redirect::to("/dashboard")
}

// --- Calendar source management ---

#[derive(Deserialize)]
struct SourceForm {
    provider: Option<String>,
    name: String,
    url: String,
    username: String,
    password: String,
    no_test: Option<String>,
}

fn caldav_providers() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("bluemind", "BlueMind", "https://mail.example.com/dav/"),
        ("nextcloud", "Nextcloud", "https://cloud.example.com/remote.php/dav"),
        ("fastmail", "Fastmail", "https://caldav.fastmail.com/dav/calendars/user/you@fastmail.com/"),
        ("icloud", "iCloud", "https://caldav.icloud.com/"),
        ("google", "Google", "https://apidata.googleusercontent.com/caldav/v2/your@gmail.com/"),
        ("zimbra", "Zimbra", "https://mail.example.com/dav/"),
        ("sogo", "SOGo", "https://mail.example.com/SOGo/dav/"),
        ("radicale", "Radicale", "https://cal.example.com/"),
        ("other", "Other / Generic CalDAV", ""),
    ]
}

async fn new_source_form(
    State(state): State<Arc<AppState>>,
    _auth_user: crate::auth::AuthUser,
) -> impl IntoResponse {
    let tmpl = match state.templates.get_template("source_form.html") {
        Ok(t) => t,
        Err(e) => return Html(format!("Template error: {}", e)),
    };

    let providers: Vec<minijinja::Value> = caldav_providers()
        .iter()
        .map(|(id, name, url)| context! { id => id, name => name, url => url })
        .collect();

    Html(
        tmpl.render(context! {
            providers => providers,
            form_provider => "bluemind",
            form_name => "",
            form_url => "https://mail.example.com/dav/",
            form_username => "",
            error => "",
        })
        .unwrap_or_else(|e| format!("Template error: {}", e)),
    )
}

async fn create_source(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Form(form): Form<SourceForm>,
) -> impl IntoResponse {
    let user = &auth_user.0;

    let account_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM accounts WHERE user_id = ? LIMIT 1",
    )
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let account_id = match account_id {
        Some(id) => id,
        None => return render_source_form_error(&state, "No scheduling account found. Please contact an administrator.", &form).into_response(),
    };

    let url = form.url.trim().to_string();
    let username = form.username.trim().to_string();
    let name = form.name.trim().to_string();

    if url.is_empty() || username.is_empty() || name.is_empty() || form.password.is_empty() {
        return render_source_form_error(&state, "All fields are required.", &form).into_response();
    }

    // Test connection unless skip requested
    let skip_test = form.no_test.as_deref() == Some("on");
    if !skip_test {
        let client = crate::caldav::CaldavClient::new(&url, &username, &form.password);
        match client.check_connection().await {
            Ok(_) => {} // fine, even if CalDAV not explicitly detected
            Err(e) => {
                let msg = format!("Connection failed: {}. Check the URL and credentials, or check \"Skip connection test\" to save anyway.", e);
                return render_source_form_error(&state, &msg, &form).into_response();
            }
        }
    }

    let id = uuid::Uuid::new_v4().to_string();
    let password_hex = hex::encode(form.password.as_bytes());

    let _ = sqlx::query(
        "INSERT INTO caldav_sources (id, account_id, name, url, username, password_enc) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&account_id)
    .bind(&name)
    .bind(&url)
    .bind(&username)
    .bind(&password_hex)
    .execute(&state.pool)
    .await;

    Redirect::to("/dashboard").into_response()
}

fn render_source_form_error(state: &AppState, error: &str, form: &SourceForm) -> Html<String> {
    let tmpl = match state.templates.get_template("source_form.html") {
        Ok(t) => t,
        Err(e) => return Html(format!("Template error: {}", e)),
    };

    let providers: Vec<minijinja::Value> = caldav_providers()
        .iter()
        .map(|(id, name, url)| context! { id => id, name => name, url => url })
        .collect();

    Html(
        tmpl.render(context! {
            providers => providers,
            form_provider => form.provider.as_deref().unwrap_or("other"),
            form_name => form.name.as_str(),
            form_url => form.url.as_str(),
            form_username => form.username.as_str(),
            error => error,
        })
        .unwrap_or_else(|e| format!("Template error: {}", e)),
    )
}

async fn remove_source(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(source_id): Path<String>,
) -> impl IntoResponse {
    let user = &auth_user.0;

    // Verify source belongs to this user before deleting
    let _ = sqlx::query(
        "DELETE FROM caldav_sources WHERE id = ? AND account_id IN (SELECT id FROM accounts WHERE user_id = ?)",
    )
    .bind(&source_id)
    .bind(&user.id)
    .execute(&state.pool)
    .await;

    Redirect::to("/dashboard")
}

async fn test_source(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(source_id): Path<String>,
) -> impl IntoResponse {
    let user = &auth_user.0;

    let source: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT cs.url, cs.username, cs.password_enc, cs.name
         FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE cs.id = ? AND a.user_id = ?",
    )
    .bind(&source_id)
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (url, username, password_hex, name) = match source {
        Some(s) => s,
        None => return Html("Source not found.".to_string()).into_response(),
    };

    let password = match hex::decode(&password_hex) {
        Ok(bytes) => String::from_utf8(bytes).unwrap_or_default(),
        Err(_) => return Html("Invalid stored credentials.".to_string()).into_response(),
    };

    let client = crate::caldav::CaldavClient::new(&url, &username, &password);
    let result = match client.check_connection().await {
        Ok(true) => format!("'{}' — connection OK, CalDAV supported.", name),
        Ok(false) => format!("'{}' — connected but CalDAV not explicitly detected. Sync may still work.", name),
        Err(e) => format!("'{}' — connection failed: {}", name, e),
    };

    // Return a simple page with back link
    let tmpl = match state.templates.get_template("source_test.html") {
        Ok(t) => t,
        Err(_) => return Html(format!(
            "<p>{}</p><p><a href=\"/dashboard\">Back to dashboard</a></p>", result
        )).into_response(),
    };
    Html(
        tmpl.render(context! { result => result })
            .unwrap_or_else(|e| format!("Template error: {}", e)),
    ).into_response()
}

async fn sync_source(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(source_id): Path<String>,
) -> impl IntoResponse {
    let user = &auth_user.0;

    let source: Option<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT cs.id, cs.url, cs.username, cs.password_enc, cs.name
         FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE cs.id = ? AND a.user_id = ?",
    )
    .bind(&source_id)
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (sid, url, username, password_hex, name) = match source {
        Some(s) => s,
        None => return Html("Source not found.".to_string()).into_response(),
    };

    let password = match hex::decode(&password_hex) {
        Ok(bytes) => String::from_utf8(bytes).unwrap_or_default(),
        Err(_) => return Html("Invalid stored credentials.".to_string()).into_response(),
    };

    let client = crate::caldav::CaldavClient::new(&url, &username, &password);
    let mut messages: Vec<String> = Vec::new();

    // Discover → list calendars → fetch events (same as sync command)
    let principal = match client.discover_principal().await {
        Ok(p) => p,
        Err(e) => {
            messages.push(format!("Could not discover principal: {}", e));
            return render_sync_result(&state, &name, &messages).into_response();
        }
    };

    let calendar_home = match client.discover_calendar_home(&principal).await {
        Ok(h) => h,
        Err(e) => {
            messages.push(format!("Could not discover calendar home: {}", e));
            return render_sync_result(&state, &name, &messages).into_response();
        }
    };

    let calendars = match client.list_calendars(&calendar_home).await {
        Ok(c) => c,
        Err(e) => {
            messages.push(format!("Could not list calendars: {}", e));
            return render_sync_result(&state, &name, &messages).into_response();
        }
    };

    let mut total_events = 0usize;

    for cal_info in &calendars {
        let display = cal_info.display_name.as_deref().unwrap_or(&cal_info.href);

        // Upsert calendar record
        let cal_id: String = match sqlx::query_scalar::<_, String>(
            "SELECT id FROM calendars WHERE source_id = ? AND href = ?",
        )
        .bind(&sid)
        .bind(&cal_info.href)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
        {
            Some(id) => id,
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                let _ = sqlx::query(
                    "INSERT INTO calendars (id, source_id, href, display_name, color, ctag) VALUES (?, ?, ?, ?, ?, ?)",
                )
                .bind(&id)
                .bind(&sid)
                .bind(&cal_info.href)
                .bind(&cal_info.display_name)
                .bind(&cal_info.color)
                .bind(&cal_info.ctag)
                .execute(&state.pool)
                .await;
                id
            }
        };

        // Fetch events
        match client.fetch_events(&cal_info.href).await {
            Ok(raw_events) => {
                let mut count = 0;
                for raw in &raw_events {
                    let uid = extract_ical_field(&raw.ical_data, "UID")
                        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                    let summary = extract_ical_field(&raw.ical_data, "SUMMARY");
                    let start_at = extract_ical_field(&raw.ical_data, "DTSTART").unwrap_or_default();
                    let end_at = extract_ical_field(&raw.ical_data, "DTEND").unwrap_or_default();
                    let location = extract_ical_field(&raw.ical_data, "LOCATION");
                    let description = extract_ical_field(&raw.ical_data, "DESCRIPTION");
                    let status = extract_ical_field(&raw.ical_data, "STATUS");
                    let rrule = extract_ical_field(&raw.ical_data, "RRULE");
                    let recurrence_id = extract_ical_field(&raw.ical_data, "RECURRENCE-ID");

                    let event_id = uuid::Uuid::new_v4().to_string();
                    let _ = sqlx::query(
                        "INSERT INTO events (id, calendar_id, uid, summary, start_at, end_at, location, description, status, rrule, raw_ical, recurrence_id)
                         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                         ON CONFLICT(uid) DO UPDATE SET
                           summary = excluded.summary,
                           start_at = excluded.start_at,
                           end_at = excluded.end_at,
                           location = excluded.location,
                           description = excluded.description,
                           status = excluded.status,
                           rrule = excluded.rrule,
                           raw_ical = excluded.raw_ical,
                           recurrence_id = excluded.recurrence_id,
                           synced_at = datetime('now')",
                    )
                    .bind(&event_id)
                    .bind(&cal_id)
                    .bind(&uid)
                    .bind(&summary)
                    .bind(&start_at)
                    .bind(&end_at)
                    .bind(&location)
                    .bind(&description)
                    .bind(&status)
                    .bind(&rrule)
                    .bind(&raw.ical_data)
                    .bind(&recurrence_id)
                    .execute(&state.pool)
                    .await;

                    count += 1;
                }
                total_events += count;
                messages.push(format!("{} — {} event(s)", display, count));
            }
            Err(e) => {
                messages.push(format!("{} — failed: {}", display, e));
            }
        }
    }

    // Update last_synced
    let _ = sqlx::query("UPDATE caldav_sources SET last_synced = datetime('now') WHERE id = ?")
        .bind(&sid)
        .execute(&state.pool)
        .await;

    messages.push(format!("Sync complete: {} calendars, {} events total.", calendars.len(), total_events));

    render_sync_result(&state, &name, &messages).into_response()
}

fn render_sync_result(state: &AppState, source_name: &str, messages: &[String]) -> Html<String> {
    let tmpl = match state.templates.get_template("source_test.html") {
        Ok(t) => t,
        Err(_) => return Html(format!(
            "<p>{}</p><p><a href=\"/dashboard\">Back to dashboard</a></p>",
            messages.join("<br>")
        )),
    };
    Html(
        tmpl.render(context! { result => messages.join("\n"), source_name => source_name })
            .unwrap_or_else(|e| format!("Template error: {}", e)),
    )
}

#[derive(Deserialize)]
struct WriteCalendarForm {
    calendar_href: String,
}

async fn set_write_calendar(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(source_id): Path<String>,
    Form(form): Form<WriteCalendarForm>,
) -> impl IntoResponse {
    let user = &auth_user.0;

    // Verify source belongs to this user
    let owned: Option<(String,)> = sqlx::query_as(
        "SELECT cs.id FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE cs.id = ? AND a.user_id = ?",
    )
    .bind(&source_id)
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    if owned.is_none() {
        return Redirect::to("/dashboard").into_response();
    }

    let href = if form.calendar_href.is_empty() {
        None
    } else {
        Some(form.calendar_href)
    };

    let _ = sqlx::query("UPDATE caldav_sources SET write_calendar_href = ? WHERE id = ?")
        .bind(&href)
        .bind(&source_id)
        .execute(&state.pool)
        .await;

    Redirect::to("/dashboard").into_response()
}

fn render_event_type_form_error(state: &AppState, error: &str, form: &EventTypeForm, editing: bool) -> Html<String> {
    let tmpl = match state.templates.get_template("event_type_form.html") {
        Ok(t) => t,
        Err(e) => return Html(format!("Template error: {}", e)),
    };

    Html(
        tmpl.render(context! {
            editing => editing,
            form_title => form.title.as_str(),
            form_slug => form.slug.as_str(),
            form_description => form.description.as_deref().unwrap_or(""),
            form_duration => form.duration_min,
            form_buffer_before => form.buffer_before.unwrap_or(0),
            form_buffer_after => form.buffer_after.unwrap_or(0),
            form_min_notice => form.min_notice_min.unwrap_or(60),
            form_requires_confirmation => form.requires_confirmation.as_deref() == Some("on"),
            form_location_type => form.location_type.as_deref().unwrap_or("link"),
            form_location_value => form.location_value.as_deref().unwrap_or(""),
            form_avail_days => form.avail_days.as_deref().unwrap_or("1,2,3,4,5"),
            form_avail_start => form.avail_start.as_deref().unwrap_or("09:00"),
            form_avail_end => form.avail_end.as_deref().unwrap_or("17:00"),
            error => error,
        })
        .unwrap_or_else(|e| format!("Template error: {}", e)),
    )
}

// --- Group event type handlers ---

async fn new_group_event_type_form(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
) -> impl IntoResponse {
    let user = &auth_user.0;

    let groups: Vec<(String, String)> = sqlx::query_as(
        "SELECT g.id, g.name FROM groups g JOIN user_groups ug ON ug.group_id = g.id WHERE ug.user_id = ? ORDER BY g.name",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    if groups.is_empty() {
        return Html("You don't belong to any groups.".to_string());
    }

    let groups_ctx: Vec<minijinja::Value> = groups
        .iter()
        .map(|(id, name)| context! { id => id, name => name })
        .collect();

    let tmpl = match state.templates.get_template("event_type_form.html") {
        Ok(t) => t,
        Err(e) => return Html(format!("Template error: {}", e)),
    };

    Html(
        tmpl.render(context! {
            editing => false,
            is_group => true,
            groups => groups_ctx,
            form_group_id => groups.first().map(|(id, _)| id.as_str()).unwrap_or(""),
            form_title => "",
            form_slug => "",
            form_description => "",
            form_duration => 30,
            form_buffer_before => 0,
            form_buffer_after => 0,
            form_min_notice => 60,
            form_requires_confirmation => false,
            form_location_type => "link",
            form_location_value => "",
            form_avail_days => "1,2,3,4,5",
            form_avail_start => "09:00",
            form_avail_end => "17:00",
            error => "",
        })
        .unwrap_or_else(|e| format!("Template error: {}", e)),
    )
}

async fn create_group_event_type(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Form(form): Form<EventTypeForm>,
) -> impl IntoResponse {
    let user = &auth_user.0;

    let group_id = match form.group_id.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(gid) => gid.to_string(),
        None => return Redirect::to("/dashboard").into_response(),
    };

    // Verify user belongs to this group
    let membership: Option<(String,)> = sqlx::query_as(
        "SELECT group_id FROM user_groups WHERE user_id = ? AND group_id = ?",
    )
    .bind(&user.id)
    .bind(&group_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    if membership.is_none() {
        return Html("You don't belong to this group.".to_string()).into_response();
    }

    // Find the user's account
    let account_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM accounts WHERE user_id = ? LIMIT 1",
    )
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let account_id = match account_id {
        Some(id) => id,
        None => return Redirect::to("/dashboard").into_response(),
    };

    let slug = form.slug.trim().to_lowercase().replace(' ', "-");
    if slug.is_empty() {
        return Html("Slug is required.".to_string()).into_response();
    }

    // Check uniqueness within the group
    let existing: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM event_types WHERE group_id = ? AND slug = ?",
    )
    .bind(&group_id)
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    if existing.is_some() {
        return Html("An event type with this slug already exists in this group.".to_string()).into_response();
    }

    let et_id = uuid::Uuid::new_v4().to_string();
    let requires_confirmation = form.requires_confirmation.as_deref() == Some("on");
    let location_type = form.location_type.as_deref().unwrap_or("link");
    let location_value = form.location_value.as_deref().filter(|s| !s.trim().is_empty());

    let _ = sqlx::query(
        "INSERT INTO event_types (id, account_id, slug, title, description, duration_min, buffer_before, buffer_after, min_notice_min, requires_confirmation, location_type, location_value, group_id, created_by_user_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&et_id)
    .bind(&account_id)
    .bind(&slug)
    .bind(form.title.trim())
    .bind(form.description.as_deref().filter(|s| !s.trim().is_empty()))
    .bind(form.duration_min)
    .bind(form.buffer_before.unwrap_or(0))
    .bind(form.buffer_after.unwrap_or(0))
    .bind(form.min_notice_min.unwrap_or(60))
    .bind(requires_confirmation as i32)
    .bind(location_type)
    .bind(location_value)
    .bind(&group_id)
    .bind(&user.id)
    .execute(&state.pool)
    .await;

    // Create availability rules
    let avail_days = form.avail_days.as_deref().unwrap_or("1,2,3,4,5");
    let avail_start = form.avail_start.as_deref().unwrap_or("09:00");
    let avail_end = form.avail_end.as_deref().unwrap_or("17:00");

    for day_str in avail_days.split(',') {
        if let Ok(day) = day_str.trim().parse::<i32>() {
            if (0..=6).contains(&day) {
                let rule_id = uuid::Uuid::new_v4().to_string();
                let _ = sqlx::query(
                    "INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time) VALUES (?, ?, ?, ?, ?)",
                )
                .bind(&rule_id)
                .bind(&et_id)
                .bind(day)
                .bind(avail_start)
                .bind(avail_end)
                .execute(&state.pool)
                .await;
            }
        }
    }

    Redirect::to("/dashboard").into_response()
}

// --- Group public pages ---

async fn group_profile(
    State(state): State<Arc<AppState>>,
    Path(group_slug): Path<String>,
) -> impl IntoResponse {
    let group: Option<(String, String)> = sqlx::query_as(
        "SELECT id, name FROM groups WHERE slug = ?",
    )
    .bind(&group_slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (group_id, group_name) = match group {
        Some(g) => g,
        None => return Html("Group not found.".to_string()),
    };

    let event_types: Vec<(String, String, Option<String>, i32)> = sqlx::query_as(
        "SELECT et.slug, et.title, et.description, et.duration_min
         FROM event_types et
         WHERE et.group_id = ? AND et.enabled = 1
         ORDER BY et.created_at",
    )
    .bind(&group_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let tmpl = match state.templates.get_template("group_profile.html") {
        Ok(t) => t,
        Err(e) => return Html(format!("Template error: {}", e)),
    };

    let et_ctx: Vec<minijinja::Value> = event_types
        .iter()
        .map(|(slug, title, desc, duration)| {
            context! { slug => slug, title => title, description => desc, duration_min => duration }
        })
        .collect();

    Html(
        tmpl.render(context! {
            group_name => group_name,
            group_slug => group_slug,
            event_types => et_ctx,
        })
        .unwrap_or_else(|e| format!("Template error: {}", e)),
    )
}

async fn show_group_slots(
    State(state): State<Arc<AppState>>,
    Path((group_slug, slug)): Path<(String, String)>,
    Query(query): Query<SlotsQuery>,
) -> impl IntoResponse {
    let et: Option<(String, String, String, Option<String>, i32, i32, i32, i32, String, Option<String>, String)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.description, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min, et.location_type, et.location_value, g.name
         FROM event_types et
         JOIN groups g ON g.id = et.group_id
         WHERE g.slug = ? AND et.slug = ? AND et.enabled = 1",
    )
    .bind(&group_slug)
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (et_id, et_slug, et_title, et_desc, duration, buf_before, buf_after, min_notice, loc_type, loc_value, group_name) =
        match et {
            Some(e) => e,
            None => return Html("Event type not found.".to_string()),
        };

    let guest_tz = parse_guest_tz(query.tz.as_deref());
    let host_tz = get_host_tz(&state.pool, &et_id).await;
    let guest_tz_name = guest_tz.name().to_string();

    let week = query.week.unwrap_or(0).max(0);
    let days_per_page = 7;
    let start_offset = week * days_per_page;
    let slot_days = compute_group_slots(
        &state.pool, &et_id, duration, buf_before, buf_after, min_notice, start_offset, days_per_page, host_tz, guest_tz,
    )
    .await;
    let prev_week = if week > 0 { Some(week - 1) } else { None };
    let next_week = week + 1;

    let days_ctx: Vec<minijinja::Value> = slot_days
        .iter()
        .map(|d| {
            let slots: Vec<minijinja::Value> = d
                .slots
                .iter()
                .map(|s| context! { start => s.start, end => s.end, host_date => s.host_date, host_time => s.host_time })
                .collect();
            context! { date => d.date, label => d.label, slots => slots }
        })
        .collect();

    let now_guest = Utc::now().with_timezone(&guest_tz).naive_local();
    let range_start = now_guest.date() + Duration::days(start_offset as i64);
    let range_end = now_guest.date() + Duration::days((start_offset + days_per_page - 1) as i64);
    let range_label = format!(
        "{} – {}",
        range_start.format("%b %-d"),
        range_end.format("%b %-d, %Y")
    );

    let tz_options: Vec<minijinja::Value> = common_timezones()
        .iter()
        .map(|(iana, label)| context! { value => iana, label => label, selected => (*iana == guest_tz_name) })
        .collect();

    let tmpl = state.templates.get_template("slots.html").unwrap();
    let rendered = tmpl
        .render(context! {
            event_type => context! {
                slug => et_slug,
                title => et_title,
                description => et_desc,
                duration_min => duration,
                location_type => loc_type,
                location_value => loc_value,
            },
            host_name => group_name,
            group_slug => group_slug,
            days => days_ctx,
            prev_week => prev_week,
            next_week => next_week,
            range_label => range_label,
            guest_tz => guest_tz_name,
            tz_options => tz_options,
        })
        .unwrap_or_else(|e| format!("Template error: {}", e));

    Html(rendered)
}

async fn show_group_book_form(
    State(state): State<Arc<AppState>>,
    Path((group_slug, slug)): Path<(String, String)>,
    Query(query): Query<BookQuery>,
) -> impl IntoResponse {
    let et: Option<(String, String, String, Option<String>, i32, String, Option<String>, String)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.description, et.duration_min, et.location_type, et.location_value, g.name
         FROM event_types et
         JOIN groups g ON g.id = et.group_id
         WHERE g.slug = ? AND et.slug = ? AND et.enabled = 1",
    )
    .bind(&group_slug)
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (_et_id, et_slug, et_title, et_desc, duration, loc_type, loc_value, group_name) = match et {
        Some(e) => e,
        None => return Html("Event type not found.".to_string()),
    };

    let guest_tz = parse_guest_tz(query.tz.as_deref());
    let guest_tz_name = guest_tz.name().to_string();

    let date = NaiveDate::parse_from_str(&query.date, "%Y-%m-%d").unwrap();
    let time = NaiveTime::parse_from_str(&query.time, "%H:%M").unwrap();
    let end_time = (date.and_time(time) + Duration::minutes(duration as i64))
        .time()
        .format("%H:%M")
        .to_string();
    let date_label = date.format("%A, %B %-d, %Y").to_string();

    let tmpl = state.templates.get_template("book.html").unwrap();
    let rendered = tmpl
        .render(context! {
            event_type => context! {
                slug => et_slug,
                title => et_title,
                description => et_desc,
                duration_min => duration,
                location_type => loc_type,
                location_value => loc_value,
            },
            host_name => group_name,
            group_slug => group_slug,
            date => query.date,
            date_label => date_label,
            time_start => query.time,
            time_end => end_time,
            guest_tz => guest_tz_name,
            error => "",
            form_name => "",
            form_email => "",
            form_notes => "",
        })
        .unwrap_or_else(|e| format!("Template error: {}", e));

    Html(rendered)
}

async fn handle_group_booking(
    State(state): State<Arc<AppState>>,
    Path((group_slug, slug)): Path<(String, String)>,
    Form(form): Form<BookForm>,
) -> impl IntoResponse {
    let et: Option<(String, String, String, i32, i32, i32, i32, i32, String, Option<String>, String)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min, et.requires_confirmation, et.location_type, et.location_value, et.group_id
         FROM event_types et
         JOIN groups g ON g.id = et.group_id
         WHERE g.slug = ? AND et.slug = ? AND et.enabled = 1",
    )
    .bind(&group_slug)
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (et_id, _et_slug, et_title, duration, buffer_before, buffer_after, min_notice, requires_confirmation, loc_type, loc_value, group_id) = match et {
        Some(e) => e,
        None => return Html("Event type not found.".to_string()).into_response(),
    };
    let needs_approval = requires_confirmation != 0;

    let date = match NaiveDate::parse_from_str(&form.date, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return Html("Invalid date.".to_string()).into_response(),
    };
    let start_time = match NaiveTime::parse_from_str(&form.time, "%H:%M") {
        Ok(t) => t,
        Err(_) => return Html("Invalid time.".to_string()).into_response(),
    };

    let slot_start = date.and_time(start_time);
    let slot_end = slot_start + Duration::minutes(duration as i64);

    let now = Local::now().naive_local();
    if slot_start < now + Duration::minutes(min_notice as i64) {
        return Html("This slot is no longer available (too soon).".to_string()).into_response();
    }

    // Pick an available group member
    let assigned = pick_group_member(
        &state.pool,
        &group_id,
        slot_start,
        slot_end,
        buffer_before,
        buffer_after,
    )
    .await;

    let (assigned_user_id, host_name, host_email) = match assigned {
        Some(a) => a,
        None => return Html("No team members are available for this slot.".to_string()).into_response(),
    };

    let id = uuid::Uuid::new_v4().to_string();
    let uid = format!("{}@calrs", uuid::Uuid::new_v4());
    let cancel_token = uuid::Uuid::new_v4().to_string();
    let reschedule_token = uuid::Uuid::new_v4().to_string();
    let start_at = slot_start.format("%Y-%m-%dT%H:%M:%S").to_string();
    let end_at = slot_end.format("%Y-%m-%dT%H:%M:%S").to_string();
    let guest_tz = parse_guest_tz(form.tz.as_deref());
    let guest_timezone = guest_tz.name().to_string();

    let initial_status = if needs_approval { "pending" } else { "confirmed" };

    sqlx::query(
        "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, notes, start_at, end_at, status, cancel_token, reschedule_token, assigned_user_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
    .execute(&state.pool)
    .await
    .unwrap();

    // Send emails if SMTP is configured
    if let Ok(Some(smtp_config)) = crate::email::load_smtp_config(&state.pool).await {
        let location_display = if loc_value.as_ref().is_some_and(|v| !v.is_empty()) {
            loc_value.clone()
        } else {
            None
        };
        let details = crate::email::BookingDetails {
            event_title: et_title.clone(),
            date: form.date.clone(),
            start_time: form.time.clone(),
            end_time: slot_end.time().format("%H:%M").to_string(),
            guest_name: form.name.clone(),
            guest_email: form.email.clone(),
            guest_timezone: guest_timezone.clone(),
            host_name: host_name.clone(),
            host_email: host_email.clone(),
            uid: uid.clone(),
            notes: form.notes.clone(),
            location: location_display,
        };

        if needs_approval {
            let _ = crate::email::send_host_approval_request(&smtp_config, &details, &id).await;
            let _ = crate::email::send_guest_pending_notice(&smtp_config, &details).await;
        } else {
            let _ = crate::email::send_guest_confirmation(&smtp_config, &details).await;
            let _ = crate::email::send_host_notification(&smtp_config, &details).await;
            // Push confirmed booking to assigned member's CalDAV
            caldav_push_booking(&state.pool, &assigned_user_id, &uid, &details).await;
        }
    }

    let date_label = date.format("%A, %B %-d, %Y").to_string();
    let end_time_str = slot_end.time().format("%H:%M").to_string();

    let tmpl = state.templates.get_template("confirmed.html").unwrap();
    let rendered = tmpl
        .render(context! {
            event_title => et_title,
            date_label => date_label,
            time_start => form.time,
            time_end => end_time_str,
            host_name => host_name,
            guest_email => form.email,
            notes => form.notes,
            pending => needs_approval,
            location_type => loc_type,
            location_value => loc_value,
        })
        .unwrap_or_else(|e| format!("Template error: {}", e));

    Html(rendered).into_response()
}

// --- Group slot computation ---

/// Compute available slots for a group event type.
/// A slot is available if ANY group member is free during that time.
async fn compute_group_slots(
    pool: &SqlitePool,
    et_id: &str,
    duration: i32,
    buffer_before: i32,
    buffer_after: i32,
    min_notice: i32,
    start_offset: i32,
    days_ahead: i32,
    host_tz: Tz,
    guest_tz: Tz,
) -> Vec<SlotDay> {
    // Get availability rules for this event type
    let rules: Vec<(i32, String, String)> = sqlx::query_as(
        "SELECT day_of_week, start_time, end_time FROM availability_rules WHERE event_type_id = ?",
    )
    .bind(et_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // Get the group_id for this event type
    let group_id: Option<String> = sqlx::query_scalar(
        "SELECT group_id FROM event_types WHERE id = ?",
    )
    .bind(et_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
    .flatten();

    let group_id = match group_id {
        Some(gid) => gid,
        None => return Vec::new(),
    };

    // Get all enabled group members
    let members: Vec<(String,)> = sqlx::query_as(
        "SELECT u.id FROM users u JOIN user_groups ug ON ug.user_id = u.id WHERE ug.group_id = ? AND u.enabled = 1",
    )
    .bind(&group_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    if members.is_empty() {
        return Vec::new();
    }

    let now_host = Utc::now().with_timezone(&host_tz).naive_local();
    let now = now_host;
    let min_start = now + Duration::minutes(min_notice as i64);
    let end_date = now.date() + Duration::days((start_offset + days_ahead) as i64);

    let end_compact = end_date.format("%Y%m%d").to_string();
    let now_compact = now.format("%Y%m%dT%H%M%S").to_string();
    let end_iso = end_date.format("%Y-%m-%dT23:59:59").to_string();
    let now_iso = now.format("%Y-%m-%dT%H:%M:%S").to_string();

    // Pre-fetch busy times for all members
    let mut member_busy: std::collections::HashMap<String, Vec<(NaiveDateTime, NaiveDateTime)>> = std::collections::HashMap::new();

    for (user_id,) in &members {
        let mut busy_times = Vec::new();

        // Non-recurring events from their CalDAV calendars
        let events: Vec<(String, String)> = sqlx::query_as(
            "SELECT e.start_at, e.end_at FROM events e
             JOIN calendars c ON c.id = e.calendar_id
             JOIN caldav_sources cs ON cs.id = c.source_id
             JOIN accounts a ON a.id = cs.account_id
             WHERE a.user_id = ? AND c.is_busy = 1
               AND (e.rrule IS NULL OR e.rrule = '')
               AND ((e.start_at <= ? AND e.end_at >= ?) OR (e.start_at <= ? AND e.end_at >= ?))",
        )
        .bind(user_id)
        .bind(&end_compact)
        .bind(&now_compact)
        .bind(&end_iso)
        .bind(&now_iso)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        for (s, e) in &events {
            if let (Some(start), Some(end)) = (parse_datetime(s), parse_datetime(e)) {
                busy_times.push((start, end));
            }
        }

        // Recurring events from their CalDAV calendars
        let end_compact_member = end_date.format("%Y%m%dT235959").to_string();
        let recurring_events: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT e.start_at, e.end_at, e.rrule, e.raw_ical FROM events e
             JOIN calendars c ON c.id = e.calendar_id
             JOIN caldav_sources cs ON cs.id = c.source_id
             JOIN accounts a ON a.id = cs.account_id
             WHERE a.user_id = ? AND c.is_busy = 1
               AND e.rrule IS NOT NULL AND e.rrule != '' AND (e.start_at <= ? OR e.start_at <= ?)",
        )
        .bind(user_id)
        .bind(&end_iso)
        .bind(&end_compact_member)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        let window_end_dt = end_date.and_hms_opt(23, 59, 59).unwrap_or(now);
        for (s, e, rrule_str, raw_ical) in &recurring_events {
            if let (Some(ev_start), Some(ev_end)) = (parse_datetime(s), parse_datetime(e)) {
                let exdates = raw_ical.as_deref().map(crate::rrule::extract_exdates).unwrap_or_default();
                let occurrences = crate::rrule::expand_rrule(ev_start, ev_end, &rrule_str, &exdates, now, window_end_dt);
                for (os, oe) in occurrences {
                    busy_times.push((os, oe));
                }
            }
        }

        // Confirmed bookings assigned to or owned by this member
        let bookings: Vec<(String, String)> = sqlx::query_as(
            "SELECT b.start_at, b.end_at FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             JOIN accounts a ON a.id = et.account_id
             WHERE (a.user_id = ? OR b.assigned_user_id = ?) AND b.status = 'confirmed'
               AND b.start_at <= ? AND b.end_at >= ?",
        )
        .bind(user_id)
        .bind(user_id)
        .bind(&end_iso)
        .bind(&now_iso)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        for (s, e) in &bookings {
            if let (Some(start), Some(end)) = (parse_datetime(s), parse_datetime(e)) {
                busy_times.push((start, end));
            }
        }

        member_busy.insert(user_id.clone(), busy_times);
    }

    let slot_duration = Duration::minutes(duration as i64);
    let mut result = Vec::new();

    for day_offset in start_offset..(start_offset + days_ahead) {
        let date = now.date() + Duration::days(day_offset as i64);
        let weekday = date.weekday().num_days_from_sunday() as i32;

        let day_rules: Vec<&(i32, String, String)> = rules
            .iter()
            .filter(|(d, _, _)| *d == weekday)
            .collect();

        if day_rules.is_empty() {
            continue;
        }

        let mut day_slots = Vec::new();

        for (_, start_str, end_str) in &day_rules {
            let window_start = match NaiveTime::parse_from_str(start_str, "%H:%M") {
                Ok(t) => t,
                Err(_) => continue,
            };
            let window_end = match NaiveTime::parse_from_str(end_str, "%H:%M") {
                Ok(t) => t,
                Err(_) => continue,
            };

            let mut cursor = window_start;
            while cursor + slot_duration <= window_end {
                let slot_start = date.and_time(cursor);
                let slot_end = slot_start + slot_duration;

                if slot_start < min_start {
                    cursor = cursor + Duration::minutes(duration as i64);
                    continue;
                }

                let buf_start = slot_start - Duration::minutes(buffer_before as i64);
                let buf_end = slot_end + Duration::minutes(buffer_after as i64);

                // A slot is available if ANY member is free
                let any_member_free = member_busy.iter().any(|(_uid, busy_times)| {
                    !busy_times.iter().any(|(s, e)| *s < buf_end && *e > buf_start)
                });

                if any_member_free {
                    let slot_start_utc = host_tz.from_local_datetime(&slot_start).earliest().unwrap_or_else(|| host_tz.from_utc_datetime(&slot_start)).with_timezone(&Utc);
                    let slot_end_utc = host_tz.from_local_datetime(&slot_end).earliest().unwrap_or_else(|| host_tz.from_utc_datetime(&slot_end)).with_timezone(&Utc);
                    let guest_start = slot_start_utc.with_timezone(&guest_tz);
                    let guest_end = slot_end_utc.with_timezone(&guest_tz);

                    day_slots.push(SlotTime {
                        start: guest_start.format("%H:%M").to_string(),
                        end: guest_end.format("%H:%M").to_string(),
                        host_date: date.format("%Y-%m-%d").to_string(),
                        host_time: cursor.format("%H:%M").to_string(),
                        guest_date: guest_start.format("%Y-%m-%d").to_string(),
                    });
                }

                cursor = cursor + Duration::minutes(duration as i64);
            }
        }

        if !day_slots.is_empty() {
            let mut guest_days: std::collections::BTreeMap<String, Vec<SlotTime>> = std::collections::BTreeMap::new();
            for slot in day_slots {
                guest_days.entry(slot.guest_date.clone()).or_default().push(slot);
            }
            for (guest_date_str, slots) in guest_days {
                if let Ok(gd) = NaiveDate::parse_from_str(&guest_date_str, "%Y-%m-%d") {
                    if !result.iter().any(|d: &SlotDay| d.date == guest_date_str) {
                        result.push(SlotDay {
                            date: guest_date_str,
                            label: gd.format("%A, %B %-d").to_string(),
                            slots,
                        });
                    } else if let Some(existing) = result.iter_mut().find(|d: &&mut SlotDay| d.date == guest_date_str) {
                        existing.slots.extend(slots);
                    }
                }
            }
        }
    }

    result.sort_by(|a, b| a.date.cmp(&b.date));
    result
}

/// Pick an available group member for a booking slot.
/// Returns (user_id, name, email) of the member with fewest recent bookings.
async fn pick_group_member(
    pool: &SqlitePool,
    group_id: &str,
    slot_start: NaiveDateTime,
    slot_end: NaiveDateTime,
    buffer_before: i32,
    buffer_after: i32,
) -> Option<(String, String, String)> {
    let buf_start = slot_start - Duration::minutes(buffer_before as i64);
    let buf_end = slot_end + Duration::minutes(buffer_after as i64);

    // Get all enabled group members
    let members: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT u.id, u.name, u.email FROM users u JOIN user_groups ug ON ug.user_id = u.id WHERE ug.group_id = ? AND u.enabled = 1",
    )
    .bind(group_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut available_members = Vec::new();

    for (user_id, name, email) in &members {
        // Check non-recurring CalDAV events
        let event_conflict: Option<(String,)> = sqlx::query_as(
            "SELECT e.id FROM events e
             JOIN calendars c ON c.id = e.calendar_id
             JOIN caldav_sources cs ON cs.id = c.source_id
             JOIN accounts a ON a.id = cs.account_id
             WHERE a.user_id = ? AND c.is_busy = 1
               AND (e.rrule IS NULL OR e.rrule = '')
               AND ((e.start_at < ? AND e.end_at > ?) OR (e.start_at < ? AND e.end_at > ?))
             LIMIT 1",
        )
        .bind(user_id)
        .bind(&buf_end.format("%Y%m%dT%H%M%S").to_string())
        .bind(&buf_start.format("%Y%m%dT%H%M%S").to_string())
        .bind(&buf_end.format("%Y-%m-%dT%H:%M:%S").to_string())
        .bind(&buf_start.format("%Y-%m-%dT%H:%M:%S").to_string())
        .fetch_optional(pool)
        .await
        .unwrap_or(None);

        if event_conflict.is_some() {
            continue;
        }

        // Check recurring CalDAV events
        let recurring_events: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT e.start_at, e.end_at, e.rrule, e.raw_ical FROM events e
             JOIN calendars c ON c.id = e.calendar_id
             JOIN caldav_sources cs ON cs.id = c.source_id
             JOIN accounts a ON a.id = cs.account_id
             WHERE a.user_id = ? AND c.is_busy = 1
               AND e.rrule IS NOT NULL AND e.rrule != ''",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        let mut recurring_conflict = false;
        for (s, e, rrule_str, raw_ical) in &recurring_events {
            if let (Some(ev_start), Some(ev_end)) = (parse_datetime(s), parse_datetime(e)) {
                let exdates = raw_ical.as_deref().map(crate::rrule::extract_exdates).unwrap_or_default();
                let occurrences = crate::rrule::expand_rrule(ev_start, ev_end, &rrule_str, &exdates, buf_start, buf_end);
                if occurrences.iter().any(|(os, oe)| *os < buf_end && *oe > buf_start) {
                    recurring_conflict = true;
                    break;
                }
            }
        }

        if recurring_conflict {
            continue;
        }

        // Check booking conflicts
        let booking_conflict: Option<(String,)> = sqlx::query_as(
            "SELECT b.id FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             JOIN accounts a ON a.id = et.account_id
             WHERE (a.user_id = ? OR b.assigned_user_id = ?) AND b.status = 'confirmed'
               AND b.start_at < ? AND b.end_at > ?
             LIMIT 1",
        )
        .bind(user_id)
        .bind(user_id)
        .bind(&buf_end.format("%Y-%m-%dT%H:%M:%S").to_string())
        .bind(&buf_start.format("%Y-%m-%dT%H:%M:%S").to_string())
        .fetch_optional(pool)
        .await
        .unwrap_or(None);

        if booking_conflict.is_some() {
            continue;
        }

        available_members.push((user_id.clone(), name.clone(), email.clone()));
    }

    if available_members.is_empty() {
        return None;
    }

    // Among available members, pick the one with fewest bookings in last 30 days
    let thirty_days_ago = (Utc::now() - Duration::days(30)).format("%Y-%m-%dT%H:%M:%S").to_string();
    let mut best: Option<(String, String, String, i64)> = None;

    for (user_id, name, email) in &available_members {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM bookings WHERE assigned_user_id = ? AND created_at >= ?",
        )
        .bind(user_id)
        .bind(&thirty_days_ago)
        .fetch_one(pool)
        .await
        .unwrap_or(0);

        match &best {
            None => best = Some((user_id.clone(), name.clone(), email.clone(), count)),
            Some((_, _, _, best_count)) if count < *best_count => {
                best = Some((user_id.clone(), name.clone(), email.clone(), count));
            }
            _ => {}
        }
    }

    best.map(|(uid, name, email, _)| (uid, name, email))
}

// --- User profile page ---

async fn user_profile(
    State(state): State<Arc<AppState>>,
    Path(username): Path<String>,
) -> impl IntoResponse {
    let user: Option<(String, String)> = sqlx::query_as(
        "SELECT id, name FROM users WHERE username = ? AND enabled = 1",
    )
    .bind(&username)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (user_id, user_name) = match user {
        Some(u) => u,
        None => return Html("User not found.".to_string()),
    };

    let event_types: Vec<(String, String, Option<String>, i32)> = sqlx::query_as(
        "SELECT et.slug, et.title, et.description, et.duration_min
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         WHERE a.user_id = ? AND et.enabled = 1
         ORDER BY et.created_at",
    )
    .bind(&user_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let tmpl = match state.templates.get_template("profile.html") {
        Ok(t) => t,
        Err(e) => return Html(format!("Template error: {}", e)),
    };

    let et_ctx: Vec<minijinja::Value> = event_types
        .iter()
        .map(|(slug, title, desc, duration)| {
            context! { slug => slug, title => title, description => desc, duration_min => duration }
        })
        .collect();

    Html(
        tmpl.render(context! {
            host_name => user_name,
            username => username,
            event_types => et_ctx,
        })
        .unwrap_or_else(|e| format!("Template error: {}", e)),
    )
}

// --- User-scoped booking handlers ---

async fn show_slots_for_user(
    State(state): State<Arc<AppState>>,
    Path((username, slug)): Path<(String, String)>,
    Query(query): Query<SlotsQuery>,
) -> impl IntoResponse {
    let et: Option<(String, String, String, Option<String>, i32, i32, i32, i32, String, Option<String>)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.description, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min, et.location_type, et.location_value
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

    let (et_id, et_slug, et_title, et_desc, duration, buf_before, buf_after, min_notice, loc_type, loc_value) =
        match et {
            Some(e) => e,
            None => return Html("Event type not found.".to_string()),
        };

    let host_name: String = sqlx::query_scalar(
        "SELECT name FROM users WHERE username = ?",
    )
    .bind(&username)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .unwrap_or_else(|| "Host".to_string());

    let guest_tz = parse_guest_tz(query.tz.as_deref());
    let host_tz = get_host_tz(&state.pool, &et_id).await;
    let guest_tz_name = guest_tz.name().to_string();

    let week = query.week.unwrap_or(0).max(0);
    let days_per_page = 7;
    let start_offset = week * days_per_page;
    let slot_days = compute_slots(
        &state.pool, &et_id, duration, buf_before, buf_after, min_notice, start_offset, days_per_page, host_tz, guest_tz,
    )
    .await;
    let prev_week = if week > 0 { Some(week - 1) } else { None };
    let next_week = week + 1;

    let days_ctx: Vec<minijinja::Value> = slot_days
        .iter()
        .map(|d| {
            let slots: Vec<minijinja::Value> = d
                .slots
                .iter()
                .map(|s| context! { start => s.start, end => s.end, host_date => s.host_date, host_time => s.host_time })
                .collect();
            context! { date => d.date, label => d.label, slots => slots }
        })
        .collect();

    let now_guest = Utc::now().with_timezone(&guest_tz).naive_local();
    let range_start = now_guest.date() + Duration::days(start_offset as i64);
    let range_end = now_guest.date() + Duration::days((start_offset + days_per_page - 1) as i64);
    let range_label = format!(
        "{} – {}",
        range_start.format("%b %-d"),
        range_end.format("%b %-d, %Y")
    );

    let tz_options: Vec<minijinja::Value> = common_timezones()
        .iter()
        .map(|(iana, label)| context! { value => iana, label => label, selected => (*iana == guest_tz_name) })
        .collect();

    let tmpl = state.templates.get_template("slots.html").unwrap();
    let rendered = tmpl
        .render(context! {
            event_type => context! {
                slug => et_slug,
                title => et_title,
                description => et_desc,
                duration_min => duration,
                location_type => loc_type,
                location_value => loc_value,
            },
            host_name => host_name,
            username => username,
            days => days_ctx,
            prev_week => prev_week,
            next_week => next_week,
            range_label => range_label,
            guest_tz => guest_tz_name,
            tz_options => tz_options,
        })
        .unwrap_or_else(|e| format!("Template error: {}", e));

    Html(rendered)
}

async fn show_book_form_for_user(
    State(state): State<Arc<AppState>>,
    Path((username, slug)): Path<(String, String)>,
    Query(query): Query<BookQuery>,
) -> impl IntoResponse {
    let et: Option<(String, String, String, Option<String>, i32, String, Option<String>)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.description, et.duration_min, et.location_type, et.location_value
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

    let (_et_id, et_slug, et_title, et_desc, duration, loc_type, loc_value) = match et {
        Some(e) => e,
        None => return Html("Event type not found.".to_string()),
    };

    let host_name: String = sqlx::query_scalar(
        "SELECT name FROM users WHERE username = ?",
    )
    .bind(&username)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .unwrap_or_else(|| "Host".to_string());

    let guest_tz = parse_guest_tz(query.tz.as_deref());
    let guest_tz_name = guest_tz.name().to_string();

    let date = NaiveDate::parse_from_str(&query.date, "%Y-%m-%d").unwrap();
    let time = NaiveTime::parse_from_str(&query.time, "%H:%M").unwrap();
    let end_time = (date.and_time(time) + Duration::minutes(duration as i64))
        .time()
        .format("%H:%M")
        .to_string();
    let date_label = date.format("%A, %B %-d, %Y").to_string();

    let tmpl = state.templates.get_template("book.html").unwrap();
    let rendered = tmpl
        .render(context! {
            event_type => context! {
                slug => et_slug,
                title => et_title,
                description => et_desc,
                duration_min => duration,
                location_type => loc_type,
                location_value => loc_value,
            },
            host_name => host_name,
            username => username,
            date => query.date,
            date_label => date_label,
            time_start => query.time,
            time_end => end_time,
            guest_tz => guest_tz_name,
            error => "",
            form_name => "",
            form_email => "",
            form_notes => "",
        })
        .unwrap_or_else(|e| format!("Template error: {}", e));

    Html(rendered)
}

async fn handle_booking_for_user(
    State(state): State<Arc<AppState>>,
    Path((username, slug)): Path<(String, String)>,
    Form(form): Form<BookForm>,
) -> impl IntoResponse {
    let et: Option<(String, String, String, i32, i32, i32, i32, i32, String, Option<String>)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min, et.requires_confirmation, et.location_type, et.location_value
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

    let (et_id, _et_slug, et_title, duration, buffer_before, buffer_after, min_notice, requires_confirmation, loc_type, loc_value) = match et {
        Some(e) => e,
        None => return Html("Event type not found.".to_string()).into_response(),
    };
    let needs_approval = requires_confirmation != 0;

    let date = match NaiveDate::parse_from_str(&form.date, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return Html("Invalid date.".to_string()).into_response(),
    };
    let start_time = match NaiveTime::parse_from_str(&form.time, "%H:%M") {
        Ok(t) => t,
        Err(_) => return Html("Invalid time.".to_string()).into_response(),
    };

    let slot_start = date.and_time(start_time);
    let slot_end = slot_start + Duration::minutes(duration as i64);

    let now = Local::now().naive_local();
    if slot_start < now + Duration::minutes(min_notice as i64) {
        return Html("This slot is no longer available (too soon).".to_string()).into_response();
    }

    let buf_start = slot_start - Duration::minutes(buffer_before as i64);
    let buf_end = slot_end + Duration::minutes(buffer_after as i64);

    // Non-recurring events + bookings
    let mut busy: Vec<(String, String)> = sqlx::query_as(
        "SELECT start_at, end_at FROM events
         WHERE (rrule IS NULL OR rrule = '')
         UNION ALL
         SELECT start_at, end_at FROM bookings WHERE status = 'confirmed'",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    // Recurring events — expand and check
    let recurring: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT start_at, end_at, rrule, raw_ical FROM events
         WHERE rrule IS NOT NULL AND rrule != ''",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let window_start = buf_start;
    let window_end = buf_end;
    for (s, e, rrule_str, raw_ical) in &recurring {
        if let (Some(ev_start), Some(ev_end)) = (parse_datetime(s), parse_datetime(e)) {
            let exdates = raw_ical.as_deref().map(crate::rrule::extract_exdates).unwrap_or_default();
            let occurrences = crate::rrule::expand_rrule(ev_start, ev_end, &rrule_str, &exdates, window_start, window_end);
            for (os, oe) in occurrences {
                busy.push((os.format("%Y-%m-%dT%H:%M:%S").to_string(), oe.format("%Y-%m-%dT%H:%M:%S").to_string()));
            }
        }
    }

    for (bs, be) in &busy {
        if let (Some(s), Some(e)) = (parse_datetime(bs), parse_datetime(be)) {
            if s < buf_end && e > buf_start {
                return Html("This slot is no longer available.".to_string()).into_response();
            }
        }
    }

    let id = uuid::Uuid::new_v4().to_string();
    let uid = format!("{}@calrs", uuid::Uuid::new_v4());
    let cancel_token = uuid::Uuid::new_v4().to_string();
    let reschedule_token = uuid::Uuid::new_v4().to_string();
    let start_at = slot_start.format("%Y-%m-%dT%H:%M:%S").to_string();
    let end_at = slot_end.format("%Y-%m-%dT%H:%M:%S").to_string();
    let guest_tz = parse_guest_tz(form.tz.as_deref());
    let guest_timezone = guest_tz.name().to_string();

    let initial_status = if needs_approval { "pending" } else { "confirmed" };

    sqlx::query(
        "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, notes, start_at, end_at, status, cancel_token, reschedule_token)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
    .execute(&state.pool)
    .await
    .unwrap();

    // Send emails if SMTP is configured
    if let Ok(Some(smtp_config)) = crate::email::load_smtp_config(&state.pool).await {
        let host: Option<(String, String)> = sqlx::query_as(
            "SELECT u.name, u.email FROM users u WHERE u.username = ?",
        )
        .bind(&username)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

        if let Some((host_name, host_email)) = host {
            let location_display = if loc_value.as_ref().is_some_and(|v| !v.is_empty()) {
                loc_value.clone()
            } else {
                None
            };
            let details = crate::email::BookingDetails {
                event_title: et_title.clone(),
                date: form.date.clone(),
                start_time: form.time.clone(),
                end_time: slot_end.time().format("%H:%M").to_string(),
                guest_name: form.name.clone(),
                guest_email: form.email.clone(),
                guest_timezone: guest_timezone.clone(),
                host_name,
                host_email,
                uid: uid.clone(),
                notes: form.notes.clone(),
                location: location_display,
            };

            if needs_approval {
                // Send approval request to host, pending notice to guest
                let _ = crate::email::send_host_approval_request(&smtp_config, &details, &id).await;
                let _ = crate::email::send_guest_pending_notice(&smtp_config, &details).await;
            } else {
                let _ = crate::email::send_guest_confirmation(&smtp_config, &details).await;
                let _ = crate::email::send_host_notification(&smtp_config, &details).await;
                // Push confirmed booking to CalDAV
                let host_user_id: Option<String> = sqlx::query_scalar(
                    "SELECT id FROM users WHERE username = ?",
                )
                .bind(&username)
                .fetch_optional(&state.pool)
                .await
                .unwrap_or(None);
                if let Some(uid_user) = host_user_id {
                    caldav_push_booking(&state.pool, &uid_user, &uid, &details).await;
                }
            }
        }
    }

    let host_name: String = sqlx::query_scalar(
        "SELECT name FROM users WHERE username = ?",
    )
    .bind(&username)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .unwrap_or_else(|| "Host".to_string());

    let date_label = date.format("%A, %B %-d, %Y").to_string();
    let end_time_str = slot_end.time().format("%H:%M").to_string();

    let tmpl = state.templates.get_template("confirmed.html").unwrap();
    let rendered = tmpl
        .render(context! {
            event_title => et_title,
            date_label => date_label,
            time_start => form.time,
            time_end => end_time_str,
            host_name => host_name,
            guest_email => form.email,
            notes => form.notes,
            pending => needs_approval,
            location_type => loc_type,
            location_value => loc_value,
        })
        .unwrap_or_else(|e| format!("Template error: {}", e));

    Html(rendered).into_response()
}

// --- Slot computation (shared with CLI) ---

fn parse_datetime(s: &str) -> Option<NaiveDateTime> {
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%S") {
        return Some(dt);
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Some(dt);
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y%m%d") {
        return Some(d.and_hms_opt(0, 0, 0)?);
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(d.and_hms_opt(0, 0, 0)?);
    }
    None
}

fn extract_ical_field(ical: &str, field: &str) -> Option<String> {
    let vevent_start = ical.find("BEGIN:VEVENT")?;
    let vevent_end = ical[vevent_start..].find("END:VEVENT")
        .map(|i| vevent_start + i)
        .unwrap_or(ical.len());
    let vevent = &ical[vevent_start..vevent_end];

    for line in vevent.lines() {
        if line.starts_with(field) {
            if let Some(colon_pos) = line.find(':') {
                let value = line[colon_pos + 1..].trim().to_string();
                if !value.is_empty() {
                    return Some(value);
                }
            }
        }
    }
    None
}

struct SlotDay {
    date: String,
    label: String,
    slots: Vec<SlotTime>,
}

struct SlotTime {
    start: String,     // guest TZ display
    end: String,       // guest TZ display
    host_date: String, // YYYY-MM-DD in host TZ (for booking)
    host_time: String, // HH:MM in host TZ (for booking)
    guest_date: String, // YYYY-MM-DD in guest TZ (for grouping by day)
}

async fn compute_slots(
    pool: &SqlitePool,
    et_id: &str,
    duration: i32,
    buffer_before: i32,
    buffer_after: i32,
    min_notice: i32,
    start_offset: i32,
    days_ahead: i32,
    host_tz: Tz,
    guest_tz: Tz,
) -> Vec<SlotDay> {
    let rules: Vec<(i32, String, String)> = sqlx::query_as(
        "SELECT day_of_week, start_time, end_time FROM availability_rules WHERE event_type_id = ?",
    )
    .bind(et_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // Work in host timezone for availability rules
    let now_host = Utc::now().with_timezone(&host_tz).naive_local();
    let now = now_host;
    let min_start = now + Duration::minutes(min_notice as i64);
    let end_date = now.date() + Duration::days((start_offset + days_ahead) as i64);

    let end_compact = end_date.format("%Y%m%d").to_string();
    let now_compact = now.format("%Y%m%dT%H%M%S").to_string();
    let end_iso = end_date.format("%Y-%m-%dT23:59:59").to_string();
    let now_iso = now.format("%Y-%m-%dT%H:%M:%S").to_string();

    // Non-recurring events in range
    let non_recurring: Vec<(String, String)> = sqlx::query_as(
        "SELECT start_at, end_at FROM events
         WHERE (rrule IS NULL OR rrule = '')
           AND ((start_at <= ? AND end_at >= ?) OR (start_at <= ? AND end_at >= ?))
         UNION ALL
         SELECT start_at, end_at FROM bookings
         WHERE status = 'confirmed'
           AND start_at <= ? AND end_at >= ?
         ORDER BY start_at",
    )
    .bind(&end_compact)
    .bind(&now_compact)
    .bind(&end_iso)
    .bind(&now_iso)
    .bind(&end_iso)
    .bind(&now_iso)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // Recurring events — expand into the window
    let end_compact = end_date.format("%Y%m%dT235959").to_string();
    let recurring: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT start_at, end_at, rrule, raw_ical FROM events
         WHERE rrule IS NOT NULL AND rrule != '' AND (start_at <= ? OR start_at <= ?)",
    )
    .bind(&end_iso)
    .bind(&end_compact)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let window_start = now;
    let window_end = end_date.and_hms_opt(23, 59, 59).unwrap_or(now);

    let mut busy: Vec<(String, String)> = non_recurring;
    for (s, e, rrule_str, raw_ical) in &recurring {
        if let (Some(ev_start), Some(ev_end)) = (parse_datetime(s), parse_datetime(e)) {
            let exdates = raw_ical.as_deref().map(crate::rrule::extract_exdates).unwrap_or_default();
            let occurrences = crate::rrule::expand_rrule(ev_start, ev_end, rrule_str, &exdates, window_start, window_end);
            for (os, oe) in occurrences {
                busy.push((os.format("%Y-%m-%dT%H:%M:%S").to_string(), oe.format("%Y-%m-%dT%H:%M:%S").to_string()));
            }
        }
    }

    let slot_duration = Duration::minutes(duration as i64);
    let mut result = Vec::new();

    for day_offset in start_offset..(start_offset + days_ahead) {
        let date = now.date() + Duration::days(day_offset as i64);
        let weekday = date.weekday().num_days_from_sunday() as i32;

        let day_rules: Vec<&(i32, String, String)> = rules
            .iter()
            .filter(|(d, _, _)| *d == weekday)
            .collect();

        if day_rules.is_empty() {
            continue;
        }

        let mut day_slots = Vec::new();

        for (_, start_str, end_str) in &day_rules {
            let window_start = match NaiveTime::parse_from_str(start_str, "%H:%M") {
                Ok(t) => t,
                Err(_) => continue,
            };
            let window_end = match NaiveTime::parse_from_str(end_str, "%H:%M") {
                Ok(t) => t,
                Err(_) => continue,
            };

            let mut cursor = window_start;
            while cursor + slot_duration <= window_end {
                let slot_start = date.and_time(cursor);
                let slot_end = slot_start + slot_duration;

                if slot_start < min_start {
                    cursor = cursor + Duration::minutes(duration as i64);
                    continue;
                }

                let buf_start = slot_start - Duration::minutes(buffer_before as i64);
                let buf_end = slot_end + Duration::minutes(buffer_after as i64);

                let has_conflict = busy.iter().any(|(bs, be)| {
                    let ev_start = parse_datetime(bs);
                    let ev_end = parse_datetime(be);
                    match (ev_start, ev_end) {
                        (Some(s), Some(e)) => s < buf_end && e > buf_start,
                        _ => false,
                    }
                });

                if !has_conflict {
                    // Convert from host TZ to guest TZ for display
                    let slot_start_utc = host_tz.from_local_datetime(&slot_start).earliest().unwrap_or_else(|| host_tz.from_utc_datetime(&slot_start)).with_timezone(&Utc);
                    let slot_end_utc = host_tz.from_local_datetime(&slot_end).earliest().unwrap_or_else(|| host_tz.from_utc_datetime(&slot_end)).with_timezone(&Utc);
                    let guest_start = slot_start_utc.with_timezone(&guest_tz);
                    let guest_end = slot_end_utc.with_timezone(&guest_tz);

                    day_slots.push(SlotTime {
                        start: guest_start.format("%H:%M").to_string(),
                        end: guest_end.format("%H:%M").to_string(),
                        // Store host-TZ date/time for the booking form (the server works in host TZ)
                        host_date: date.format("%Y-%m-%d").to_string(),
                        host_time: cursor.format("%H:%M").to_string(),
                        guest_date: guest_start.format("%Y-%m-%d").to_string(),
                    });
                }

                cursor = cursor + Duration::minutes(duration as i64);
            }
        }

        if !day_slots.is_empty() {
            // Group slots by guest date (may differ from host date due to TZ offset)
            let mut guest_days: std::collections::BTreeMap<String, Vec<SlotTime>> = std::collections::BTreeMap::new();
            for slot in day_slots {
                guest_days.entry(slot.guest_date.clone()).or_default().push(slot);
            }
            for (guest_date_str, slots) in guest_days {
                if let Ok(gd) = NaiveDate::parse_from_str(&guest_date_str, "%Y-%m-%d") {
                    // Only add if we haven't already added this guest date
                    if !result.iter().any(|d: &SlotDay| d.date == guest_date_str) {
                        result.push(SlotDay {
                            date: guest_date_str,
                            label: gd.format("%A, %B %-d").to_string(),
                            slots,
                        });
                    } else {
                        // Merge slots into existing day
                        if let Some(existing) = result.iter_mut().find(|d| d.date == guest_date_str) {
                            existing.slots.extend(slots);
                        }
                    }
                }
            }
        }
    }

    // Sort by date since guest TZ conversion may reorder
    result.sort_by(|a, b| a.date.cmp(&b.date));
    result
}

// --- Handlers ---

#[derive(Deserialize)]
struct SlotsQuery {
    #[serde(default)]
    week: Option<i32>,
    #[serde(default)]
    tz: Option<String>,
}

/// Parse a timezone string into a Tz, falling back to server local.
fn parse_guest_tz(tz: Option<&str>) -> Tz {
    tz.and_then(|s| s.parse::<Tz>().ok())
        .unwrap_or_else(|| {
            // Fall back to server's local timezone
            iana_time_zone::get_timezone()
                .ok()
                .and_then(|s| s.parse::<Tz>().ok())
                .unwrap_or(Tz::UTC)
        })
}

/// Get the host's timezone (uses server local TZ as proxy).
async fn get_host_tz(_pool: &SqlitePool, _et_id: &str) -> Tz {
    iana_time_zone::get_timezone()
        .ok()
        .and_then(|s| s.parse::<Tz>().ok())
        .unwrap_or(Tz::UTC)
}

/// Common IANA timezones for the selector (most used ones).
fn common_timezones() -> Vec<(&'static str, &'static str)> {
    vec![
        ("Pacific/Midway", "UTC-11 Midway"),
        ("Pacific/Honolulu", "UTC-10 Hawaii"),
        ("America/Anchorage", "UTC-9 Alaska"),
        ("America/Los_Angeles", "UTC-8 Pacific"),
        ("America/Denver", "UTC-7 Mountain"),
        ("America/Chicago", "UTC-6 Central"),
        ("America/New_York", "UTC-5 Eastern"),
        ("America/Sao_Paulo", "UTC-3 Brasilia"),
        ("Atlantic/Cape_Verde", "UTC-1 Cape Verde"),
        ("UTC", "UTC"),
        ("Europe/London", "UTC+0 London"),
        ("Europe/Paris", "UTC+1 Paris"),
        ("Europe/Helsinki", "UTC+2 Helsinki"),
        ("Europe/Moscow", "UTC+3 Moscow"),
        ("Asia/Dubai", "UTC+4 Dubai"),
        ("Asia/Kolkata", "UTC+5:30 India"),
        ("Asia/Bangkok", "UTC+7 Bangkok"),
        ("Asia/Shanghai", "UTC+8 Shanghai"),
        ("Asia/Tokyo", "UTC+9 Tokyo"),
        ("Australia/Sydney", "UTC+11 Sydney"),
        ("Pacific/Auckland", "UTC+12 Auckland"),
    ]
}

async fn show_slots(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    Query(query): Query<SlotsQuery>,
) -> impl IntoResponse {
    let et: Option<(String, String, String, Option<String>, i32, i32, i32, i32)> = sqlx::query_as(
        "SELECT id, slug, title, description, duration_min, buffer_before, buffer_after, min_notice_min
         FROM event_types WHERE slug = ? AND enabled = 1",
    )
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (et_id, et_slug, et_title, et_desc, duration, buf_before, buf_after, min_notice) =
        match et {
            Some(e) => e,
            None => return Html("Event type not found.".to_string()),
        };

    let host_name: String = sqlx::query_scalar(
        "SELECT a.name FROM accounts a JOIN event_types et ON et.account_id = a.id WHERE et.id = ?",
    )
    .bind(&et_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .unwrap_or_else(|| "Host".to_string());

    let guest_tz = parse_guest_tz(query.tz.as_deref());
    let host_tz = get_host_tz(&state.pool, &et_id).await;
    let guest_tz_name = guest_tz.name().to_string();

    let week = query.week.unwrap_or(0).max(0);
    let days_per_page = 7;
    let start_offset = week * days_per_page;
    let slot_days = compute_slots(
        &state.pool, &et_id, duration, buf_before, buf_after, min_notice, start_offset, days_per_page, host_tz, guest_tz,
    )
    .await;
    let prev_week = if week > 0 { Some(week - 1) } else { None };
    let next_week = week + 1;

    let days_ctx: Vec<minijinja::Value> = slot_days
        .iter()
        .map(|d| {
            let slots: Vec<minijinja::Value> = d
                .slots
                .iter()
                .map(|s| context! { start => s.start, end => s.end, host_date => s.host_date, host_time => s.host_time })
                .collect();
            context! { date => d.date, label => d.label, slots => slots }
        })
        .collect();

    let now_guest = Utc::now().with_timezone(&guest_tz).naive_local();
    let range_start = now_guest.date() + Duration::days(start_offset as i64);
    let range_end = now_guest.date() + Duration::days((start_offset + days_per_page - 1) as i64);
    let range_label = format!(
        "{} – {}",
        range_start.format("%b %-d"),
        range_end.format("%b %-d, %Y")
    );

    let tz_options: Vec<minijinja::Value> = common_timezones()
        .iter()
        .map(|(iana, label)| context! { value => iana, label => label, selected => (*iana == guest_tz_name) })
        .collect();

    let tmpl = state.templates.get_template("slots.html").unwrap();
    let rendered = tmpl
        .render(context! {
            event_type => context! {
                slug => et_slug,
                title => et_title,
                description => et_desc,
                duration_min => duration,
            },
            host_name => host_name,
            days => days_ctx,
            prev_week => prev_week,
            next_week => next_week,
            range_label => range_label,
            guest_tz => guest_tz_name,
            tz_options => tz_options,
        })
        .unwrap_or_else(|e| format!("Template error: {}", e));

    Html(rendered)
}

#[derive(Deserialize)]
struct BookQuery {
    date: String,
    time: String,
    #[serde(default)]
    tz: Option<String>,
}

async fn show_book_form(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    Query(query): Query<BookQuery>,
) -> impl IntoResponse {
    let et: Option<(String, String, String, Option<String>, i32)> = sqlx::query_as(
        "SELECT id, slug, title, description, duration_min
         FROM event_types WHERE slug = ? AND enabled = 1",
    )
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (et_id, et_slug, et_title, et_desc, duration) = match et {
        Some(e) => e,
        None => return Html("Event type not found.".to_string()),
    };

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

    let date = NaiveDate::parse_from_str(&query.date, "%Y-%m-%d").unwrap();
    let time = NaiveTime::parse_from_str(&query.time, "%H:%M").unwrap();
    let end_time = (date.and_time(time) + Duration::minutes(duration as i64))
        .time()
        .format("%H:%M")
        .to_string();
    let date_label = date.format("%A, %B %-d, %Y").to_string();

    let tmpl = state.templates.get_template("book.html").unwrap();
    let rendered = tmpl
        .render(context! {
            event_type => context! {
                slug => et_slug,
                title => et_title,
                description => et_desc,
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
        })
        .unwrap_or_else(|e| format!("Template error: {}", e));

    Html(rendered)
}

#[derive(Deserialize)]
struct BookForm {
    date: String,
    time: String,
    name: String,
    email: String,
    notes: Option<String>,
    #[serde(default)]
    tz: Option<String>,
}

async fn handle_booking(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
    Form(form): Form<BookForm>,
) -> impl IntoResponse {
    let et: Option<(String, String, String, i32, i32, i32, i32, i32)> = sqlx::query_as(
        "SELECT id, slug, title, duration_min, buffer_before, buffer_after, min_notice_min, requires_confirmation
         FROM event_types WHERE slug = ? AND enabled = 1",
    )
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (et_id, _et_slug, et_title, duration, buffer_before, buffer_after, min_notice, requires_confirmation) = match et {
        Some(e) => e,
        None => return Html("Event type not found.".to_string()).into_response(),
    };
    let needs_approval = requires_confirmation != 0;

    let date = match NaiveDate::parse_from_str(&form.date, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return Html("Invalid date.".to_string()).into_response(),
    };
    let start_time = match NaiveTime::parse_from_str(&form.time, "%H:%M") {
        Ok(t) => t,
        Err(_) => return Html("Invalid time.".to_string()).into_response(),
    };

    let slot_start = date.and_time(start_time);
    let slot_end = slot_start + Duration::minutes(duration as i64);

    // Validate minimum notice
    let now = Local::now().naive_local();
    if slot_start < now + Duration::minutes(min_notice as i64) {
        return Html("This slot is no longer available (too soon).".to_string()).into_response();
    }

    // Validate conflicts
    let buf_start = slot_start - Duration::minutes(buffer_before as i64);
    let buf_end = slot_end + Duration::minutes(buffer_after as i64);

    // Non-recurring events + bookings
    let mut busy: Vec<(String, String)> = sqlx::query_as(
        "SELECT start_at, end_at FROM events
         WHERE (rrule IS NULL OR rrule = '')
         UNION ALL
         SELECT start_at, end_at FROM bookings WHERE status = 'confirmed'",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    // Recurring events — expand and check
    let recurring: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT start_at, end_at, rrule, raw_ical FROM events
         WHERE rrule IS NOT NULL AND rrule != ''",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    for (s, e, rrule_str, raw_ical) in &recurring {
        if let (Some(ev_start), Some(ev_end)) = (parse_datetime(s), parse_datetime(e)) {
            let exdates = raw_ical.as_deref().map(crate::rrule::extract_exdates).unwrap_or_default();
            let occurrences = crate::rrule::expand_rrule(ev_start, ev_end, &rrule_str, &exdates, buf_start, buf_end);
            for (os, oe) in occurrences {
                busy.push((os.format("%Y-%m-%dT%H:%M:%S").to_string(), oe.format("%Y-%m-%dT%H:%M:%S").to_string()));
            }
        }
    }

    for (bs, be) in &busy {
        if let (Some(s), Some(e)) = (parse_datetime(bs), parse_datetime(be)) {
            if s < buf_end && e > buf_start {
                return Html("This slot is no longer available.".to_string()).into_response();
            }
        }
    }

    // Create booking
    let id = uuid::Uuid::new_v4().to_string();
    let uid = format!("{}@calrs", uuid::Uuid::new_v4());
    let cancel_token = uuid::Uuid::new_v4().to_string();
    let reschedule_token = uuid::Uuid::new_v4().to_string();
    let start_at = slot_start.format("%Y-%m-%dT%H:%M:%S").to_string();
    let end_at = slot_end.format("%Y-%m-%dT%H:%M:%S").to_string();
    let guest_tz = parse_guest_tz(form.tz.as_deref());
    let guest_timezone = guest_tz.name().to_string();

    let initial_status = if needs_approval { "pending" } else { "confirmed" };

    sqlx::query(
        "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, notes, start_at, end_at, status, cancel_token, reschedule_token)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
    .execute(&state.pool)
    .await
    .unwrap();

    // Send emails if SMTP is configured
    if let Ok(Some(smtp_config)) = crate::email::load_smtp_config(&state.pool).await {
        let host: Option<(String, String)> = sqlx::query_as(
            "SELECT name, email FROM accounts WHERE id = (SELECT account_id FROM event_types WHERE id = ?)",
        )
        .bind(&et_id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

        if let Some((host_name, host_email)) = host {
            let details = crate::email::BookingDetails {
                event_title: et_title.clone(),
                date: form.date.clone(),
                start_time: form.time.clone(),
                end_time: slot_end.time().format("%H:%M").to_string(),
                guest_name: form.name.clone(),
                guest_email: form.email.clone(),
                guest_timezone: guest_timezone.clone(),
                host_name,
                host_email,
                uid: uid.clone(),
                notes: form.notes.clone(),
                location: None, // Legacy route doesn't have location
            };

            if needs_approval {
                let _ = crate::email::send_host_approval_request(&smtp_config, &details, &id).await;
                let _ = crate::email::send_guest_pending_notice(&smtp_config, &details).await;
            } else {
                let _ = crate::email::send_guest_confirmation(&smtp_config, &details).await;
                let _ = crate::email::send_host_notification(&smtp_config, &details).await;
                // Push confirmed booking to CalDAV
                let host_user_id: Option<String> = sqlx::query_scalar(
                    "SELECT u.id FROM users u JOIN accounts a ON a.user_id = u.id JOIN event_types et ON et.account_id = a.id WHERE et.id = ?",
                )
                .bind(&et_id)
                .fetch_optional(&state.pool)
                .await
                .unwrap_or(None);
                if let Some(uid_user) = host_user_id {
                    caldav_push_booking(&state.pool, &uid_user, &uid, &details).await;
                }
            }
        }
    }

    // Render confirmation
    let host_name: String = sqlx::query_scalar(
        "SELECT a.name FROM accounts a JOIN event_types et ON et.account_id = a.id WHERE et.id = ?",
    )
    .bind(&et_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .unwrap_or_else(|| "Host".to_string());

    let date_label = date.format("%A, %B %-d, %Y").to_string();
    let end_time_str = slot_end.time().format("%H:%M").to_string();

    let tmpl = state.templates.get_template("confirmed.html").unwrap();
    let rendered = tmpl
        .render(context! {
            event_title => et_title,
            date_label => date_label,
            time_start => form.time,
            time_end => end_time_str,
            host_name => host_name,
            guest_email => form.email,
            notes => form.notes,
            pending => needs_approval,
        })
        .unwrap_or_else(|e| format!("Template error: {}", e));

    Html(rendered).into_response()
}

// --- Troubleshoot ---

#[derive(Deserialize)]
struct TroubleshootQuery {
    date: Option<String>,
    event_type: Option<String>,
}

async fn troubleshoot(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Query(params): Query<TroubleshootQuery>,
) -> impl IntoResponse {
    let user = &auth_user.0;
    let host_tz = get_host_tz(&state.pool, "").await;
    let now_host = Utc::now().with_timezone(&host_tz).naive_local();

    let target_date = params
        .date
        .as_deref()
        .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        .unwrap_or(now_host.date());

    // Fetch user's event types for the selector
    let event_types: Vec<(String, String, i32, i32, i32, i32)> = sqlx::query_as(
        "SELECT et.slug, et.title, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         WHERE a.user_id = ? AND et.group_id IS NULL AND et.enabled = 1
         ORDER BY et.created_at",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    if event_types.is_empty() {
        let tmpl = match state.templates.get_template("troubleshoot.html") {
            Ok(t) => t,
            Err(e) => return Html(format!("Template error: {}", e)),
        };
        return Html(tmpl.render(context! {
            user_name => &user.name,
            no_event_types => true,
        }).unwrap_or_default());
    }

    let selected_slug = params.event_type.as_deref().unwrap_or(&event_types[0].0);
    let selected_et = event_types.iter().find(|et| et.0 == selected_slug).unwrap_or(&event_types[0]);
    let (ref et_slug, ref et_title, duration, buf_before, buf_after, min_notice) = *selected_et;

    // Get event type ID
    let et_id: Option<(String,)> = sqlx::query_as(
        "SELECT et.id FROM event_types et JOIN accounts a ON a.id = et.account_id WHERE a.user_id = ? AND et.slug = ?",
    )
    .bind(&user.id)
    .bind(et_slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let et_id = match et_id {
        Some((id,)) => id,
        None => return Html("Event type not found".to_string()),
    };

    // Availability rules for this day of week
    let weekday = target_date.weekday().num_days_from_sunday() as i32;
    let rules: Vec<(String, String)> = sqlx::query_as(
        "SELECT start_time, end_time FROM availability_rules WHERE event_type_id = ? AND day_of_week = ? ORDER BY start_time",
    )
    .bind(&et_id)
    .bind(weekday)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    // Busy events for this date — enriched with title + calendar name
    let day_start_compact = target_date.format("%Y%m%d").to_string();
    let day_end_compact = (target_date + Duration::days(1)).format("%Y%m%d").to_string();
    let day_start_iso = target_date.format("%Y-%m-%dT00:00:00").to_string();
    let day_end_iso = target_date.format("%Y-%m-%dT23:59:59").to_string();

    // Non-recurring busy events for this date
    let mut busy_events: Vec<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT e.start_at, e.end_at, e.summary, c.display_name
         FROM events e
         JOIN calendars c ON c.id = e.calendar_id
         JOIN caldav_sources cs ON cs.id = c.source_id
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND c.is_busy = 1
           AND (e.rrule IS NULL OR e.rrule = '')
           AND ((e.start_at < ? AND e.end_at > ?) OR (e.start_at < ? AND e.end_at > ?))
         ORDER BY e.start_at",
    )
    .bind(&user.id)
    .bind(&day_end_compact).bind(&day_start_compact)
    .bind(&day_end_iso).bind(&day_start_iso)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    // Recurring busy events — expand into this date
    let recurring_events: Vec<(String, String, String, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT e.start_at, e.end_at, e.rrule, e.raw_ical, e.summary, c.display_name
         FROM events e
         JOIN calendars c ON c.id = e.calendar_id
         JOIN caldav_sources cs ON cs.id = c.source_id
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND c.is_busy = 1
           AND e.rrule IS NOT NULL AND e.rrule != ''
           AND (e.start_at <= ? OR e.start_at <= ?)",
    )
    .bind(&user.id)
    .bind(&day_end_iso)
    .bind(&day_end_compact)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let ts_window_start = target_date.and_hms_opt(0, 0, 0).unwrap();
    let ts_window_end = target_date.and_hms_opt(23, 59, 59).unwrap();
    for (s, e, rrule_str, raw_ical, summary, cal_name) in &recurring_events {
        if let (Some(ev_start), Some(ev_end)) = (parse_datetime(s), parse_datetime(e)) {
            let exdates = raw_ical.as_deref().map(crate::rrule::extract_exdates).unwrap_or_default();
            let occurrences = crate::rrule::expand_rrule(ev_start, ev_end, rrule_str, &exdates, ts_window_start, ts_window_end);
            for (os, oe) in occurrences {
                busy_events.push((
                    os.format("%Y-%m-%dT%H:%M:%S").to_string(),
                    oe.format("%Y-%m-%dT%H:%M:%S").to_string(),
                    summary.clone(),
                    cal_name.clone(),
                ));
            }
        }
    }

    // Bookings for this date
    let bookings: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT b.start_at, b.end_at, b.guest_name, et2.title
         FROM bookings b
         JOIN event_types et2 ON et2.id = b.event_type_id
         JOIN accounts a ON a.id = et2.account_id
         WHERE a.user_id = ? AND b.status IN ('confirmed', 'pending')
           AND b.start_at < ? AND b.end_at > ?
         ORDER BY b.start_at",
    )
    .bind(&user.id)
    .bind(&day_end_iso).bind(&day_start_iso)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    // Build timeline: scan 15-min ticks from display_start to display_end
    let display_start_hour: u32 = rules.iter()
        .filter_map(|(s, _)| NaiveTime::parse_from_str(s, "%H:%M").ok())
        .map(|t| t.hour())
        .min()
        .unwrap_or(8)
        .saturating_sub(1);
    let display_end_hour: u32 = rules.iter()
        .filter_map(|(_, e)| NaiveTime::parse_from_str(e, "%H:%M").ok())
        .map(|t| if t.minute() > 0 { t.hour() + 1 } else { t.hour() })
        .max()
        .unwrap_or(18)
        .min(23) + 1;

    let display_start = NaiveTime::from_hms_opt(display_start_hour, 0, 0).unwrap_or(NaiveTime::from_hms_opt(7, 0, 0).unwrap());
    let display_end = NaiveTime::from_hms_opt(display_end_hour, 0, 0).unwrap_or(NaiveTime::from_hms_opt(19, 0, 0).unwrap());
    let total_minutes = (display_end - display_start).num_minutes() as f64;

    let min_start = now_host + Duration::minutes(min_notice as i64);

    // Parse availability windows
    let avail_windows: Vec<(NaiveTime, NaiveTime)> = rules.iter()
        .filter_map(|(s, e)| {
            let st = NaiveTime::parse_from_str(s, "%H:%M").ok()?;
            let en = NaiveTime::parse_from_str(e, "%H:%M").ok()?;
            Some((st, en))
        })
        .collect();

    // Parse busy events into (start_dt, end_dt, label, detail)
    let busy_parsed: Vec<(NaiveDateTime, NaiveDateTime, String, String)> = busy_events.iter()
        .filter_map(|(s, e, summary, cal)| {
            let start = parse_datetime(s)?;
            let end = parse_datetime(e)?;
            let label = summary.clone().unwrap_or_else(|| "Busy".to_string());
            let detail = cal.clone().unwrap_or_default();
            Some((start, end, label, detail))
        })
        .collect();

    // Parse bookings into (start_dt, end_dt, label, detail)
    let bookings_parsed: Vec<(NaiveDateTime, NaiveDateTime, String, String)> = bookings.iter()
        .filter_map(|(s, e, guest, et_title)| {
            let start = parse_datetime(s)?;
            let end = parse_datetime(e)?;
            Some((start, end, guest.clone(), et_title.clone()))
        })
        .collect();

    // Scan in 15-min increments and classify each tick
    struct Tick {
        time: NaiveTime,
        status: String,      // "available", "outside", "busy_event", "booking", "buffer", "min_notice"
        label: String,
        detail: String,
    }

    let mut ticks: Vec<Tick> = Vec::new();
    let mut cursor = display_start;
    let tick_size = Duration::minutes(15);

    while cursor < display_end {
        let tick_dt = target_date.and_time(cursor);
        let tick_end = tick_dt + tick_size;

        // 1. Check if within availability window
        let in_avail = avail_windows.iter().any(|(ws, we)| cursor >= *ws && cursor < *we);

        if !in_avail {
            ticks.push(Tick {
                time: cursor,
                status: "outside".to_string(),
                label: "Outside availability".to_string(),
                detail: String::new(),
            });
            cursor = (tick_dt + tick_size).time();
            continue;
        }

        // 2. Check minimum notice
        if tick_dt < min_start {
            ticks.push(Tick {
                time: cursor,
                status: "min_notice".to_string(),
                label: format!("Min. notice ({}min)", min_notice),
                detail: String::new(),
            });
            cursor = (tick_dt + tick_size).time();
            continue;
        }

        // 3. Check calendar events (with buffers)
        let event_conflict = busy_parsed.iter().find(|(s, e, _, _)| {
            let buf_s = *s - Duration::minutes(buf_before as i64);
            let buf_e = *e + Duration::minutes(buf_after as i64);
            tick_dt < buf_e && tick_end > buf_s
        });

        if let Some((ev_s, ev_e, ev_label, ev_detail)) = event_conflict {
            // Is it the event itself or just the buffer zone?
            let in_event = tick_dt < *ev_e && tick_end > *ev_s;
            if in_event {
                ticks.push(Tick {
                    time: cursor,
                    status: "busy_event".to_string(),
                    label: ev_label.clone(),
                    detail: ev_detail.clone(),
                });
            } else {
                ticks.push(Tick {
                    time: cursor,
                    status: "buffer".to_string(),
                    label: format!("Buffer ({}min)", if tick_dt < *ev_s { buf_before } else { buf_after }),
                    detail: format!("Around: {}", ev_label),
                });
            }
            cursor = (tick_dt + tick_size).time();
            continue;
        }

        // 4. Check bookings (with buffers)
        let booking_conflict = bookings_parsed.iter().find(|(s, e, _, _)| {
            let buf_s = *s - Duration::minutes(buf_before as i64);
            let buf_e = *e + Duration::minutes(buf_after as i64);
            tick_dt < buf_e && tick_end > buf_s
        });

        if let Some((bk_s, bk_e, bk_guest, bk_et)) = booking_conflict {
            let in_booking = tick_dt < *bk_e && tick_end > *bk_s;
            if in_booking {
                ticks.push(Tick {
                    time: cursor,
                    status: "booking".to_string(),
                    label: bk_guest.clone(),
                    detail: bk_et.clone(),
                });
            } else {
                ticks.push(Tick {
                    time: cursor,
                    status: "buffer".to_string(),
                    label: format!("Buffer ({}min)", if tick_dt < *bk_s { buf_before } else { buf_after }),
                    detail: format!("Around: {} booking", bk_guest),
                });
            }
            cursor = (tick_dt + tick_size).time();
            continue;
        }

        // 5. Available!
        ticks.push(Tick {
            time: cursor,
            status: "available".to_string(),
            label: "Available".to_string(),
            detail: String::new(),
        });
        cursor = (tick_dt + tick_size).time();
    }

    // Merge consecutive ticks with same status+label into blocks
    struct Block {
        start: NaiveTime,
        end: NaiveTime,
        status: String,
        label: String,
        detail: String,
        left_pct: f64,
        width_pct: f64,
    }

    let mut blocks: Vec<Block> = Vec::new();
    for tick in &ticks {
        let tick_end_time = (target_date.and_time(tick.time) + tick_size).time();
        if let Some(last) = blocks.last_mut() {
            if last.status == tick.status && last.label == tick.label {
                last.end = tick_end_time;
                let start_min = (last.start - display_start).num_minutes() as f64;
                let dur_min = (last.end - last.start).num_minutes() as f64;
                last.left_pct = start_min / total_minutes * 100.0;
                last.width_pct = dur_min / total_minutes * 100.0;
                continue;
            }
        }
        let start_min = (tick.time - display_start).num_minutes() as f64;
        let dur_min = tick_size.num_minutes() as f64;
        blocks.push(Block {
            start: tick.time,
            end: tick_end_time,
            status: tick.status.clone(),
            label: tick.label.clone(),
            detail: tick.detail.clone(),
            left_pct: start_min / total_minutes * 100.0,
            width_pct: dur_min / total_minutes * 100.0,
        });
    }

    // Build template data
    let blocks_ctx: Vec<minijinja::Value> = blocks.iter().map(|b| {
        context! {
            start => b.start.format("%H:%M").to_string(),
            end => b.end.format("%H:%M").to_string(),
            status => &b.status,
            label => &b.label,
            detail => &b.detail,
            left_pct => format!("{:.2}", b.left_pct),
            width_pct => format!("{:.2}", b.width_pct),
        }
    }).collect();

    // Hour markers for the timeline
    let mut hour_markers: Vec<minijinja::Value> = Vec::new();
    let mut h = display_start_hour;
    while h <= display_end_hour {
        let min_offset = (h - display_start_hour) as f64 * 60.0;
        let left_pct = min_offset / total_minutes * 100.0;
        hour_markers.push(context! {
            label => format!("{:02}:00", h),
            left_pct => format!("{:.2}", left_pct),
        });
        h += 1;
    }

    // Breakdown: only non-available blocks
    let breakdown_ctx: Vec<minijinja::Value> = blocks.iter()
        .filter(|b| b.status != "available" && b.status != "outside")
        .map(|b| {
            let reason = match b.status.as_str() {
                "busy_event" => format!("Calendar event: {}", b.label),
                "booking" => format!("Booking: {}", b.label),
                "buffer" => b.label.clone(),
                "min_notice" => b.label.clone(),
                _ => b.status.clone(),
            };
            context! {
                start => b.start.format("%H:%M").to_string(),
                end => b.end.format("%H:%M").to_string(),
                status => &b.status,
                reason => reason,
                detail => &b.detail,
            }
        })
        .collect();

    let et_options: Vec<minijinja::Value> = event_types.iter().map(|et| {
        context! {
            slug => &et.0,
            title => &et.1,
            selected => et.0 == *et_slug,
        }
    }).collect();

    let prev_date = (target_date - Duration::days(1)).format("%Y-%m-%d").to_string();
    let next_date = (target_date + Duration::days(1)).format("%Y-%m-%d").to_string();
    let date_label = target_date.format("%A, %B %-d, %Y").to_string();

    let tmpl = match state.templates.get_template("troubleshoot.html") {
        Ok(t) => t,
        Err(e) => return Html(format!("Template error: {}", e)),
    };

    Html(tmpl.render(context! {
        user_name => &user.name,
        no_event_types => false,
        event_types => et_options,
        selected_slug => et_slug,
        selected_date => target_date.format("%Y-%m-%d").to_string(),
        date_label => date_label,
        prev_date => prev_date,
        next_date => next_date,
        has_rules => !rules.is_empty(),
        blocks => blocks_ctx,
        hour_markers => hour_markers,
        breakdown => breakdown_ctx,
        et_title => et_title,
        duration => duration,
        buf_before => buf_before,
        buf_after => buf_after,
        min_notice => min_notice,
    }).unwrap_or_default())
}

// --- Admin dashboard ---

async fn admin_dashboard(
    State(state): State<Arc<AppState>>,
    admin: crate::auth::AdminUser,
) -> impl IntoResponse {
    let current_user = &admin.0;

    // Fetch all users
    let users: Vec<(String, String, String, String, String, bool)> = sqlx::query_as(
        "SELECT id, name, email, role, auth_provider, enabled FROM users ORDER BY created_at",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let user_count = users.len();

    // Fetch groups per user
    let user_groups_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT ug.user_id, g.name FROM user_groups ug JOIN groups g ON g.id = ug.group_id ORDER BY g.name",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    // Build a map of user_id -> comma-separated group names
    let mut user_groups_map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for (uid, gname) in &user_groups_rows {
        user_groups_map.entry(uid.clone()).or_default().push(gname.clone());
    }

    let users_ctx: Vec<minijinja::Value> = users
        .iter()
        .map(|(id, name, email, role, auth_provider, enabled)| {
            let groups = user_groups_map.get(id).cloned().unwrap_or_default();
            context! {
                id => id,
                name => name,
                email => email,
                role => role,
                auth_provider => auth_provider,
                enabled => enabled,
                groups => groups,
            }
        })
        .collect();

    // Fetch groups with member count
    let groups_rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT g.id, g.name, COUNT(ug.user_id) as member_count \
         FROM groups g LEFT JOIN user_groups ug ON ug.group_id = g.id \
         GROUP BY g.id ORDER BY g.name",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let group_count = groups_rows.len();

    let groups_ctx: Vec<minijinja::Value> = groups_rows
        .iter()
        .map(|(id, name, member_count)| {
            // Fetch members for this group
            context! {
                id => id,
                name => name,
                member_count => member_count,
            }
        })
        .collect();

    // Fetch auth config
    let auth_config = crate::auth::get_auth_config(&state.pool).await.ok();
    let registration_enabled = auth_config.as_ref().map(|c| c.registration_enabled).unwrap_or(false);
    let allowed_email_domains = auth_config.as_ref().and_then(|c| c.allowed_email_domains.clone()).unwrap_or_default();
    let oidc_enabled = auth_config.as_ref().map(|c| c.oidc_enabled).unwrap_or(false);
    let oidc_issuer_url = auth_config.as_ref().and_then(|c| c.oidc_issuer_url.clone()).unwrap_or_default();
    let oidc_client_id = auth_config.as_ref().and_then(|c| c.oidc_client_id.clone()).unwrap_or_default();
    let oidc_auto_register = auth_config.as_ref().map(|c| c.oidc_auto_register).unwrap_or(true);

    // Fetch SMTP config (first one found)
    let smtp: Option<(String, i32, String, bool)> = sqlx::query_as(
        "SELECT host, port, from_email, enabled FROM smtp_config LIMIT 1",
    )
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let smtp_configured = smtp.is_some();
    let (smtp_host, smtp_port, smtp_from_email, smtp_enabled) = smtp.unwrap_or_default();

    let tmpl = match state.templates.get_template("admin.html") {
        Ok(t) => t,
        Err(e) => return Html(format!("Template error: {}", e)),
    };

    Html(
        tmpl.render(context! {
            current_user_id => current_user.id,
            users => users_ctx,
            user_count => user_count,
            groups => groups_ctx,
            group_count => group_count,
            registration_enabled => registration_enabled,
            allowed_email_domains => allowed_email_domains,
            oidc_enabled => oidc_enabled,
            oidc_issuer_url => oidc_issuer_url,
            oidc_client_id => oidc_client_id,
            oidc_auto_register => oidc_auto_register,
            smtp_configured => smtp_configured,
            smtp_host => smtp_host,
            smtp_port => smtp_port,
            smtp_from_email => smtp_from_email,
            smtp_enabled => smtp_enabled,
        })
        .unwrap_or_else(|e| format!("Template error: {}", e)),
    )
}

async fn admin_toggle_role(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    // Get current role
    let current_role: Option<(String,)> = sqlx::query_as("SELECT role FROM users WHERE id = ?")
        .bind(&user_id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

    if let Some((role,)) = current_role {
        let new_role = if role == "admin" { "user" } else { "admin" };
        let _ = sqlx::query("UPDATE users SET role = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(new_role)
            .bind(&user_id)
            .execute(&state.pool)
            .await;
    }

    Redirect::to("/dashboard/admin")
}

async fn admin_toggle_enabled(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    // Get current enabled status
    let current: Option<(bool,)> = sqlx::query_as("SELECT enabled FROM users WHERE id = ?")
        .bind(&user_id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

    if let Some((enabled,)) = current {
        let new_enabled = !enabled;
        let _ = sqlx::query("UPDATE users SET enabled = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(new_enabled)
            .bind(&user_id)
            .execute(&state.pool)
            .await;
    }

    Redirect::to("/dashboard/admin")
}

#[derive(Deserialize)]
struct AdminAuthForm {
    registration_enabled: Option<String>,
    allowed_email_domains: Option<String>,
}

async fn admin_update_auth(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    Form(form): Form<AdminAuthForm>,
) -> impl IntoResponse {
    let registration_enabled = form.registration_enabled.is_some();
    let allowed_domains = form.allowed_email_domains.filter(|d| !d.trim().is_empty());

    let _ = sqlx::query(
        "UPDATE auth_config SET registration_enabled = ?, allowed_email_domains = ?, updated_at = datetime('now') WHERE id = 'singleton'",
    )
    .bind(registration_enabled)
    .bind(&allowed_domains)
    .execute(&state.pool)
    .await;

    Redirect::to("/dashboard/admin")
}

#[derive(Deserialize)]
struct AdminOidcForm {
    oidc_enabled: Option<String>,
    oidc_issuer_url: Option<String>,
    oidc_client_id: Option<String>,
    oidc_client_secret: Option<String>,
    oidc_auto_register: Option<String>,
}

async fn admin_update_oidc(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    Form(form): Form<AdminOidcForm>,
) -> impl IntoResponse {
    let oidc_enabled = form.oidc_enabled.is_some();
    let issuer_url = form.oidc_issuer_url.filter(|s| !s.trim().is_empty());
    let client_id = form.oidc_client_id.filter(|s| !s.trim().is_empty());
    let auto_register = form.oidc_auto_register.is_some();

    // If client_secret is provided (non-empty), update it; otherwise keep current value
    let secret_provided = form.oidc_client_secret.as_ref().map(|s| !s.trim().is_empty()).unwrap_or(false);

    if secret_provided {
        let client_secret = form.oidc_client_secret.unwrap();
        let _ = sqlx::query(
            "UPDATE auth_config SET oidc_enabled = ?, oidc_issuer_url = ?, oidc_client_id = ?, oidc_client_secret = ?, oidc_auto_register = ?, updated_at = datetime('now') WHERE id = 'singleton'",
        )
        .bind(oidc_enabled)
        .bind(&issuer_url)
        .bind(&client_id)
        .bind(&client_secret)
        .bind(auto_register)
        .execute(&state.pool)
        .await;
    } else {
        let _ = sqlx::query(
            "UPDATE auth_config SET oidc_enabled = ?, oidc_issuer_url = ?, oidc_client_id = ?, oidc_auto_register = ?, updated_at = datetime('now') WHERE id = 'singleton'",
        )
        .bind(oidc_enabled)
        .bind(&issuer_url)
        .bind(&client_id)
        .bind(auto_register)
        .execute(&state.pool)
        .await;
    }

    Redirect::to("/dashboard/admin")
}

// --- CalDAV write-back ---

/// Push a confirmed booking to the host's CalDAV calendar.
/// Finds the first CalDAV source with a write_calendar_href set for this user,
/// generates the ICS, and PUTs it to the CalDAV server.
async fn caldav_push_booking(
    pool: &SqlitePool,
    user_id: &str,
    booking_uid: &str,
    details: &crate::email::BookingDetails,
) {
    // Find a CalDAV source with write_calendar_href configured for this user
    let source: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT cs.url, cs.username, cs.password_enc, cs.write_calendar_href
         FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND cs.enabled = 1 AND cs.write_calendar_href IS NOT NULL
         LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    let (url, username, password_hex, calendar_href) = match source {
        Some(s) => s,
        None => return, // No CalDAV write configured — silently skip
    };

    let password = match hex::decode(&password_hex) {
        Ok(bytes) => String::from_utf8(bytes).unwrap_or_default(),
        Err(_) => return,
    };

    let ics = crate::email::generate_ics(details, "REQUEST");
    let client = crate::caldav::CaldavClient::new(&url, &username, &password);

    if let Err(e) = client.put_event(&calendar_href, booking_uid, &ics).await {
        eprintln!("CalDAV write-back failed: {}", e);
        return;
    }

    // Record which calendar href the booking was pushed to
    let _ = sqlx::query("UPDATE bookings SET caldav_calendar_href = ? WHERE uid = ?")
        .bind(&calendar_href)
        .bind(booking_uid)
        .execute(pool)
        .await;
}

/// Delete a booking from the host's CalDAV calendar.
async fn caldav_delete_booking(pool: &SqlitePool, user_id: &str, booking_uid: &str) {
    // Check if this booking was pushed to CalDAV
    let info: Option<(String,)> = sqlx::query_as(
        "SELECT caldav_calendar_href FROM bookings WHERE uid = ? AND caldav_calendar_href IS NOT NULL",
    )
    .bind(booking_uid)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    let calendar_href = match info {
        Some((href,)) => href,
        None => return, // Was never pushed to CalDAV
    };

    // Get the CalDAV source credentials
    let source: Option<(String, String, String)> = sqlx::query_as(
        "SELECT cs.url, cs.username, cs.password_enc
         FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND cs.enabled = 1 AND cs.write_calendar_href = ?
         LIMIT 1",
    )
    .bind(user_id)
    .bind(&calendar_href)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    let (url, username, password_hex) = match source {
        Some(s) => s,
        None => return,
    };

    let password = match hex::decode(&password_hex) {
        Ok(bytes) => String::from_utf8(bytes).unwrap_or_default(),
        Err(_) => return,
    };

    let client = crate::caldav::CaldavClient::new(&url, &username, &password);
    if let Err(e) = client.delete_event(&calendar_href, booking_uid).await {
        eprintln!("CalDAV delete failed: {}", e);
    }
}
