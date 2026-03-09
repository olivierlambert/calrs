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

    let event_types: Vec<(String, String, i32, bool)> = sqlx::query_as(
        "SELECT et.slug, et.title, et.duration_min, et.enabled
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         WHERE a.user_id = ?
         ORDER BY et.created_at",
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
        .map(|(slug, title, duration, enabled)| {
            context! { slug => slug, title => title, duration_min => duration, enabled => enabled }
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
    let et: Option<(String, String, String, Option<String>, i32, i32, i32, i32)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.description, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min
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

    let (et_id, et_slug, et_title, et_desc, duration, buf_before, buf_after, min_notice) =
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
    let et: Option<(String, String, String, Option<String>, i32)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.description, et.duration_min
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

    let (et_id, et_slug, et_title, et_desc, duration) = match et {
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
    let et: Option<(String, String, String, i32, i32, i32, i32)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min
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

    let (et_id, _et_slug, et_title, duration, buffer_before, buffer_after, min_notice) = match et {
        Some(e) => e,
        None => return Html("Event type not found.".to_string()).into_response(),
    };

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

    sqlx::query(
        "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, notes, start_at, end_at, cancel_token, reschedule_token)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
            };

            let _ = crate::email::send_guest_confirmation(&smtp_config, &details).await;
            let _ = crate::email::send_host_notification(&smtp_config, &details).await;
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
    let et: Option<(String, String, String, i32, i32, i32, i32)> = sqlx::query_as(
        "SELECT id, slug, title, duration_min, buffer_before, buffer_after, min_notice_min
         FROM event_types WHERE slug = ? AND enabled = 1",
    )
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (et_id, _et_slug, et_title, duration, buffer_before, buffer_after, min_notice) = match et {
        Some(e) => e,
        None => return Html("Event type not found.".to_string()).into_response(),
    };

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

    sqlx::query(
        "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, notes, start_at, end_at, cancel_token, reschedule_token)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
            };

            let _ = crate::email::send_guest_confirmation(&smtp_config, &details).await;
            let _ = crate::email::send_host_notification(&smtp_config, &details).await;
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
        })
        .unwrap_or_else(|e| format!("Template error: {}", e));

    Html(rendered).into_response()
}
