use axum::extract::{Form, Path, Query, State};
use axum::response::{Html, IntoResponse};
use axum::response::Redirect;
use axum::routing::{get, post};
use axum::Router;
use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime};
use minijinja::{context, Environment};
use serde::Deserialize;
use sqlx::SqlitePool;
use std::sync::Arc;

pub struct AppState {
    pub pool: SqlitePool,
    pub templates: Environment<'static>,
}

pub fn create_router(pool: SqlitePool) -> Router {
    let mut env = Environment::new();
    env.set_loader(minijinja::path_loader("templates"));

    let state = Arc::new(AppState {
        pool,
        templates: env,
    });

    Router::new()
        .merge(crate::auth::auth_router())
        .route("/dashboard", get(dashboard))
        .route("/dashboard/bookings/{id}/cancel", post(cancel_booking))
        .route("/dashboard/bookings/{id}/confirm", post(confirm_booking))
        .route("/dashboard/event-types/new", get(new_event_type_form).post(create_event_type))
        .route("/dashboard/event-types/{slug}/edit", get(edit_event_type_form).post(update_event_type))
        .route("/dashboard/event-types/{slug}/toggle", post(toggle_event_type))
        // Admin routes
        .route("/dashboard/admin", get(admin_dashboard))
        .route("/dashboard/admin/users/{id}/toggle-role", post(admin_toggle_role))
        .route("/dashboard/admin/users/{id}/toggle-enabled", post(admin_toggle_enabled))
        .route("/dashboard/admin/auth", post(admin_update_auth))
        .route("/dashboard/admin/oidc", post(admin_update_oidc))
        // User-scoped public booking routes
        .route("/u/{username}", get(user_profile))
        .route("/u/{username}/{slug}", get(show_slots_for_user))
        .route("/u/{username}/{slug}/book", get(show_book_form_for_user).post(handle_booking_for_user))
        // Legacy single-user routes (kept for backward compatibility)
        .route("/{slug}", get(show_slots))
        .route("/{slug}/book", get(show_book_form).post(handle_booking))
        .with_state(state)
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
         WHERE a.user_id = ?
         ORDER BY et.created_at",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

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

    Html(
        tmpl.render(context! {
            user_name => user.name,
            user_email => user.email,
            user_role => user.role,
            username => user.username,
            event_types => et_ctx,
            pending_bookings => pending_ctx,
            bookings => bookings_ctx,
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

    // Send confirmation emails
    if let Ok(Some(smtp_config)) = crate::email::load_smtp_config(&state.pool).await {
        let date = if start_at.len() >= 10 { &start_at[..10] } else { &start_at };
        let start_time = if start_at.len() >= 16 { &start_at[11..16] } else { "00:00" };
        let end_time = if end_at.len() >= 16 { &end_at[11..16] } else { "00:00" };

        let details = crate::email::BookingDetails {
            event_title,
            date: date.to_string(),
            start_time: start_time.to_string(),
            end_time: end_time.to_string(),
            guest_name,
            guest_email,
            guest_timezone: "UTC".to_string(),
            host_name: user.name.clone(),
            host_email: user.email.clone(),
            uid,
            notes: None,
            location: location_value,
        };

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
}

async fn new_event_type_form(
    State(state): State<Arc<AppState>>,
    _auth_user: crate::auth::AuthUser,
) -> impl IntoResponse {
    let tmpl = match state.templates.get_template("event_type_form.html") {
        Ok(t) => t,
        Err(e) => return Html(format!("Template error: {}", e)),
    };

    Html(
        tmpl.render(context! {
            editing => false,
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

    let _ = sqlx::query(
        "INSERT INTO event_types (id, account_id, slug, title, description, duration_min, buffer_before, buffer_after, min_notice_min, requires_confirmation, location_type, location_value)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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

    let week = query.week.unwrap_or(0).max(0);
    let days_per_page = 7;
    let start_offset = week * days_per_page;
    let slot_days = compute_slots(
        &state.pool, &et_id, duration, buf_before, buf_after, min_notice, start_offset, days_per_page,
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
                .map(|s| context! { start => s.start, end => s.end })
                .collect();
            context! { date => d.date, label => d.label, slots => slots }
        })
        .collect();

    let now = Local::now().naive_local();
    let range_start = now.date() + Duration::days(start_offset as i64);
    let range_end = now.date() + Duration::days((start_offset + days_per_page - 1) as i64);
    let range_label = format!(
        "{} – {}",
        range_start.format("%b %-d"),
        range_end.format("%b %-d, %Y")
    );

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

    let busy: Vec<(String, String)> = sqlx::query_as(
        "SELECT start_at, end_at FROM events
         UNION ALL
         SELECT start_at, end_at FROM bookings WHERE status = 'confirmed'",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

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
    let guest_timezone = "UTC".to_string();

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

struct SlotDay {
    date: String,
    label: String,
    slots: Vec<SlotTime>,
}

struct SlotTime {
    start: String,
    end: String,
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
) -> Vec<SlotDay> {
    let rules: Vec<(i32, String, String)> = sqlx::query_as(
        "SELECT day_of_week, start_time, end_time FROM availability_rules WHERE event_type_id = ?",
    )
    .bind(et_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let now = Local::now().naive_local();
    let min_start = now + Duration::minutes(min_notice as i64);
    let end_date = now.date() + Duration::days((start_offset + days_ahead) as i64);

    let end_compact = end_date.format("%Y%m%d").to_string();
    let now_compact = now.format("%Y%m%dT%H%M%S").to_string();
    let end_iso = end_date.format("%Y-%m-%dT23:59:59").to_string();
    let now_iso = now.format("%Y-%m-%dT%H:%M:%S").to_string();

    let busy: Vec<(String, String)> = sqlx::query_as(
        "SELECT start_at, end_at FROM events
         WHERE (start_at <= ? AND end_at >= ?)
            OR (start_at <= ? AND end_at >= ?)
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
                    day_slots.push(SlotTime {
                        start: cursor.format("%H:%M").to_string(),
                        end: (cursor + slot_duration).format("%H:%M").to_string(),
                    });
                }

                cursor = cursor + Duration::minutes(duration as i64);
            }
        }

        if !day_slots.is_empty() {
            let label = date.format("%A, %B %-d").to_string();
            let date_str = date.format("%Y-%m-%d").to_string();
            result.push(SlotDay {
                date: date_str,
                label,
                slots: day_slots,
            });
        }
    }

    result
}

// --- Handlers ---

#[derive(Deserialize)]
struct SlotsQuery {
    #[serde(default)]
    week: Option<i32>,
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

    let week = query.week.unwrap_or(0).max(0);
    let days_per_page = 7;
    let start_offset = week * days_per_page;
    let slot_days = compute_slots(
        &state.pool, &et_id, duration, buf_before, buf_after, min_notice, start_offset, days_per_page,
    )
    .await;
    let prev_week = if week > 0 { Some(week - 1) } else { None };
    let next_week = week + 1; // always allow forward navigation

    // Convert to template-friendly format
    let days_ctx: Vec<minijinja::Value> = slot_days
        .iter()
        .map(|d| {
            let slots: Vec<minijinja::Value> = d
                .slots
                .iter()
                .map(|s| context! { start => s.start, end => s.end })
                .collect();
            context! { date => d.date, label => d.label, slots => slots }
        })
        .collect();

    // Compute the date range label for this week view
    let now = Local::now().naive_local();
    let range_start = now.date() + Duration::days(start_offset as i64);
    let range_end = now.date() + Duration::days((start_offset + days_per_page - 1) as i64);
    let range_label = format!(
        "{} – {}",
        range_start.format("%b %-d"),
        range_end.format("%b %-d, %Y")
    );

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
        })
        .unwrap_or_else(|e| format!("Template error: {}", e));

    Html(rendered)
}

#[derive(Deserialize)]
struct BookQuery {
    date: String,
    time: String,
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

    let busy: Vec<(String, String)> = sqlx::query_as(
        "SELECT start_at, end_at FROM events
         UNION ALL
         SELECT start_at, end_at FROM bookings WHERE status = 'confirmed'",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

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
    let guest_timezone = "UTC".to_string(); // TODO: detect from browser

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
