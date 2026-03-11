use crate::utils::{convert_event_to_tz, extract_vevent_field, extract_vevent_tzid, split_vevents};
use axum::extract::{Form, Multipart, Path, Query, State};
use axum::response::Redirect;
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::Router;
use chrono::{
    Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Timelike, Utc,
};
use chrono_tz::Tz;
use minijinja::{context, Environment};
use serde::Deserialize;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::PathBuf;
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
    pub data_dir: PathBuf,
    pub secret_key: [u8; 32],
}

/// Background task that sends booking reminders on a 60-second tick.
pub async fn run_reminder_loop(pool: SqlitePool, secret_key: [u8; 32]) {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;

        // Find bookings that need a reminder:
        // - status is confirmed
        // - reminder not yet sent
        // - event type has reminder_minutes set (> 0)
        // - start_at minus reminder_minutes <= now
        // - start_at > now (don't remind for past bookings)
        let due: Vec<(String, String, String, String, String, String, String, String, String, Option<String>, Option<String>, String)> = sqlx::query_as(
            "SELECT b.id, b.guest_name, b.guest_email, b.guest_timezone, b.start_at, b.end_at, et.title, u.name, COALESCE(u.booking_email, u.email), et.location_value, b.cancel_token, b.uid
             FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             JOIN accounts a ON a.id = et.account_id
             JOIN users u ON u.id = a.user_id
             WHERE b.status = 'confirmed'
               AND b.reminder_sent_at IS NULL
               AND et.reminder_minutes IS NOT NULL
               AND et.reminder_minutes > 0
               AND datetime(b.start_at, '-' || et.reminder_minutes || ' minutes') <= datetime('now')
               AND datetime(b.start_at) > datetime('now')",
        )
        .fetch_all(&pool)
        .await
        .unwrap_or_default();

        if due.is_empty() {
            continue;
        }

        let smtp_config = match crate::email::load_smtp_config(&pool, &secret_key).await {
            Ok(Some(cfg)) => cfg,
            _ => continue,
        };

        let base_url = std::env::var("CALRS_BASE_URL").ok();

        for (
            bid,
            guest_name,
            guest_email,
            guest_timezone,
            start_at,
            end_at,
            event_title,
            host_name,
            host_email,
            location_value,
            cancel_token,
            uid,
        ) in &due
        {
            let date = if start_at.len() >= 10 {
                &start_at[..10]
            } else {
                start_at
            };
            let start_time = if start_at.len() >= 16 {
                &start_at[11..16]
            } else {
                "00:00"
            };
            let end_time = if end_at.len() >= 16 {
                &end_at[11..16]
            } else {
                "00:00"
            };

            let location = location_value.as_ref().filter(|v| !v.is_empty()).cloned();

            let details = crate::email::BookingDetails {
                event_title: event_title.clone(),
                date: date.to_string(),
                start_time: start_time.to_string(),
                end_time: end_time.to_string(),
                guest_name: guest_name.clone(),
                guest_email: guest_email.clone(),
                guest_timezone: guest_timezone.clone(),
                host_name: host_name.clone(),
                host_email: host_email.clone(),
                uid: uid.clone(),
                notes: None,
                location,
            };

            let guest_cancel_url = cancel_token.as_ref().and_then(|t| {
                base_url
                    .as_ref()
                    .map(|base| format!("{}/booking/cancel/{}", base.trim_end_matches('/'), t))
            });

            let _ = crate::email::send_guest_reminder(
                &smtp_config,
                &details,
                guest_cancel_url.as_deref(),
            )
            .await;
            let _ = crate::email::send_host_reminder(&smtp_config, &details).await;

            // Mark reminder as sent
            let _ =
                sqlx::query("UPDATE bookings SET reminder_sent_at = datetime('now') WHERE id = ?")
                    .bind(bid)
                    .execute(&pool)
                    .await;
        }
    }
}

pub fn create_router(pool: SqlitePool, data_dir: PathBuf, secret_key: [u8; 32]) -> Router {
    let mut env = Environment::new();
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Lenient);
    env.set_loader(minijinja::path_loader("templates"));

    let state = Arc::new(AppState {
        pool,
        templates: env,
        // 10 login attempts per IP per 15 minutes
        login_limiter: RateLimiter::new(10, 900),
        secret_key,
        data_dir,
    });

    Router::new()
        .merge(crate::auth::auth_router())
        .route("/", get(root_redirect))
        .route("/dashboard", get(dashboard))
        .route("/dashboard/bookings/{id}/cancel", post(cancel_booking))
        .route("/dashboard/bookings/{id}/confirm", post(confirm_booking))
        .route(
            "/dashboard/event-types/new",
            get(new_event_type_form).post(create_event_type),
        )
        .route(
            "/dashboard/event-types/{slug}/edit",
            get(edit_event_type_form).post(update_event_type),
        )
        .route(
            "/dashboard/event-types/{slug}/toggle",
            post(toggle_event_type),
        )
        .route(
            "/dashboard/event-types/{slug}/delete",
            post(delete_event_type),
        )
        // Calendar source management
        .route(
            "/dashboard/sources/new",
            get(new_source_form).post(create_source),
        )
        .route("/dashboard/sources/{id}/remove", post(remove_source))
        .route("/dashboard/sources/{id}/test", post(test_source))
        .route("/dashboard/sources/{id}/sync", post(sync_source))
        .route(
            "/dashboard/sources/{id}/setup-write",
            get(setup_write_calendar),
        )
        .route(
            "/dashboard/sources/{id}/write-calendar",
            post(set_write_calendar),
        )
        // Settings
        .route(
            "/dashboard/settings",
            get(settings_page).post(settings_save),
        )
        // Troubleshoot
        .route("/dashboard/troubleshoot", get(troubleshoot))
        // Admin routes
        .route("/dashboard/admin", get(admin_dashboard))
        .route(
            "/dashboard/admin/users/{id}/toggle-role",
            post(admin_toggle_role),
        )
        .route(
            "/dashboard/admin/users/{id}/toggle-enabled",
            post(admin_toggle_enabled),
        )
        .route("/dashboard/admin/auth", post(admin_update_auth))
        .route("/dashboard/admin/oidc", post(admin_update_oidc))
        .route("/dashboard/admin/logo", post(admin_upload_logo))
        .route("/dashboard/admin/logo/delete", post(admin_delete_logo))
        .route("/dashboard/admin/impersonate/{id}", post(admin_impersonate))
        .route(
            "/dashboard/admin/stop-impersonate",
            post(admin_stop_impersonate),
        )
        // Group event type management
        .route(
            "/dashboard/group-event-types/new",
            get(new_group_event_type_form).post(create_group_event_type),
        )
        // Serve logo
        .route("/logo", get(serve_logo))
        // Group public routes (before the catch-all)
        .route("/g/{group_slug}", get(group_profile))
        .route("/g/{group_slug}/{slug}", get(show_group_slots))
        .route(
            "/g/{group_slug}/{slug}/book",
            get(show_group_book_form).post(handle_group_booking),
        )
        // User-scoped public booking routes
        .route("/booking/approve/{token}", get(approve_booking_by_token))
        .route(
            "/booking/decline/{token}",
            get(decline_booking_form).post(decline_booking_by_token),
        )
        .route(
            "/booking/cancel/{token}",
            get(guest_cancel_form).post(guest_cancel_booking),
        )
        .route("/u/{username}", get(user_profile))
        .route("/u/{username}/{slug}", get(show_slots_for_user))
        .route(
            "/u/{username}/{slug}/book",
            get(show_book_form_for_user).post(handle_booking_for_user),
        )
        // Legacy single-user routes (kept for backward compatibility)
        .route("/{slug}", get(show_slots))
        .route("/{slug}/book", get(show_book_form).post(handle_booking))
        .with_state(state)
}

/// Helper: create impersonation template context values (active, target_name, admin_name).
fn impersonation_ctx(auth_user: &crate::auth::AuthUser) -> (bool, String, String) {
    match &auth_user.impersonation {
        Some(info) => (true, info.target_name.clone(), info.admin_name.clone()),
        None => (false, String::new(), String::new()),
    }
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
    let user = &auth_user.user;

    let event_types: Vec<(String, String, i32, bool, i32, i64)> = sqlx::query_as(
        "SELECT et.slug, et.title, et.duration_min, et.enabled, et.requires_confirmation,
                (SELECT COUNT(*) FROM bookings b WHERE b.event_type_id = et.id AND b.status IN ('confirmed', 'pending')) as active_bookings
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
    let user_has_groups: bool =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM user_groups WHERE user_id = ?")
            .bind(&user.id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0)
            > 0;

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
        .map(|(slug, title, duration, enabled, req_conf, active_bookings)| {
            context! { slug => slug, title => title, duration_min => duration, enabled => enabled, requires_confirmation => *req_conf != 0, active_bookings => active_bookings }
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
        .map(
            |(id, name, url, username, last_synced, enabled, write_cal)| {
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
            },
        )
        .collect();

    let (impersonating, impersonating_name, _impersonating_admin) = impersonation_ctx(&auth_user);
    // When impersonating, show admin role to keep admin button accessible
    let effective_role = if impersonating {
        "admin".to_string()
    } else {
        user.role.clone()
    };

    Html(
        tmpl.render(context! {
            user_name => user.name,
            user_email => user.email,
            user_role => effective_role,
            username => user.username,
            event_types => et_ctx,
            group_event_types => group_et_ctx,
            user_has_groups => user_has_groups,
            pending_bookings => pending_ctx,
            bookings => bookings_ctx,
            sources => sources_ctx,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| format!("Template error: {}", e)),
    )
}

// --- Settings ---

#[derive(Deserialize)]
struct SettingsForm {
    name: String,
    booking_email: Option<String>,
}

async fn settings_page(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
) -> impl IntoResponse {
    settings_render(&state, &auth_user.user, None, None)
}

fn settings_render(
    state: &AppState,
    user: &crate::models::User,
    success: Option<&str>,
    error: Option<&str>,
) -> Html<String> {
    let tmpl = state.templates.get_template("settings.html").unwrap();
    Html(
        tmpl.render(context! {
            form_name => user.name,
            form_booking_email => user.booking_email.as_deref().unwrap_or(""),
            user_email => user.email,
            success => success.unwrap_or(""),
            error => error.unwrap_or(""),
        })
        .unwrap_or_else(|e| format!("Template error: {}", e)),
    )
}

async fn settings_save(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Form(form): Form<SettingsForm>,
) -> impl IntoResponse {
    let user = &auth_user.user;
    let name = form.name.trim().to_string();

    if name.is_empty() {
        return settings_render(&state, user, None, Some("Name cannot be empty.")).into_response();
    }

    let booking_email = form
        .booking_email
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let result = sqlx::query(
        "UPDATE users SET name = ?, booking_email = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(&name)
    .bind(&booking_email)
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

            // Re-fetch user to show updated values
            let updated_user = crate::auth::get_user_by_id(&state.pool, &user.id)
                .await
                .unwrap_or_else(|| user.clone());
            settings_render(&state, &updated_user, Some("Settings saved."), None).into_response()
        }
        Err(_) => {
            settings_render(&state, user, None, Some("Failed to save settings.")).into_response()
        }
    }
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
    let user = &auth_user.user;

    // Verify the booking belongs to this user and is confirmed
    let booking: Option<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    )> = sqlx::query_as(
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
    caldav_delete_booking(&state.pool, &state.secret_key, &user.id, &uid).await;

    // Send cancellation emails
    if let Ok(Some(smtp_config)) =
        crate::email::load_smtp_config(&state.pool, &state.secret_key).await
    {
        // Extract date and times from start_at/end_at
        let date = if start_at.len() >= 10 {
            &start_at[..10]
        } else {
            &start_at
        };
        let start_time = if start_at.len() >= 16 {
            &start_at[11..16]
        } else {
            "00:00"
        };
        let end_time = if end_at.len() >= 16 {
            &end_at[11..16]
        } else {
            "00:00"
        };

        let reason = form.reason.filter(|r| !r.trim().is_empty());

        let details = crate::email::CancellationDetails {
            event_title: event_title.clone(),
            date: date.to_string(),
            start_time: start_time.to_string(),
            end_time: end_time.to_string(),
            guest_name,
            guest_email,
            host_name: user.name.clone(),
            host_email: user
                .booking_email
                .clone()
                .unwrap_or_else(|| user.email.clone()),
            uid,
            reason,
            cancelled_by_host: true,
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
    let user = &auth_user.user;

    // Verify the booking belongs to this user and is pending
    let booking: Option<(String, String, String, String, String, String, String, Option<String>, Option<String>)> =
        sqlx::query_as(
            "SELECT b.id, b.uid, b.guest_name, b.guest_email, b.start_at, b.end_at, et.title, et.location_value, b.cancel_token
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
    ) = match booking {
        Some(b) => b,
        None => return Redirect::to("/dashboard").into_response(),
    };

    // Confirm the booking
    let _ = sqlx::query("UPDATE bookings SET status = 'confirmed' WHERE id = ?")
        .bind(&bid)
        .execute(&state.pool)
        .await;

    let date = if start_at.len() >= 10 {
        start_at[..10].to_string()
    } else {
        start_at.clone()
    };
    let start_time = if start_at.len() >= 16 {
        start_at[11..16].to_string()
    } else {
        "00:00".to_string()
    };
    let end_time = if end_at.len() >= 16 {
        end_at[11..16].to_string()
    } else {
        "00:00".to_string()
    };

    let details = crate::email::BookingDetails {
        event_title,
        date,
        start_time,
        end_time,
        guest_name,
        guest_email,
        guest_timezone: "UTC".to_string(),
        host_name: user.name.clone(),
        host_email: user
            .booking_email
            .clone()
            .unwrap_or_else(|| user.email.clone()),
        uid: uid.clone(),
        notes: None,
        location: location_value,
    };

    // Push to CalDAV calendar
    caldav_push_booking(&state.pool, &state.secret_key, &user.id, &uid, &details).await;

    // Send confirmation emails
    if let Ok(Some(smtp_config)) =
        crate::email::load_smtp_config(&state.pool, &state.secret_key).await
    {
        let guest_cancel_url = cancel_token.as_ref().and_then(|t| {
            std::env::var("CALRS_BASE_URL")
                .ok()
                .map(|base| format!("{}/booking/cancel/{}", base.trim_end_matches('/'), t))
        });
        let _ = crate::email::send_guest_confirmation(
            &smtp_config,
            &details,
            guest_cancel_url.as_deref(),
        )
        .await;
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
    location_type: Option<String>,         // "link", "phone", "in_person", "custom"
    location_value: Option<String>,
    // Availability schedule
    avail_days: Option<String>,  // comma-separated: "1,2,3,4,5"
    avail_start: Option<String>, // "09:00"
    avail_end: Option<String>,   // "17:00"
    // Group (optional)
    group_id: Option<String>,
    // Calendar selection (comma-separated IDs)
    calendar_ids: Option<String>,
    // Reminder
    reminder_minutes: Option<i32>,
}

async fn new_event_type_form(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
) -> impl IntoResponse {
    let user = &auth_user.user;

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

    // Get user's calendars (is_busy=1) for calendar selection
    let calendars: Vec<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT c.id, c.display_name, cs.name FROM calendars c
         JOIN caldav_sources cs ON cs.id = c.source_id
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND c.is_busy = 1
         ORDER BY cs.name, c.display_name",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let calendars_ctx: Vec<minijinja::Value> = calendars
        .iter()
        .map(|(id, display_name, source_name)| context! {
            id => id,
            name => format!("{} ({})", display_name.as_deref().unwrap_or("Unnamed"), source_name),
        })
        .collect();

    let tmpl = match state.templates.get_template("event_type_form.html") {
        Ok(t) => t,
        Err(e) => return Html(format!("Template error: {}", e)),
    };

    Html(
        tmpl.render(context! {
            editing => false,
            groups => groups_ctx,
            calendars => calendars_ctx,
            selected_calendar_ids => "",
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
            form_reminder_minutes => 1440,
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
    let user = &auth_user.user;

    // Find the user's account
    let account_id: Option<String> =
        sqlx::query_scalar("SELECT id FROM accounts WHERE user_id = ? LIMIT 1")
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
        return render_event_type_form_error(&state, "Slug is required.", &form, false)
            .into_response();
    }

    // Check uniqueness
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT id FROM event_types WHERE account_id = ? AND slug = ?")
            .bind(&account_id)
            .bind(&slug)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    if existing.is_some() {
        return render_event_type_form_error(
            &state,
            "An event type with this slug already exists.",
            &form,
            false,
        )
        .into_response();
    }

    let et_id = uuid::Uuid::new_v4().to_string();
    let requires_confirmation = form.requires_confirmation.as_deref() == Some("on");

    let location_type = form.location_type.as_deref().unwrap_or("link");
    let location_value = form
        .location_value
        .as_deref()
        .filter(|s| !s.trim().is_empty());

    // Check if a group_id was provided and it's non-empty
    let group_id = form.group_id.as_deref().filter(|s| !s.trim().is_empty());

    let reminder_minutes = form.reminder_minutes.filter(|&m| m > 0);

    let _ = sqlx::query(
        "INSERT INTO event_types (id, account_id, slug, title, description, duration_min, buffer_before, buffer_after, min_notice_min, requires_confirmation, location_type, location_value, group_id, created_by_user_id, reminder_minutes)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
    .bind(reminder_minutes)
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

    // Save calendar selections
    if let Some(ref cal_ids_str) = form.calendar_ids {
        for cal_id in cal_ids_str.split(',') {
            let cal_id = cal_id.trim();
            if !cal_id.is_empty() {
                let _ = sqlx::query(
                    "INSERT INTO event_type_calendars (event_type_id, calendar_id) VALUES (?, ?)",
                )
                .bind(&et_id)
                .bind(cal_id)
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
    let user = &auth_user.user;

    let et: Option<(String, String, String, Option<String>, i32, i32, i32, i32, i32, String, Option<String>, Option<i32>)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.description, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min, et.requires_confirmation, et.location_type, et.location_value, et.reminder_minutes
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         WHERE a.user_id = ? AND et.slug = ?",
    )
    .bind(&user.id)
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
        requires_conf,
        loc_type,
        loc_value,
        reminder_min,
    ) = match et {
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

    let avail_days: String = all_rules
        .iter()
        .map(|(d,)| d.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let (avail_start, avail_end) = rules
        .first()
        .map(|(_, s, e)| (s.clone(), e.clone()))
        .unwrap_or_else(|| ("09:00".to_string(), "17:00".to_string()));

    // Get user's calendars (is_busy=1) for calendar selection
    let calendars: Vec<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT c.id, c.display_name, cs.name FROM calendars c
         JOIN caldav_sources cs ON cs.id = c.source_id
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND c.is_busy = 1
         ORDER BY cs.name, c.display_name",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let calendars_ctx: Vec<minijinja::Value> = calendars
        .iter()
        .map(|(id, display_name, source_name)| context! {
            id => id,
            name => format!("{} ({})", display_name.as_deref().unwrap_or("Unnamed"), source_name),
        })
        .collect();

    // Get currently selected calendars for this event type
    let selected_cals: Vec<(String,)> =
        sqlx::query_as("SELECT calendar_id FROM event_type_calendars WHERE event_type_id = ?")
            .bind(&et_id)
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default();

    let selected_calendar_ids: String = selected_cals
        .iter()
        .map(|(id,)| id.as_str())
        .collect::<Vec<_>>()
        .join(",");

    let tmpl = match state.templates.get_template("event_type_form.html") {
        Ok(t) => t,
        Err(e) => return Html(format!("Template error: {}", e)),
    };

    Html(
        tmpl.render(context! {
            editing => true,
            original_slug => et_slug,
            calendars => calendars_ctx,
            selected_calendar_ids => selected_calendar_ids,
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
            form_reminder_minutes => reminder_min.unwrap_or(0),
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
    let user = &auth_user.user;

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
        let existing: Option<(String,)> =
            sqlx::query_as("SELECT id FROM event_types WHERE account_id = ? AND slug = ?")
                .bind(&account_id)
                .bind(&new_slug)
                .fetch_optional(&state.pool)
                .await
                .unwrap_or(None);

        if existing.is_some() {
            return render_event_type_form_error(
                &state,
                "An event type with this slug already exists.",
                &form,
                true,
            )
            .into_response();
        }
    }

    let location_type = form.location_type.as_deref().unwrap_or("link");
    let location_value = form
        .location_value
        .as_deref()
        .filter(|s| !s.trim().is_empty());

    let reminder_minutes = form.reminder_minutes.filter(|&m| m > 0);

    let _ = sqlx::query(
        "UPDATE event_types SET slug = ?, title = ?, description = ?, duration_min = ?, buffer_before = ?, buffer_after = ?, min_notice_min = ?, requires_confirmation = ?, location_type = ?, location_value = ?, reminder_minutes = ? WHERE id = ?",
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
    .bind(reminder_minutes)
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

    // Update calendar selections: delete old, insert new
    let _ = sqlx::query("DELETE FROM event_type_calendars WHERE event_type_id = ?")
        .bind(&et_id)
        .execute(&state.pool)
        .await;

    if let Some(ref cal_ids_str) = form.calendar_ids {
        for cal_id in cal_ids_str.split(',') {
            let cal_id = cal_id.trim();
            if !cal_id.is_empty() {
                let _ = sqlx::query(
                    "INSERT INTO event_type_calendars (event_type_id, calendar_id) VALUES (?, ?)",
                )
                .bind(&et_id)
                .bind(cal_id)
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
    let user = &auth_user.user;

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

async fn delete_event_type(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(slug): Path<String>,
) -> impl IntoResponse {
    let user = &auth_user.user;

    // Find the event type owned by this user
    let et: Option<(String,)> = sqlx::query_as(
        "SELECT et.id FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         WHERE et.slug = ? AND a.user_id = ?",
    )
    .bind(&slug)
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let et_id = match et {
        Some((id,)) => id,
        None => return Redirect::to("/dashboard"),
    };

    // Check for active bookings (confirmed or pending)
    let active_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM bookings WHERE event_type_id = ? AND status IN ('confirmed', 'pending')",
    )
    .bind(&et_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    if active_count > 0 {
        // Can't delete — has active bookings. Just redirect back.
        return Redirect::to("/dashboard");
    }

    // Delete in order: availability_rules, availability_overrides, event_type_calendars, bookings (past/cancelled), then event_type
    let _ = sqlx::query("DELETE FROM availability_rules WHERE event_type_id = ?")
        .bind(&et_id)
        .execute(&state.pool)
        .await;
    let _ = sqlx::query("DELETE FROM availability_overrides WHERE event_type_id = ?")
        .bind(&et_id)
        .execute(&state.pool)
        .await;
    let _ = sqlx::query("DELETE FROM event_type_calendars WHERE event_type_id = ?")
        .bind(&et_id)
        .execute(&state.pool)
        .await;
    let _ = sqlx::query("DELETE FROM bookings WHERE event_type_id = ?")
        .bind(&et_id)
        .execute(&state.pool)
        .await;
    let _ = sqlx::query("DELETE FROM event_types WHERE id = ?")
        .bind(&et_id)
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
        (
            "nextcloud",
            "Nextcloud",
            "https://cloud.example.com/remote.php/dav",
        ),
        (
            "fastmail",
            "Fastmail",
            "https://caldav.fastmail.com/dav/calendars/user/you@fastmail.com/",
        ),
        ("icloud", "iCloud", "https://caldav.icloud.com/"),
        (
            "google",
            "Google",
            "https://apidata.googleusercontent.com/caldav/v2/your@gmail.com/",
        ),
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
    let user = &auth_user.user;

    let account_id: Option<String> =
        sqlx::query_scalar("SELECT id FROM accounts WHERE user_id = ? LIMIT 1")
            .bind(&user.id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    let account_id = match account_id {
        Some(id) => id,
        None => {
            return render_source_form_error(
                &state,
                "No scheduling account found. Please contact an administrator.",
                &form,
            )
            .into_response()
        }
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
    let password_enc = match crate::crypto::encrypt_password(&state.secret_key, &form.password) {
        Ok(enc) => enc,
        Err(_) => return Html("Encryption error.".to_string()).into_response(),
    };

    let _ = sqlx::query(
        "INSERT INTO caldav_sources (id, account_id, name, url, username, password_enc) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&account_id)
    .bind(&name)
    .bind(&url)
    .bind(&username)
    .bind(&password_enc)
    .execute(&state.pool)
    .await;

    // Auto-sync immediately after creating the source, then redirect to
    // write-back setup if calendars were found.
    let (messages, calendar_count) =
        run_sync(&state.pool, &id, &url, &username, &form.password).await;

    if calendar_count > 0 {
        let joined_messages = messages.join("\n");
        let encoded_messages = urlencoding::encode(&joined_messages);
        return Redirect::to(&format!(
            "/dashboard/sources/{}/setup-write?sync_messages={}",
            id, encoded_messages
        ))
        .into_response();
    }

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
    let user = &auth_user.user;

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
    let user = &auth_user.user;

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

    let (url, username, password_enc, name) = match source {
        Some(s) => s,
        None => return Html("Source not found.".to_string()).into_response(),
    };

    let password = match crate::crypto::decrypt_password(&state.secret_key, &password_enc) {
        Ok(p) => p,
        Err(_) => return Html("Failed to decrypt stored credentials.".to_string()).into_response(),
    };

    let client = crate::caldav::CaldavClient::new(&url, &username, &password);
    let result = match client.check_connection().await {
        Ok(true) => format!("'{}' — connection OK, CalDAV supported.", name),
        Ok(false) => format!(
            "'{}' — connected but CalDAV not explicitly detected. Sync may still work.",
            name
        ),
        Err(e) => format!("'{}' — connection failed: {}", name, e),
    };

    // Return a simple page with back link
    let tmpl = match state.templates.get_template("source_test.html") {
        Ok(t) => t,
        Err(_) => {
            return Html(format!(
                "<p>{}</p><p><a href=\"/dashboard\">Back to dashboard</a></p>",
                result
            ))
            .into_response()
        }
    };
    Html(
        tmpl.render(context! { result => result })
            .unwrap_or_else(|e| format!("Template error: {}", e)),
    )
    .into_response()
}

/// Runs CalDAV discovery + sync for a source. Returns (messages, calendar_count).
/// On error during discovery, returns partial messages with 0 calendars.
async fn run_sync(
    pool: &SqlitePool,
    source_id: &str,
    url: &str,
    username: &str,
    password: &str,
) -> (Vec<String>, usize) {
    let client = crate::caldav::CaldavClient::new(url, username, password);
    let mut messages: Vec<String> = Vec::new();

    let principal = match client.discover_principal().await {
        Ok(p) => p,
        Err(e) => {
            messages.push(format!("Could not discover principal: {}", e));
            return (messages, 0);
        }
    };

    let calendar_home = match client.discover_calendar_home(&principal).await {
        Ok(h) => h,
        Err(e) => {
            messages.push(format!("Could not discover calendar home: {}", e));
            return (messages, 0);
        }
    };

    let calendars = match client.list_calendars(&calendar_home).await {
        Ok(c) => c,
        Err(e) => {
            messages.push(format!("Could not list calendars: {}", e));
            return (messages, 0);
        }
    };

    let mut total_events = 0usize;

    for cal_info in &calendars {
        let display = cal_info.display_name.as_deref().unwrap_or(&cal_info.href);

        // Upsert calendar record
        let cal_id: String = match sqlx::query_scalar::<_, String>(
            "SELECT id FROM calendars WHERE source_id = ? AND href = ?",
        )
        .bind(source_id)
        .bind(&cal_info.href)
        .fetch_optional(pool)
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
                .bind(source_id)
                .bind(&cal_info.href)
                .bind(&cal_info.display_name)
                .bind(&cal_info.color)
                .bind(&cal_info.ctag)
                .execute(pool)
                .await;
                id
            }
        };

        // Fetch events
        match client.fetch_events(&cal_info.href).await {
            Ok(raw_events) => {
                let mut count = 0;
                for raw in &raw_events {
                    let vevent_blocks = split_vevents(&raw.ical_data);
                    for vevent in &vevent_blocks {
                        let uid = extract_vevent_field(vevent, "UID")
                            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                        let summary = extract_vevent_field(vevent, "SUMMARY");
                        let start_at = extract_vevent_field(vevent, "DTSTART").unwrap_or_default();
                        let end_at = extract_vevent_field(vevent, "DTEND").unwrap_or_default();
                        let location = extract_vevent_field(vevent, "LOCATION");
                        let description = extract_vevent_field(vevent, "DESCRIPTION");
                        let status = extract_vevent_field(vevent, "STATUS");
                        let rrule = extract_vevent_field(vevent, "RRULE");
                        let recurrence_id = extract_vevent_field(vevent, "RECURRENCE-ID");
                        let timezone = extract_vevent_tzid(vevent, "DTSTART");

                        let event_id = uuid::Uuid::new_v4().to_string();
                        let _ = sqlx::query(
                            "INSERT INTO events (id, calendar_id, uid, summary, start_at, end_at, location, description, status, rrule, raw_ical, recurrence_id, timezone)
                             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                             ON CONFLICT(uid, COALESCE(recurrence_id, '')) DO UPDATE SET
                               summary = excluded.summary,
                               start_at = excluded.start_at,
                               end_at = excluded.end_at,
                               location = excluded.location,
                               description = excluded.description,
                               status = excluded.status,
                               rrule = excluded.rrule,
                               raw_ical = excluded.raw_ical,
                               recurrence_id = excluded.recurrence_id,
                               timezone = excluded.timezone,
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
                        .bind(&timezone)
                        .execute(pool)
                        .await;

                        count += 1;
                    }
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
        .bind(source_id)
        .execute(pool)
        .await;

    messages.push(format!(
        "Sync complete: {} calendars, {} events total.",
        calendars.len(),
        total_events
    ));

    (messages, calendars.len())
}

async fn sync_source(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(source_id): Path<String>,
) -> impl IntoResponse {
    let user = &auth_user.user;

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

    let (sid, url, username, password_enc, name) = match source {
        Some(s) => s,
        None => return Html("Source not found.".to_string()).into_response(),
    };

    let password = match crate::crypto::decrypt_password(&state.secret_key, &password_enc) {
        Ok(p) => p,
        Err(_) => return Html("Failed to decrypt stored credentials.".to_string()).into_response(),
    };

    let (messages, calendar_count) = run_sync(&state.pool, &sid, &url, &username, &password).await;

    // If write_calendar_href is not yet configured and we found calendars,
    // redirect to the write-calendar setup page (onboarding flow).
    let write_href: Option<String> =
        sqlx::query_scalar("SELECT write_calendar_href FROM caldav_sources WHERE id = ?")
            .bind(&sid)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None)
            .flatten();

    if write_href.is_none() && calendar_count > 0 {
        let joined_messages = messages.join("\n");
        let encoded_messages = urlencoding::encode(&joined_messages);
        return Redirect::to(&format!(
            "/dashboard/sources/{}/setup-write?sync_messages={}",
            sid, encoded_messages
        ))
        .into_response();
    }

    render_sync_result(&state, &name, &messages).into_response()
}

fn render_sync_result(state: &AppState, source_name: &str, messages: &[String]) -> Html<String> {
    let tmpl = match state.templates.get_template("source_test.html") {
        Ok(t) => t,
        Err(_) => {
            return Html(format!(
                "<p>{}</p><p><a href=\"/dashboard\">Back to dashboard</a></p>",
                messages.join("<br>")
            ))
        }
    };
    Html(
        tmpl.render(context! { result => messages.join("\n"), source_name => source_name })
            .unwrap_or_else(|e| format!("Template error: {}", e)),
    )
}

#[derive(Deserialize)]
struct SetupWriteQuery {
    sync_messages: Option<String>,
}

async fn setup_write_calendar(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(source_id): Path<String>,
    Query(query): Query<SetupWriteQuery>,
) -> impl IntoResponse {
    let user = &auth_user.user;

    // Fetch source name and verify ownership
    let source: Option<(String, String)> = sqlx::query_as(
        "SELECT cs.id, cs.name FROM caldav_sources cs
         JOIN accounts a ON a.id = cs.account_id
         WHERE cs.id = ? AND a.user_id = ?",
    )
    .bind(&source_id)
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (_sid, source_name) = match source {
        Some(s) => s,
        None => return Redirect::to("/dashboard").into_response(),
    };

    // Get calendars for this source, sorted by event count (most events first)
    let calendars: Vec<(String, Option<String>, Option<String>, i64)> = sqlx::query_as(
        "SELECT c.href, c.display_name, c.color,
                (SELECT COUNT(*) FROM events e WHERE e.calendar_id = c.id) as event_count
         FROM calendars c WHERE c.source_id = ?
         ORDER BY event_count DESC, c.display_name",
    )
    .bind(&source_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    if calendars.is_empty() {
        return Redirect::to("/dashboard").into_response();
    }

    let cal_values: Vec<minijinja::Value> = calendars
        .iter()
        .map(|(href, name, color, event_count)| {
            context! {
                href => href,
                name => name.as_deref().unwrap_or(href),
                color => color,
                event_count => event_count,
            }
        })
        .collect();

    let tmpl = match state.templates.get_template("source_write_setup.html") {
        Ok(t) => t,
        Err(e) => return Html(format!("Template error: {}", e)).into_response(),
    };

    Html(
        tmpl.render(context! {
            source_id => source_id,
            source_name => source_name,
            calendars => cal_values,
            sync_messages => query.sync_messages,
        })
        .unwrap_or_else(|e| format!("Template error: {}", e)),
    )
    .into_response()
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
    let user = &auth_user.user;

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

fn render_event_type_form_error(
    state: &AppState,
    error: &str,
    form: &EventTypeForm,
    editing: bool,
) -> Html<String> {
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
    let user = &auth_user.user;

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
    let user = &auth_user.user;

    let group_id = match form.group_id.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(gid) => gid.to_string(),
        None => return Redirect::to("/dashboard").into_response(),
    };

    // Verify user belongs to this group
    let membership: Option<(String,)> =
        sqlx::query_as("SELECT group_id FROM user_groups WHERE user_id = ? AND group_id = ?")
            .bind(&user.id)
            .bind(&group_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    if membership.is_none() {
        return Html("You don't belong to this group.".to_string()).into_response();
    }

    // Find the user's account
    let account_id: Option<String> =
        sqlx::query_scalar("SELECT id FROM accounts WHERE user_id = ? LIMIT 1")
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
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT id FROM event_types WHERE group_id = ? AND slug = ?")
            .bind(&group_id)
            .bind(&slug)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    if existing.is_some() {
        return Html("An event type with this slug already exists in this group.".to_string())
            .into_response();
    }

    let et_id = uuid::Uuid::new_v4().to_string();
    let requires_confirmation = form.requires_confirmation.as_deref() == Some("on");
    let location_type = form.location_type.as_deref().unwrap_or("link");
    let location_value = form
        .location_value
        .as_deref()
        .filter(|s| !s.trim().is_empty());

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
    let group: Option<(String, String)> =
        sqlx::query_as("SELECT id, name FROM groups WHERE slug = ?")
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
        group_name,
    ) = match et {
        Some(e) => e,
        None => return Html("Event type not found.".to_string()),
    };

    let guest_tz = parse_guest_tz(query.tz.as_deref());
    let host_tz = get_host_tz(&state.pool, &et_id).await;
    let guest_tz_name = guest_tz.name().to_string();

    let week = query.week.unwrap_or(0).max(0);
    let days_per_page = 7;
    let start_offset = week * days_per_page;

    // Build group busy source: fetch busy times per member
    let now_host = Utc::now().with_timezone(&host_tz).naive_local();
    let end_date = now_host.date() + Duration::days((start_offset + days_per_page) as i64);
    let window_end = end_date.and_hms_opt(23, 59, 59).unwrap_or(now_host);

    let group_id: Option<String> =
        sqlx::query_scalar("SELECT group_id FROM event_types WHERE id = ?")
            .bind(&et_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None)
            .flatten();
    let busy = if let Some(ref gid) = group_id {
        let members: Vec<(String,)> = sqlx::query_as(
            "SELECT u.id FROM users u JOIN user_groups ug ON ug.user_id = u.id WHERE ug.group_id = ? AND u.enabled = 1",
        ).bind(gid).fetch_all(&state.pool).await.unwrap_or_default();
        // Sync all group members' calendars if stale
        for (uid,) in &members {
            crate::commands::sync::sync_if_stale(&state.pool, &state.secret_key, uid).await;
        }
        let mut member_busy = HashMap::new();
        for (uid,) in &members {
            member_busy.insert(
                uid.clone(),
                fetch_busy_times_for_user(
                    &state.pool,
                    uid,
                    now_host,
                    window_end,
                    host_tz,
                    Some(&et_id),
                )
                .await,
            );
        }
        BusySource::Group(member_busy)
    } else {
        // Fallback for individual event type (shouldn't happen on group route, but be safe)
        let owner_id: String = sqlx::query_scalar(
            "SELECT a.user_id FROM accounts a JOIN event_types et ON et.account_id = a.id WHERE et.id = ?",
        )
        .bind(&et_id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
        .unwrap_or_default();
        BusySource::Individual(
            fetch_busy_times_for_user(
                &state.pool,
                &owner_id,
                now_host,
                window_end,
                host_tz,
                Some(&et_id),
            )
            .await,
        )
    };

    let slot_days = compute_slots(
        &state.pool,
        &et_id,
        duration,
        buf_before,
        buf_after,
        min_notice,
        start_offset,
        days_per_page,
        host_tz,
        guest_tz,
        busy,
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
        group_id,
    ) = match et {
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
    let host_tz = get_host_tz(&state.pool, &et_id).await;
    let assigned = pick_group_member(
        &state.pool,
        &group_id,
        &et_id,
        slot_start,
        slot_end,
        buffer_before,
        buffer_after,
        host_tz,
    )
    .await;

    let (assigned_user_id, host_name, host_email) = match assigned {
        Some(a) => a,
        None => {
            return Html("No team members are available for this slot.".to_string()).into_response()
        }
    };

    let id = uuid::Uuid::new_v4().to_string();
    let uid = format!("{}@calrs", uuid::Uuid::new_v4());
    let cancel_token = uuid::Uuid::new_v4().to_string();
    let reschedule_token = uuid::Uuid::new_v4().to_string();
    let start_at = slot_start.format("%Y-%m-%dT%H:%M:%S").to_string();
    let end_at = slot_end.format("%Y-%m-%dT%H:%M:%S").to_string();
    let guest_tz = parse_guest_tz(form.tz.as_deref());
    let guest_timezone = guest_tz.name().to_string();

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

    sqlx::query(
        "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, notes, start_at, end_at, status, cancel_token, reschedule_token, assigned_user_id, confirm_token)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
    .execute(&state.pool)
    .await
    .unwrap();

    // Send emails if SMTP is configured
    if let Ok(Some(smtp_config)) =
        crate::email::load_smtp_config(&state.pool, &state.secret_key).await
    {
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

        let base_url = std::env::var("CALRS_BASE_URL").ok();
        let guest_cancel_url = base_url.as_ref().map(|base| {
            format!(
                "{}/booking/cancel/{}",
                base.trim_end_matches('/'),
                cancel_token
            )
        });

        if needs_approval {
            let _ = crate::email::send_host_approval_request(
                &smtp_config,
                &details,
                &id,
                confirm_token.as_deref(),
                base_url.as_deref(),
            )
            .await;
            let _ = crate::email::send_guest_pending_notice(
                &smtp_config,
                &details,
                guest_cancel_url.as_deref(),
            )
            .await;
        } else {
            let _ = crate::email::send_guest_confirmation(
                &smtp_config,
                &details,
                guest_cancel_url.as_deref(),
            )
            .await;
            let _ = crate::email::send_host_notification(&smtp_config, &details).await;
            // Push confirmed booking to assigned member's CalDAV
            caldav_push_booking(
                &state.pool,
                &state.secret_key,
                &assigned_user_id,
                &uid,
                &details,
            )
            .await;
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

// --- User profile page ---

async fn user_profile(
    State(state): State<Arc<AppState>>,
    Path(username): Path<String>,
) -> impl IntoResponse {
    let user: Option<(String, String)> =
        sqlx::query_as("SELECT id, name FROM users WHERE username = ? AND enabled = 1")
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
    let et: Option<(String, String, String, Option<String>, i32, i32, i32, i32, String, Option<String>, String, String)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.description, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min, et.location_type, et.location_value, u.id, u.name
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
        buf_before,
        buf_after,
        min_notice,
        loc_type,
        loc_value,
        host_user_id,
        host_name,
    ) = match et {
        Some(e) => e,
        None => return Html("Event type not found.".to_string()),
    };

    // Sync calendars if stale before computing availability
    crate::commands::sync::sync_if_stale(&state.pool, &state.secret_key, &host_user_id).await;

    let guest_tz = parse_guest_tz(query.tz.as_deref());
    let host_tz = get_host_tz(&state.pool, &et_id).await;
    let guest_tz_name = guest_tz.name().to_string();

    let week = query.week.unwrap_or(0).max(0);
    let days_per_page = 7;
    let start_offset = week * days_per_page;
    let now_host = Utc::now().with_timezone(&host_tz).naive_local();
    let end_date = now_host.date() + Duration::days((start_offset + days_per_page) as i64);
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
        days_per_page,
        host_tz,
        guest_tz,
        busy,
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

    let host_name: String = sqlx::query_scalar("SELECT name FROM users WHERE username = ?")
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
    let et: Option<(String, String, String, i32, i32, i32, i32, i32, String, Option<String>, String)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min, et.requires_confirmation, et.location_type, et.location_value, u.id
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
        _et_slug,
        et_title,
        duration,
        buffer_before,
        buffer_after,
        min_notice,
        requires_confirmation,
        loc_type,
        loc_value,
        host_user_id,
    ) = match et {
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

    let host_tz = get_host_tz(&state.pool, &et_id).await;
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
        return Html("This slot is no longer available.".to_string()).into_response();
    }

    let id = uuid::Uuid::new_v4().to_string();
    let uid = format!("{}@calrs", uuid::Uuid::new_v4());
    let cancel_token = uuid::Uuid::new_v4().to_string();
    let reschedule_token = uuid::Uuid::new_v4().to_string();
    let start_at = slot_start.format("%Y-%m-%dT%H:%M:%S").to_string();
    let end_at = slot_end.format("%Y-%m-%dT%H:%M:%S").to_string();
    let guest_tz = parse_guest_tz(form.tz.as_deref());
    let guest_timezone = guest_tz.name().to_string();

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

    sqlx::query(
        "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, notes, start_at, end_at, status, cancel_token, reschedule_token, confirm_token)
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
    .bind(&confirm_token)
    .execute(&state.pool)
    .await
    .unwrap();

    // Send emails if SMTP is configured
    if let Ok(Some(smtp_config)) =
        crate::email::load_smtp_config(&state.pool, &state.secret_key).await
    {
        let host: Option<(String, String)> = sqlx::query_as(
            "SELECT u.name, COALESCE(u.booking_email, u.email) FROM users u WHERE u.username = ?",
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

            let base_url = std::env::var("CALRS_BASE_URL").ok();
            let guest_cancel_url = base_url.as_ref().map(|base| {
                format!(
                    "{}/booking/cancel/{}",
                    base.trim_end_matches('/'),
                    cancel_token
                )
            });

            if needs_approval {
                let _ = crate::email::send_host_approval_request(
                    &smtp_config,
                    &details,
                    &id,
                    confirm_token.as_deref(),
                    base_url.as_deref(),
                )
                .await;
                let _ = crate::email::send_guest_pending_notice(
                    &smtp_config,
                    &details,
                    guest_cancel_url.as_deref(),
                )
                .await;
            } else {
                let _ = crate::email::send_guest_confirmation(
                    &smtp_config,
                    &details,
                    guest_cancel_url.as_deref(),
                )
                .await;
                let _ = crate::email::send_host_notification(&smtp_config, &details).await;
                // Push confirmed booking to CalDAV
                let host_user_id: Option<String> =
                    sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
                        .bind(&username)
                        .fetch_optional(&state.pool)
                        .await
                        .unwrap_or(None);
                if let Some(uid_user) = host_user_id {
                    caldav_push_booking(&state.pool, &state.secret_key, &uid_user, &uid, &details)
                        .await;
                }
            }
        }
    }

    let host_name: String = sqlx::query_scalar("SELECT name FROM users WHERE username = ?")
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
        return d.and_hms_opt(0, 0, 0);
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return d.and_hms_opt(0, 0, 0);
    }
    None
}

/// Pick an available group member for a booking slot.
/// Returns (user_id, name, email) of the member with fewest recent bookings.
async fn pick_group_member(
    pool: &SqlitePool,
    group_id: &str,
    event_type_id: &str,
    slot_start: NaiveDateTime,
    slot_end: NaiveDateTime,
    buffer_before: i32,
    buffer_after: i32,
    host_tz: Tz,
) -> Option<(String, String, String)> {
    let buf_start = slot_start - Duration::minutes(buffer_before as i64);
    let buf_end = slot_end + Duration::minutes(buffer_after as i64);

    let members: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT u.id, u.name, COALESCE(u.booking_email, u.email) FROM users u JOIN user_groups ug ON ug.user_id = u.id WHERE ug.group_id = ? AND u.enabled = 1",
    )
    .bind(group_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut available_members = Vec::new();

    for (user_id, name, email) in &members {
        let busy = fetch_busy_times_for_user(
            pool,
            user_id,
            buf_start,
            buf_end,
            host_tz,
            Some(event_type_id),
        )
        .await;
        if !has_conflict(&busy, buf_start, buf_end) {
            available_members.push((user_id.clone(), name.clone(), email.clone()));
        }
    }

    if available_members.is_empty() {
        return None;
    }

    // Among available members, pick the one with fewest bookings in last 30 days
    let thirty_days_ago = (Utc::now() - Duration::days(30))
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();
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

struct SlotDay {
    date: String,
    label: String,
    slots: Vec<SlotTime>,
}

struct SlotTime {
    start: String,      // guest TZ display
    end: String,        // guest TZ display
    host_date: String,  // YYYY-MM-DD in host TZ (for booking)
    host_time: String,  // HH:MM in host TZ (for booking)
    guest_date: String, // YYYY-MM-DD in guest TZ (for grouping by day)
}

// --- Shared busy-time helpers ---

/// Expand recurring events into (start, end) pairs within a time window.
/// Tuples are (start_at, end_at, rrule, raw_ical, timezone).
fn expand_recurring_into_busy(
    recurring: &[(String, String, String, Option<String>, Option<String>)],
    window_start: NaiveDateTime,
    window_end: NaiveDateTime,
    host_tz: Tz,
) -> Vec<(NaiveDateTime, NaiveDateTime)> {
    let mut result = Vec::new();
    for (s, e, rrule_str, raw_ical, event_tz) in recurring {
        if let (Some(ev_start), Some(ev_end)) = (parse_datetime(s), parse_datetime(e)) {
            let exdates = raw_ical
                .as_deref()
                .map(crate::rrule::extract_exdates)
                .unwrap_or_default();
            // Expand RRULE in the event's own timezone (correct for DST)
            let occurrences = crate::rrule::expand_rrule(
                ev_start,
                ev_end,
                rrule_str,
                &exdates,
                window_start,
                window_end,
            );
            // Convert each occurrence to host timezone
            for (os, oe) in occurrences {
                let cs = convert_event_to_tz(os, event_tz.as_deref(), host_tz);
                let ce = convert_event_to_tz(oe, event_tz.as_deref(), host_tz);
                result.push((cs, ce));
            }
        }
    }
    result
}

/// Fetch busy times for a specific user (events from their calendars + their bookings).
/// Event times are converted from their stored timezone to `host_tz`.
async fn fetch_busy_times_for_user(
    pool: &SqlitePool,
    user_id: &str,
    window_start: NaiveDateTime,
    window_end: NaiveDateTime,
    host_tz: Tz,
    event_type_id: Option<&str>,
) -> Vec<(NaiveDateTime, NaiveDateTime)> {
    let end_compact = window_end.format("%Y%m%d").to_string();
    let start_compact = window_start.format("%Y%m%dT%H%M%S").to_string();
    let end_iso = window_end.format("%Y-%m-%dT%H:%M:%S").to_string();
    let start_iso = window_start.format("%Y-%m-%dT%H:%M:%S").to_string();

    // Empty string means NOT EXISTS is always true (no rows match), so all calendars pass
    let et_id_for_filter = event_type_id.unwrap_or("");

    let events: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT e.start_at, e.end_at, e.timezone FROM events e
         JOIN calendars c ON c.id = e.calendar_id
         JOIN caldav_sources cs ON cs.id = c.source_id
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND c.is_busy = 1
           AND (NOT EXISTS (SELECT 1 FROM event_type_calendars WHERE event_type_id = ?)
                OR c.id IN (SELECT calendar_id FROM event_type_calendars WHERE event_type_id = ?))
           AND (e.rrule IS NULL OR e.rrule = '')
           AND (e.status IS NULL OR e.status != 'CANCELLED')
           AND ((e.start_at <= ? AND e.end_at >= ?) OR (e.start_at <= ? AND e.end_at >= ?))",
    )
    .bind(user_id)
    .bind(et_id_for_filter)
    .bind(et_id_for_filter)
    .bind(&end_compact)
    .bind(&start_compact)
    .bind(&end_iso)
    .bind(&start_iso)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut busy: Vec<(NaiveDateTime, NaiveDateTime)> = events
        .iter()
        .filter_map(|(s, e, tz)| {
            let start = convert_event_to_tz(parse_datetime(s)?, tz.as_deref(), host_tz);
            let end = convert_event_to_tz(parse_datetime(e)?, tz.as_deref(), host_tz);
            Some((start, end))
        })
        .collect();

    let end_compact_rrule = window_end.format("%Y%m%dT235959").to_string();
    let recurring: Vec<(String, String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT e.start_at, e.end_at, e.rrule, e.raw_ical, e.timezone FROM events e
         JOIN calendars c ON c.id = e.calendar_id
         JOIN caldav_sources cs ON cs.id = c.source_id
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND c.is_busy = 1
           AND (NOT EXISTS (SELECT 1 FROM event_type_calendars WHERE event_type_id = ?)
                OR c.id IN (SELECT calendar_id FROM event_type_calendars WHERE event_type_id = ?))
           AND (e.status IS NULL OR e.status != 'CANCELLED')
           AND e.rrule IS NOT NULL AND e.rrule != '' AND (e.start_at <= ? OR e.start_at <= ?)",
    )
    .bind(user_id)
    .bind(et_id_for_filter)
    .bind(et_id_for_filter)
    .bind(&end_iso)
    .bind(&end_compact_rrule)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    busy.extend(expand_recurring_into_busy(
        &recurring,
        window_start,
        window_end,
        host_tz,
    ));

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
    .bind(&start_iso)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    for (s, e) in &bookings {
        if let (Some(start), Some(end)) = (parse_datetime(s), parse_datetime(e)) {
            busy.push((start, end));
        }
    }

    busy
}

/// Check if any busy period overlaps with [buf_start, buf_end).
fn has_conflict(
    busy: &[(NaiveDateTime, NaiveDateTime)],
    buf_start: NaiveDateTime,
    buf_end: NaiveDateTime,
) -> bool {
    busy.iter().any(|(s, e)| *s < buf_end && *e > buf_start)
}

// --- Busy source for unified slot computation ---

enum BusySource {
    /// Flat list of busy times (individual event type)
    Individual(Vec<(NaiveDateTime, NaiveDateTime)>),
    /// Per-member busy times; slot is available if ANY member is free
    Group(HashMap<String, Vec<(NaiveDateTime, NaiveDateTime)>>),
}

/// Compute available slots for an event type.
/// Caller provides pre-fetched busy times via BusySource.
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
    busy: BusySource,
) -> Vec<SlotDay> {
    let rules: Vec<(i32, String, String)> = sqlx::query_as(
        "SELECT day_of_week, start_time, end_time FROM availability_rules WHERE event_type_id = ?",
    )
    .bind(et_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let now_host = Utc::now().with_timezone(&host_tz).naive_local();
    let min_start = now_host + Duration::minutes(min_notice as i64);

    let slot_duration = Duration::minutes(duration as i64);
    let mut result = Vec::new();

    for day_offset in start_offset..(start_offset + days_ahead) {
        let date = now_host.date() + Duration::days(day_offset as i64);
        let weekday = date.weekday().num_days_from_sunday() as i32;

        let day_rules: Vec<&(i32, String, String)> =
            rules.iter().filter(|(d, _, _)| *d == weekday).collect();

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
                    cursor += Duration::minutes(duration as i64);
                    continue;
                }

                let buf_start = slot_start - Duration::minutes(buffer_before as i64);
                let buf_end = slot_end + Duration::minutes(buffer_after as i64);

                let is_free = match &busy {
                    BusySource::Individual(times) => !has_conflict(times, buf_start, buf_end),
                    BusySource::Group(member_busy) => member_busy
                        .values()
                        .any(|times| !has_conflict(times, buf_start, buf_end)),
                };

                if is_free {
                    let slot_start_utc = host_tz
                        .from_local_datetime(&slot_start)
                        .earliest()
                        .unwrap_or_else(|| host_tz.from_utc_datetime(&slot_start))
                        .with_timezone(&Utc);
                    let slot_end_utc = host_tz
                        .from_local_datetime(&slot_end)
                        .earliest()
                        .unwrap_or_else(|| host_tz.from_utc_datetime(&slot_end))
                        .with_timezone(&Utc);
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

                cursor += Duration::minutes(duration as i64);
            }
        }

        if !day_slots.is_empty() {
            let mut guest_days: std::collections::BTreeMap<String, Vec<SlotTime>> =
                std::collections::BTreeMap::new();
            for slot in day_slots {
                guest_days
                    .entry(slot.guest_date.clone())
                    .or_default()
                    .push(slot);
            }
            for (guest_date_str, slots) in guest_days {
                if let Ok(gd) = NaiveDate::parse_from_str(&guest_date_str, "%Y-%m-%d") {
                    if !result.iter().any(|d: &SlotDay| d.date == guest_date_str) {
                        result.push(SlotDay {
                            date: guest_date_str,
                            label: gd.format("%A, %B %-d").to_string(),
                            slots,
                        });
                    } else if let Some(existing) =
                        result.iter_mut().find(|d| d.date == guest_date_str)
                    {
                        existing.slots.extend(slots);
                    }
                }
            }
        }
    }

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
    tz.and_then(|s| s.parse::<Tz>().ok()).unwrap_or_else(|| {
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

    let (et_id, et_slug, et_title, et_desc, duration, buf_before, buf_after, min_notice) = match et
    {
        Some(e) => e,
        None => return Html("Event type not found.".to_string()),
    };

    let host_info: Option<(String, String)> = sqlx::query_as(
        "SELECT a.user_id, a.name FROM accounts a JOIN event_types et ON et.account_id = a.id WHERE et.id = ?",
    )
    .bind(&et_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (host_user_id, host_name) =
        host_info.unwrap_or_else(|| ("".to_string(), "Host".to_string()));

    // Sync calendars if stale before computing availability
    crate::commands::sync::sync_if_stale(&state.pool, &state.secret_key, &host_user_id).await;

    let guest_tz = parse_guest_tz(query.tz.as_deref());
    let host_tz = get_host_tz(&state.pool, &et_id).await;
    let guest_tz_name = guest_tz.name().to_string();

    let week = query.week.unwrap_or(0).max(0);
    let days_per_page = 7;
    let start_offset = week * days_per_page;
    let now_host = Utc::now().with_timezone(&host_tz).naive_local();
    let end_date = now_host.date() + Duration::days((start_offset + days_per_page) as i64);
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
        days_per_page,
        host_tz,
        guest_tz,
        busy,
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

    let (
        et_id,
        _et_slug,
        et_title,
        duration,
        buffer_before,
        buffer_after,
        min_notice,
        requires_confirmation,
    ) = match et {
        Some(e) => e,
        None => return Html("Event type not found.".to_string()).into_response(),
    };
    let needs_approval = requires_confirmation != 0;

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

    let host_tz = get_host_tz(&state.pool, &et_id).await;
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
        return Html("This slot is no longer available.".to_string()).into_response();
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

    sqlx::query(
        "INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, notes, start_at, end_at, status, cancel_token, reschedule_token, confirm_token)
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
    .bind(&confirm_token)
    .execute(&state.pool)
    .await
    .unwrap();

    // Send emails if SMTP is configured
    if let Ok(Some(smtp_config)) =
        crate::email::load_smtp_config(&state.pool, &state.secret_key).await
    {
        let host: Option<(String, String)> = sqlx::query_as(
            "SELECT u.name, COALESCE(u.booking_email, u.email) FROM users u JOIN accounts a ON a.user_id = u.id WHERE a.id = (SELECT account_id FROM event_types WHERE id = ?)",
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
                location: None,
            };

            let base_url = std::env::var("CALRS_BASE_URL").ok();
            let guest_cancel_url = base_url.as_ref().map(|base| {
                format!(
                    "{}/booking/cancel/{}",
                    base.trim_end_matches('/'),
                    cancel_token
                )
            });

            if needs_approval {
                let _ = crate::email::send_host_approval_request(
                    &smtp_config,
                    &details,
                    &id,
                    confirm_token.as_deref(),
                    base_url.as_deref(),
                )
                .await;
                let _ = crate::email::send_guest_pending_notice(
                    &smtp_config,
                    &details,
                    guest_cancel_url.as_deref(),
                )
                .await;
            } else {
                let _ = crate::email::send_guest_confirmation(
                    &smtp_config,
                    &details,
                    guest_cancel_url.as_deref(),
                )
                .await;
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
                    caldav_push_booking(&state.pool, &state.secret_key, &uid_user, &uid, &details)
                        .await;
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
    let user = &auth_user.user;

    // Always sync before troubleshooting to ensure fresh data
    crate::commands::sync::sync_if_stale(&state.pool, &state.secret_key, &user.id).await;

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
        return Html(
            tmpl.render(context! {
                user_name => &user.name,
                no_event_types => true,
            })
            .unwrap_or_default(),
        );
    }

    let selected_slug = params.event_type.as_deref().unwrap_or(&event_types[0].0);
    let selected_et = event_types
        .iter()
        .find(|et| et.0 == selected_slug)
        .unwrap_or(&event_types[0]);
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
    let day_end_compact = (target_date + Duration::days(1))
        .format("%Y%m%d")
        .to_string();
    let day_start_iso = target_date.format("%Y-%m-%dT00:00:00").to_string();
    let day_end_iso = target_date.format("%Y-%m-%dT23:59:59").to_string();

    // Non-recurring busy events for this date
    let raw_busy_events: Vec<(
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT e.start_at, e.end_at, e.summary, c.display_name, e.timezone
         FROM events e
         JOIN calendars c ON c.id = e.calendar_id
         JOIN caldav_sources cs ON cs.id = c.source_id
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND c.is_busy = 1
           AND (NOT EXISTS (SELECT 1 FROM event_type_calendars WHERE event_type_id = ?)
                OR c.id IN (SELECT calendar_id FROM event_type_calendars WHERE event_type_id = ?))
           AND (e.rrule IS NULL OR e.rrule = '')
           AND (e.status IS NULL OR e.status != 'CANCELLED')
           AND ((e.start_at < ? AND e.end_at > ?) OR (e.start_at < ? AND e.end_at > ?))
         ORDER BY e.start_at",
    )
    .bind(&user.id)
    .bind(&et_id)
    .bind(&et_id)
    .bind(&day_end_compact)
    .bind(&day_start_compact)
    .bind(&day_end_iso)
    .bind(&day_start_iso)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let mut busy_events: Vec<(String, String, Option<String>, Option<String>)> = raw_busy_events
        .iter()
        .filter_map(|(s, e, summary, cal_name, event_tz)| {
            let start = convert_event_to_tz(parse_datetime(s)?, event_tz.as_deref(), host_tz);
            let end = convert_event_to_tz(parse_datetime(e)?, event_tz.as_deref(), host_tz);
            Some((
                start.format("%Y-%m-%dT%H:%M:%S").to_string(),
                end.format("%Y-%m-%dT%H:%M:%S").to_string(),
                summary.clone(),
                cal_name.clone(),
            ))
        })
        .collect();

    // Recurring busy events — expand into this date
    let recurring_events: Vec<(
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT e.start_at, e.end_at, e.rrule, e.raw_ical, e.summary, c.display_name, e.timezone
         FROM events e
         JOIN calendars c ON c.id = e.calendar_id
         JOIN caldav_sources cs ON cs.id = c.source_id
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ? AND c.is_busy = 1
           AND (NOT EXISTS (SELECT 1 FROM event_type_calendars WHERE event_type_id = ?)
                OR c.id IN (SELECT calendar_id FROM event_type_calendars WHERE event_type_id = ?))
           AND (e.status IS NULL OR e.status != 'CANCELLED')
           AND e.rrule IS NOT NULL AND e.rrule != ''
           AND (e.start_at <= ? OR e.start_at <= ?)",
    )
    .bind(&user.id)
    .bind(&et_id)
    .bind(&et_id)
    .bind(&day_end_iso)
    .bind(&day_end_compact)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let ts_window_start = target_date.and_hms_opt(0, 0, 0).unwrap();
    let ts_window_end = target_date.and_hms_opt(23, 59, 59).unwrap();
    for (s, e, rrule_str, raw_ical, summary, cal_name, event_tz) in &recurring_events {
        if let (Some(ev_start), Some(ev_end)) = (parse_datetime(s), parse_datetime(e)) {
            let exdates = raw_ical
                .as_deref()
                .map(crate::rrule::extract_exdates)
                .unwrap_or_default();
            let occurrences = crate::rrule::expand_rrule(
                ev_start,
                ev_end,
                rrule_str,
                &exdates,
                ts_window_start,
                ts_window_end,
            );
            for (os, oe) in occurrences {
                let cs = convert_event_to_tz(os, event_tz.as_deref(), host_tz);
                let ce = convert_event_to_tz(oe, event_tz.as_deref(), host_tz);
                busy_events.push((
                    cs.format("%Y-%m-%dT%H:%M:%S").to_string(),
                    ce.format("%Y-%m-%dT%H:%M:%S").to_string(),
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
    .bind(&day_end_iso)
    .bind(&day_start_iso)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    // Build timeline: scan 15-min ticks from display_start to display_end
    let display_start_hour: u32 = rules
        .iter()
        .filter_map(|(s, _)| NaiveTime::parse_from_str(s, "%H:%M").ok())
        .map(|t| t.hour())
        .min()
        .unwrap_or(8)
        .saturating_sub(1);
    let display_end_hour: u32 = rules
        .iter()
        .filter_map(|(_, e)| NaiveTime::parse_from_str(e, "%H:%M").ok())
        .map(|t| {
            if t.minute() > 0 {
                t.hour() + 1
            } else {
                t.hour()
            }
        })
        .max()
        .unwrap_or(18)
        .min(23)
        + 1;

    let display_start = NaiveTime::from_hms_opt(display_start_hour, 0, 0)
        .unwrap_or(NaiveTime::from_hms_opt(7, 0, 0).unwrap());
    let display_end = NaiveTime::from_hms_opt(display_end_hour, 0, 0)
        .unwrap_or(NaiveTime::from_hms_opt(19, 0, 0).unwrap());
    let total_minutes = (display_end - display_start).num_minutes() as f64;

    let min_start = now_host + Duration::minutes(min_notice as i64);

    // Parse availability windows
    let avail_windows: Vec<(NaiveTime, NaiveTime)> = rules
        .iter()
        .filter_map(|(s, e)| {
            let st = NaiveTime::parse_from_str(s, "%H:%M").ok()?;
            let en = NaiveTime::parse_from_str(e, "%H:%M").ok()?;
            Some((st, en))
        })
        .collect();

    // Parse busy events into (start_dt, end_dt, label, detail)
    let busy_parsed: Vec<(NaiveDateTime, NaiveDateTime, String, String)> = busy_events
        .iter()
        .filter_map(|(s, e, summary, cal)| {
            let start = parse_datetime(s)?;
            let end = parse_datetime(e)?;
            let label = summary.clone().unwrap_or_else(|| "Busy".to_string());
            let detail = cal.clone().unwrap_or_default();
            Some((start, end, label, detail))
        })
        .collect();

    // Parse bookings into (start_dt, end_dt, label, detail)
    let bookings_parsed: Vec<(NaiveDateTime, NaiveDateTime, String, String)> = bookings
        .iter()
        .filter_map(|(s, e, guest, et_title)| {
            let start = parse_datetime(s)?;
            let end = parse_datetime(e)?;
            Some((start, end, guest.clone(), et_title.clone()))
        })
        .collect();

    // Scan in 15-min increments and classify each tick
    struct Tick {
        time: NaiveTime,
        status: String, // "available", "outside", "busy_event", "booking", "buffer", "min_notice"
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
        let in_avail = avail_windows
            .iter()
            .any(|(ws, we)| cursor >= *ws && cursor < *we);

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
                    label: format!(
                        "Buffer ({}min)",
                        if tick_dt < *ev_s {
                            buf_before
                        } else {
                            buf_after
                        }
                    ),
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
                    label: format!(
                        "Buffer ({}min)",
                        if tick_dt < *bk_s {
                            buf_before
                        } else {
                            buf_after
                        }
                    ),
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
    let blocks_ctx: Vec<minijinja::Value> = blocks
        .iter()
        .map(|b| {
            context! {
                start => b.start.format("%H:%M").to_string(),
                end => b.end.format("%H:%M").to_string(),
                status => &b.status,
                label => &b.label,
                detail => &b.detail,
                left_pct => format!("{:.2}", b.left_pct),
                width_pct => format!("{:.2}", b.width_pct),
            }
        })
        .collect();

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
    let breakdown_ctx: Vec<minijinja::Value> = blocks
        .iter()
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

    let et_options: Vec<minijinja::Value> = event_types
        .iter()
        .map(|et| {
            context! {
                slug => &et.0,
                title => &et.1,
                selected => et.0 == *et_slug,
            }
        })
        .collect();

    let prev_date = (target_date - Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let next_date = (target_date + Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let date_label = target_date.format("%A, %B %-d, %Y").to_string();

    let tmpl = match state.templates.get_template("troubleshoot.html") {
        Ok(t) => t,
        Err(e) => return Html(format!("Template error: {}", e)),
    };

    let (impersonating, impersonating_name, _impersonating_admin) = impersonation_ctx(&auth_user);
    Html(
        tmpl.render(context! {
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
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_default(),
    )
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
    let mut user_groups_map: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for (uid, gname) in &user_groups_rows {
        user_groups_map
            .entry(uid.clone())
            .or_default()
            .push(gname.clone());
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
    let registration_enabled = auth_config
        .as_ref()
        .map(|c| c.registration_enabled)
        .unwrap_or(false);
    let allowed_email_domains = auth_config
        .as_ref()
        .and_then(|c| c.allowed_email_domains.clone())
        .unwrap_or_default();
    let oidc_enabled = auth_config
        .as_ref()
        .map(|c| c.oidc_enabled)
        .unwrap_or(false);
    let oidc_issuer_url = auth_config
        .as_ref()
        .and_then(|c| c.oidc_issuer_url.clone())
        .unwrap_or_default();
    let oidc_client_id = auth_config
        .as_ref()
        .and_then(|c| c.oidc_client_id.clone())
        .unwrap_or_default();
    let oidc_auto_register = auth_config
        .as_ref()
        .map(|c| c.oidc_auto_register)
        .unwrap_or(true);

    // Fetch SMTP config (first one found)
    let smtp: Option<(String, i32, String, bool)> =
        sqlx::query_as("SELECT host, port, from_email, enabled FROM smtp_config LIMIT 1")
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
            has_logo => state.data_dir.join("logo.png").exists(),
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
        let _ =
            sqlx::query("UPDATE users SET enabled = ?, updated_at = datetime('now') WHERE id = ?")
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
    let secret_provided = form
        .oidc_client_secret
        .as_ref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);

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

// --- Logo management ---

async fn serve_logo(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let logo_path = state.data_dir.join("logo.png");
    match tokio::fs::read(&logo_path).await {
        Ok(bytes) => {
            let content_type = if logo_path.exists() {
                // Detect from magic bytes
                if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
                    "image/png"
                } else if bytes.starts_with(&[0xFF, 0xD8]) {
                    "image/jpeg"
                } else if bytes.starts_with(b"<svg") || bytes.starts_with(b"<?xml") {
                    "image/svg+xml"
                } else {
                    "image/png"
                }
            } else {
                "image/png"
            };
            axum::response::Response::builder()
                .status(200)
                .header("Content-Type", content_type)
                .header("Cache-Control", "public, max-age=3600")
                .body(axum::body::Body::from(bytes))
                .unwrap()
                .into_response()
        }
        Err(_) => axum::response::Response::builder()
            .status(404)
            .body(axum::body::Body::empty())
            .unwrap()
            .into_response(),
    }
}

async fn admin_upload_logo(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    mut multipart: Multipart,
) -> impl IntoResponse {
    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("logo") {
            let content_type = field.content_type().unwrap_or("").to_string();
            if !content_type.starts_with("image/") {
                return Redirect::to("/dashboard/admin");
            }
            if let Ok(bytes) = field.bytes().await {
                if bytes.len() > 2 * 1024 * 1024 {
                    return Redirect::to("/dashboard/admin");
                }
                let logo_path = state.data_dir.join("logo.png");
                let _ = tokio::fs::write(&logo_path, &bytes).await;
            }
        }
    }
    Redirect::to("/dashboard/admin")
}

async fn admin_delete_logo(
    State(state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
) -> impl IntoResponse {
    let logo_path = state.data_dir.join("logo.png");
    let _ = tokio::fs::remove_file(&logo_path).await;
    Redirect::to("/dashboard/admin")
}

// --- Impersonation ---

async fn admin_impersonate(
    State(_state): State<Arc<AppState>>,
    _admin: crate::auth::AdminUser,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    let cookie = format!(
        "calrs_impersonate={}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={}",
        user_id,
        86400 // 24 hours
    );
    ([("Set-Cookie", cookie)], Redirect::to("/dashboard")).into_response()
}

async fn admin_stop_impersonate(_admin: crate::auth::AdminUser) -> impl IntoResponse {
    let cookie = "calrs_impersonate=; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0";
    (
        [("Set-Cookie", cookie.to_string())],
        Redirect::to("/dashboard"),
    )
        .into_response()
}

// --- Token-based approve/decline (from email) ---

#[derive(Deserialize)]
struct DeclineForm {
    reason: Option<String>,
}

async fn approve_booking_by_token(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
) -> impl IntoResponse {
    // Look up booking by confirm_token
    let booking: Option<(String, String, String, String, String, String, String, String, String, Option<String>, Option<String>)> =
        sqlx::query_as(
            "SELECT b.id, b.uid, b.guest_name, b.guest_email, b.start_at, b.end_at, et.title, a.user_id, u.name, et.location_value, b.cancel_token
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
    ) = match booking {
        Some(b) => b,
        None => {
            // Check if already confirmed
            let already: Option<(String,)> =
                sqlx::query_as("SELECT status FROM bookings WHERE confirm_token = ?")
                    .bind(&token)
                    .fetch_optional(&state.pool)
                    .await
                    .unwrap_or(None);

            let (title, message) = match already {
                Some((status,)) if status == "confirmed" => (
                    "Already approved",
                    "This booking has already been approved.",
                ),
                Some((status,)) if status == "declined" => (
                    "Already declined",
                    "This booking has already been declined.",
                ),
                Some((status,)) if status == "cancelled" => {
                    ("Booking cancelled", "This booking was cancelled.")
                }
                _ => (
                    "Invalid link",
                    "This approval link is invalid or has expired.",
                ),
            };

            let tmpl = state
                .templates
                .get_template("booking_action_error.html")
                .unwrap();
            let rendered = tmpl
                .render(context! { title, message })
                .unwrap_or_else(|e| format!("Template error: {}", e));
            return Html(rendered).into_response();
        }
    };

    // Confirm the booking
    let _ = sqlx::query("UPDATE bookings SET status = 'confirmed' WHERE id = ?")
        .bind(&bid)
        .execute(&state.pool)
        .await;

    let date = if start_at.len() >= 10 {
        start_at[..10].to_string()
    } else {
        start_at.clone()
    };
    let start_time = if start_at.len() >= 16 {
        start_at[11..16].to_string()
    } else {
        "00:00".to_string()
    };
    let end_time = if end_at.len() >= 16 {
        end_at[11..16].to_string()
    } else {
        "00:00".to_string()
    };

    // Get host email for BookingDetails
    let host_email: String =
        sqlx::query_scalar("SELECT COALESCE(booking_email, email) FROM users WHERE id = ?")
            .bind(&user_id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or_default();

    let details = crate::email::BookingDetails {
        event_title: event_title.clone(),
        date: date.clone(),
        start_time: start_time.clone(),
        end_time: end_time.clone(),
        guest_name: guest_name.clone(),
        guest_email: guest_email.clone(),
        guest_timezone: "UTC".to_string(),
        host_name: host_name.clone(),
        host_email,
        uid: uid.clone(),
        notes: None,
        location: location_value,
    };

    // Push to CalDAV calendar
    caldav_push_booking(&state.pool, &state.secret_key, &user_id, &uid, &details).await;

    // Send confirmation email to guest
    if let Ok(Some(smtp_config)) =
        crate::email::load_smtp_config(&state.pool, &state.secret_key).await
    {
        let guest_cancel_url = cancel_token.as_ref().and_then(|t| {
            std::env::var("CALRS_BASE_URL")
                .ok()
                .map(|base| format!("{}/booking/cancel/{}", base.trim_end_matches('/'), t))
        });
        let _ = crate::email::send_guest_confirmation(
            &smtp_config,
            &details,
            guest_cancel_url.as_deref(),
        )
        .await;
    }

    let tmpl = state
        .templates
        .get_template("booking_approved.html")
        .unwrap();
    let rendered = tmpl
        .render(context! {
            event_title,
            date,
            start_time,
            end_time,
            guest_name,
            guest_email,
        })
        .unwrap_or_else(|e| format!("Template error: {}", e));

    Html(rendered).into_response()
}

async fn decline_booking_form(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
) -> impl IntoResponse {
    let booking: Option<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT b.guest_name, b.guest_email, b.start_at, b.end_at, et.title
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
            let tmpl = state
                .templates
                .get_template("booking_action_error.html")
                .unwrap();
            let rendered = tmpl.render(context! {
                title => "Invalid link",
                message => "This decline link is invalid, has expired, or the booking has already been processed.",
            }).unwrap_or_else(|e| format!("Template error: {}", e));
            return Html(rendered).into_response();
        }
    };

    let date = if start_at.len() >= 10 {
        start_at[..10].to_string()
    } else {
        start_at.clone()
    };
    let start_time = if start_at.len() >= 16 {
        start_at[11..16].to_string()
    } else {
        "00:00".to_string()
    };
    let end_time = if end_at.len() >= 16 {
        end_at[11..16].to_string()
    } else {
        "00:00".to_string()
    };

    let tmpl = state
        .templates
        .get_template("booking_decline_form.html")
        .unwrap();
    let rendered = tmpl
        .render(context! {
            event_title,
            date,
            start_time,
            end_time,
            guest_name,
            guest_email,
        })
        .unwrap_or_else(|e| format!("Template error: {}", e));

    Html(rendered).into_response()
}

async fn decline_booking_by_token(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
    Form(form): Form<DeclineForm>,
) -> impl IntoResponse {
    let booking: Option<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT b.id, b.guest_name, b.guest_email, b.start_at, b.end_at, et.title, u.name, COALESCE(u.booking_email, u.email)
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

    let (bid, guest_name, guest_email, start_at, end_at, event_title, host_name, host_email) =
        match booking {
            Some(b) => b,
            None => {
                let tmpl = state
                    .templates
                    .get_template("booking_action_error.html")
                    .unwrap();
                let rendered = tmpl.render(context! {
                    title => "Invalid link",
                    message => "This decline link is invalid, has expired, or the booking has already been processed.",
                }).unwrap_or_else(|e| format!("Template error: {}", e));
                return Html(rendered).into_response();
            }
        };

    // Decline the booking
    let _ = sqlx::query("UPDATE bookings SET status = 'declined' WHERE id = ?")
        .bind(&bid)
        .execute(&state.pool)
        .await;

    let date = if start_at.len() >= 10 {
        start_at[..10].to_string()
    } else {
        start_at.clone()
    };
    let start_time = if start_at.len() >= 16 {
        start_at[11..16].to_string()
    } else {
        "00:00".to_string()
    };
    let end_time = if end_at.len() >= 16 {
        end_at[11..16].to_string()
    } else {
        "00:00".to_string()
    };

    let reason = form.reason.filter(|r| !r.trim().is_empty());

    // Send decline notification to guest
    if let Ok(Some(smtp_config)) =
        crate::email::load_smtp_config(&state.pool, &state.secret_key).await
    {
        let details = crate::email::CancellationDetails {
            event_title: event_title.clone(),
            date: date.clone(),
            start_time: start_time.clone(),
            end_time: end_time.clone(),
            guest_name: guest_name.clone(),
            guest_email: guest_email.clone(),
            host_name: host_name.clone(),
            host_email,
            uid: String::new(),
            reason: reason.clone(),
            cancelled_by_host: true,
        };
        let _ = crate::email::send_guest_decline_notice(&smtp_config, &details).await;
    }

    let tmpl = state
        .templates
        .get_template("booking_declined.html")
        .unwrap();
    let rendered = tmpl
        .render(context! {
            event_title,
            date,
            start_time,
            end_time,
            guest_name,
            guest_email,
            reason,
        })
        .unwrap_or_else(|e| format!("Template error: {}", e));

    Html(rendered).into_response()
}

// --- Guest cancel booking by token ---

async fn guest_cancel_form(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
) -> impl IntoResponse {
    let booking: Option<(String, String, String, String, String, String)> = sqlx::query_as(
        "SELECT b.guest_name, b.guest_email, b.start_at, b.end_at, et.title, u.name
             FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             JOIN accounts a ON a.id = et.account_id
             JOIN users u ON u.id = a.user_id
             WHERE b.cancel_token = ? AND b.status IN ('confirmed', 'pending')",
    )
    .bind(&token)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (guest_name, _guest_email, start_at, end_at, event_title, host_name) = match booking {
        Some(b) => b,
        None => {
            let already: Option<(String,)> =
                sqlx::query_as("SELECT status FROM bookings WHERE cancel_token = ?")
                    .bind(&token)
                    .fetch_optional(&state.pool)
                    .await
                    .unwrap_or(None);

            let (title, message) = match already {
                Some((status,)) if status == "cancelled" => (
                    "Already cancelled",
                    "This booking has already been cancelled.",
                ),
                Some((status,)) if status == "declined" => (
                    "Booking declined",
                    "This booking has been declined by the host.",
                ),
                _ => (
                    "Invalid link",
                    "This cancellation link is invalid or has expired.",
                ),
            };

            let tmpl = state
                .templates
                .get_template("booking_action_error.html")
                .unwrap();
            let rendered = tmpl
                .render(context! { title, message })
                .unwrap_or_else(|e| format!("Template error: {}", e));
            return Html(rendered).into_response();
        }
    };

    let date = if start_at.len() >= 10 {
        start_at[..10].to_string()
    } else {
        start_at.clone()
    };
    let start_time = if start_at.len() >= 16 {
        start_at[11..16].to_string()
    } else {
        "00:00".to_string()
    };
    let end_time = if end_at.len() >= 16 {
        end_at[11..16].to_string()
    } else {
        "00:00".to_string()
    };

    let tmpl = state
        .templates
        .get_template("booking_cancel_form.html")
        .unwrap();
    let rendered = tmpl
        .render(context! {
            event_title,
            date,
            start_time,
            end_time,
            guest_name,
            host_name,
        })
        .unwrap_or_else(|e| format!("Template error: {}", e));

    Html(rendered).into_response()
}

async fn guest_cancel_booking(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
    Form(form): Form<CancelForm>,
) -> impl IntoResponse {
    let booking: Option<(String, String, String, String, String, String, String, String, String)> =
        sqlx::query_as(
            "SELECT b.id, b.uid, b.guest_name, b.guest_email, b.start_at, b.end_at, et.title, u.name, COALESCE(u.booking_email, u.email)
             FROM bookings b
             JOIN event_types et ON et.id = b.event_type_id
             JOIN accounts a ON a.id = et.account_id
             JOIN users u ON u.id = a.user_id
             WHERE b.cancel_token = ? AND b.status IN ('confirmed', 'pending')",
        )
        .bind(&token)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None);

    let (bid, uid, guest_name, guest_email, start_at, end_at, event_title, host_name, host_email) =
        match booking {
            Some(b) => b,
            None => {
                let tmpl = state
                    .templates
                    .get_template("booking_action_error.html")
                    .unwrap();
                let rendered = tmpl
                    .render(context! {
                        title => "Invalid link",
                        message => "This cancellation link is invalid, has expired, or the booking has already been cancelled.",
                    })
                    .unwrap_or_else(|e| format!("Template error: {}", e));
                return Html(rendered).into_response();
            }
        };

    // Cancel the booking
    let _ = sqlx::query("UPDATE bookings SET status = 'cancelled' WHERE id = ?")
        .bind(&bid)
        .execute(&state.pool)
        .await;

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
    }

    let date = if start_at.len() >= 10 {
        start_at[..10].to_string()
    } else {
        start_at.clone()
    };
    let start_time = if start_at.len() >= 16 {
        start_at[11..16].to_string()
    } else {
        "00:00".to_string()
    };
    let end_time = if end_at.len() >= 16 {
        end_at[11..16].to_string()
    } else {
        "00:00".to_string()
    };

    let reason = form.reason.filter(|r| !r.trim().is_empty());

    // Send cancellation emails
    if let Ok(Some(smtp_config)) =
        crate::email::load_smtp_config(&state.pool, &state.secret_key).await
    {
        let details = crate::email::CancellationDetails {
            event_title: event_title.clone(),
            date: date.clone(),
            start_time: start_time.clone(),
            end_time: end_time.clone(),
            guest_name: guest_name.clone(),
            guest_email: guest_email.clone(),
            host_name: host_name.clone(),
            host_email,
            uid,
            reason: reason.clone(),
            cancelled_by_host: false,
        };

        let _ = crate::email::send_guest_cancellation(&smtp_config, &details).await;
        let _ = crate::email::send_host_cancellation(&smtp_config, &details).await;
    }

    let tmpl = state
        .templates
        .get_template("booking_cancelled_guest.html")
        .unwrap();
    let rendered = tmpl
        .render(context! {
            event_title,
            date,
            start_time,
            end_time,
            host_name,
            reason,
        })
        .unwrap_or_else(|e| format!("Template error: {}", e));

    Html(rendered).into_response()
}

// --- CalDAV write-back ---

/// Push a confirmed booking to the host's CalDAV calendar.
/// Finds the first CalDAV source with a write_calendar_href set for this user,
/// generates the ICS, and PUTs it to the CalDAV server.
async fn caldav_push_booking(
    pool: &SqlitePool,
    key: &[u8; 32],
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

    let (url, username, password_enc, calendar_href) = match source {
        Some(s) => s,
        None => return, // No CalDAV write configured — silently skip
    };

    let password = match crate::crypto::decrypt_password(key, &password_enc) {
        Ok(p) => p,
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
async fn caldav_delete_booking(
    pool: &SqlitePool,
    key: &[u8; 32],
    user_id: &str,
    booking_uid: &str,
) {
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

    let (url, username, password_enc) = match source {
        Some(s) => s,
        None => return,
    };

    let password = match crate::crypto::decrypt_password(key, &password_enc) {
        Ok(p) => p,
        Err(_) => return,
    };

    let client = crate::caldav::CaldavClient::new(&url, &username, &password);
    if let Err(e) = client.delete_event(&calendar_href, booking_uid).await {
        eprintln!("CalDAV delete failed: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Rate limiter tests ---

    #[tokio::test]
    async fn rate_limiter_allows_under_limit() {
        let limiter = RateLimiter::new(3, 60);
        assert!(!limiter.check_limited("ip1").await);
        assert!(!limiter.check_limited("ip1").await);
        assert!(!limiter.check_limited("ip1").await);
    }

    #[tokio::test]
    async fn rate_limiter_blocks_over_limit() {
        let limiter = RateLimiter::new(2, 60);
        assert!(!limiter.check_limited("ip1").await); // 1
        assert!(!limiter.check_limited("ip1").await); // 2
        assert!(limiter.check_limited("ip1").await); // 3 → blocked
        assert!(limiter.check_limited("ip1").await); // still blocked
    }

    #[tokio::test]
    async fn rate_limiter_independent_per_ip() {
        let limiter = RateLimiter::new(1, 60);
        assert!(!limiter.check_limited("ip1").await);
        assert!(limiter.check_limited("ip1").await); // ip1 blocked
        assert!(!limiter.check_limited("ip2").await); // ip2 still ok
    }

    #[tokio::test]
    async fn rate_limiter_resets_after_window() {
        let limiter = RateLimiter::new(1, 0); // 0-second window = immediate expiry
        assert!(!limiter.check_limited("ip1").await);
        // Window has already expired (0 seconds)
        assert!(!limiter.check_limited("ip1").await); // reset, allowed again
    }

    // --- parse_datetime tests ---

    #[test]
    fn parse_datetime_compact_format() {
        let dt = parse_datetime("20260315T140000").unwrap();
        assert_eq!(
            dt,
            NaiveDate::from_ymd_opt(2026, 3, 15)
                .unwrap()
                .and_hms_opt(14, 0, 0)
                .unwrap()
        );
    }

    #[test]
    fn parse_datetime_iso_format() {
        let dt = parse_datetime("2026-03-15T14:00:00").unwrap();
        assert_eq!(
            dt,
            NaiveDate::from_ymd_opt(2026, 3, 15)
                .unwrap()
                .and_hms_opt(14, 0, 0)
                .unwrap()
        );
    }

    #[test]
    fn parse_datetime_allday_compact() {
        let dt = parse_datetime("20260315").unwrap();
        assert_eq!(
            dt,
            NaiveDate::from_ymd_opt(2026, 3, 15)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
        );
    }

    #[test]
    fn parse_datetime_allday_iso() {
        let dt = parse_datetime("2026-03-15").unwrap();
        assert_eq!(
            dt,
            NaiveDate::from_ymd_opt(2026, 3, 15)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
        );
    }

    #[test]
    fn parse_datetime_invalid() {
        assert!(parse_datetime("not-a-date").is_none());
        assert!(parse_datetime("").is_none());
    }

    // --- has_conflict tests ---

    fn dt(y: i32, m: u32, d: u32, h: u32, mi: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, mi, 0)
            .unwrap()
    }

    #[test]
    fn conflict_overlapping_event() {
        let busy = vec![(dt(2026, 3, 15, 10, 0), dt(2026, 3, 15, 11, 0))];
        // Slot 10:30-11:30 overlaps with 10:00-11:00
        assert!(has_conflict(
            &busy,
            dt(2026, 3, 15, 10, 30),
            dt(2026, 3, 15, 11, 30)
        ));
    }

    #[test]
    fn conflict_no_overlap() {
        let busy = vec![(dt(2026, 3, 15, 10, 0), dt(2026, 3, 15, 11, 0))];
        // Slot 11:00-12:00 starts exactly when event ends (no overlap)
        assert!(!has_conflict(
            &busy,
            dt(2026, 3, 15, 11, 0),
            dt(2026, 3, 15, 12, 0)
        ));
    }

    #[test]
    fn conflict_event_contains_slot() {
        let busy = vec![(dt(2026, 3, 15, 9, 0), dt(2026, 3, 15, 17, 0))];
        // Slot entirely within busy period
        assert!(has_conflict(
            &busy,
            dt(2026, 3, 15, 10, 0),
            dt(2026, 3, 15, 11, 0)
        ));
    }

    #[test]
    fn conflict_slot_contains_event() {
        let busy = vec![(dt(2026, 3, 15, 10, 15), dt(2026, 3, 15, 10, 45))];
        // Slot 10:00-11:00 contains the 10:15-10:45 event
        assert!(has_conflict(
            &busy,
            dt(2026, 3, 15, 10, 0),
            dt(2026, 3, 15, 11, 0)
        ));
    }

    #[test]
    fn conflict_adjacent_not_conflicting() {
        let busy = vec![
            (dt(2026, 3, 15, 9, 0), dt(2026, 3, 15, 10, 0)),
            (dt(2026, 3, 15, 11, 0), dt(2026, 3, 15, 12, 0)),
        ];
        // Slot 10:00-11:00 is between two events (no overlap)
        assert!(!has_conflict(
            &busy,
            dt(2026, 3, 15, 10, 0),
            dt(2026, 3, 15, 11, 0)
        ));
    }

    #[test]
    fn conflict_empty_busy_list() {
        let busy: Vec<(NaiveDateTime, NaiveDateTime)> = vec![];
        assert!(!has_conflict(
            &busy,
            dt(2026, 3, 15, 10, 0),
            dt(2026, 3, 15, 11, 0)
        ));
    }

    #[test]
    fn conflict_buffer_causes_overlap() {
        let busy = vec![(dt(2026, 3, 15, 10, 0), dt(2026, 3, 15, 11, 0))];
        // Slot is 11:00-12:00, but with 15min buffer before → buf_start=10:45 overlaps
        assert!(has_conflict(
            &busy,
            dt(2026, 3, 15, 10, 45),
            dt(2026, 3, 15, 12, 0)
        ));
    }

    // --- expand_recurring_into_busy tests ---

    #[test]
    fn expand_recurring_weekly_into_busy() {
        let recurring = vec![(
            "20260309T100000".to_string(), // Monday 10:00
            "20260309T110000".to_string(), // Monday 11:00
            "FREQ=WEEKLY;BYDAY=MO".to_string(),
            None,
            None,
        )];
        let window_start = dt(2026, 3, 9, 0, 0);
        let window_end = dt(2026, 3, 23, 23, 59);
        let busy = expand_recurring_into_busy(&recurring, window_start, window_end, Tz::UTC);
        // Should have 3 occurrences: Mar 9, 16, 23
        assert_eq!(busy.len(), 3);
        assert_eq!(busy[0].0, dt(2026, 3, 9, 10, 0));
        assert_eq!(busy[1].0, dt(2026, 3, 16, 10, 0));
        assert_eq!(busy[2].0, dt(2026, 3, 23, 10, 0));
    }

    #[test]
    fn expand_recurring_with_exdate() {
        let raw_ical = "BEGIN:VEVENT\nDTSTART:20260309T100000\nDTEND:20260309T110000\nRRULE:FREQ=WEEKLY;BYDAY=MO\nEXDATE:20260316T100000\nEND:VEVENT";
        let recurring = vec![(
            "20260309T100000".to_string(),
            "20260309T110000".to_string(),
            "FREQ=WEEKLY;BYDAY=MO".to_string(),
            Some(raw_ical.to_string()),
            None,
        )];
        let window_start = dt(2026, 3, 9, 0, 0);
        let window_end = dt(2026, 3, 23, 23, 59);
        let busy = expand_recurring_into_busy(&recurring, window_start, window_end, Tz::UTC);
        // Mar 16 excluded, so only Mar 9 and 23
        assert_eq!(busy.len(), 2);
        assert_eq!(busy[0].0, dt(2026, 3, 9, 10, 0));
        assert_eq!(busy[1].0, dt(2026, 3, 23, 10, 0));
    }

    // --- Integration tests with in-memory SQLite ---

    async fn setup_test_db() -> SqlitePool {
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
        use std::str::FromStr;
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::from_str("sqlite::memory:")
                    .unwrap()
                    .foreign_keys(true),
            )
            .await
            .unwrap();
        crate::db::migrate(&pool).await.unwrap();
        pool
    }

    /// Insert test fixtures: user, account, event type, availability rules
    async fn seed_test_data(pool: &SqlitePool) -> (String, String, String) {
        let user_id = uuid::Uuid::new_v4().to_string();
        let account_id = uuid::Uuid::new_v4().to_string();
        let et_id = uuid::Uuid::new_v4().to_string();

        sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'test@example.com', 'Test User', 'admin', 'local', 'testuser', 1)")
            .bind(&user_id)
            .execute(pool).await.unwrap();

        sqlx::query("INSERT INTO accounts (id, name, email, timezone, user_id) VALUES (?, 'Test User', 'test@example.com', 'UTC', ?)")
            .bind(&account_id)
            .bind(&user_id)
            .execute(pool).await.unwrap();

        sqlx::query("INSERT INTO event_types (id, account_id, slug, title, duration_min, buffer_before, buffer_after, min_notice_min, enabled) VALUES (?, ?, 'test-meeting', 'Test Meeting', 30, 0, 0, 0, 1)")
            .bind(&et_id)
            .bind(&account_id)
            .execute(pool).await.unwrap();

        // Mon-Fri 09:00-17:00
        for day in [1, 2, 3, 4, 5] {
            let rule_id = uuid::Uuid::new_v4().to_string();
            sqlx::query("INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time) VALUES (?, ?, ?, '09:00', '17:00')")
                .bind(&rule_id)
                .bind(&et_id)
                .bind(day)
                .execute(pool).await.unwrap();
        }

        (user_id, account_id, et_id)
    }

    #[tokio::test]
    async fn fetch_busy_times_empty_calendar() {
        let pool = setup_test_db().await;
        let (user_id, _, _) = seed_test_data(&pool).await;

        let busy = fetch_busy_times_for_user(
            &pool,
            &user_id,
            dt(2026, 3, 15, 0, 0),
            dt(2026, 3, 21, 23, 59),
            Tz::UTC,
            None,
        )
        .await;

        assert!(busy.is_empty(), "No events or bookings → no busy times");
    }

    #[tokio::test]
    async fn fetch_busy_times_includes_bookings() {
        let pool = setup_test_db().await;
        let (user_id, _, et_id) = seed_test_data(&pool).await;

        let booking_id = uuid::Uuid::new_v4().to_string();
        let cancel_tok = uuid::Uuid::new_v4().to_string();
        let resched_tok = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid1', 'Guest', 'guest@example.com', 'UTC', '2026-03-16T10:00:00', '2026-03-16T10:30:00', 'confirmed', ?, ?)")
            .bind(&booking_id)
            .bind(&et_id)
            .bind(&cancel_tok)
            .bind(&resched_tok)
            .execute(&pool).await.unwrap();

        let busy = fetch_busy_times_for_user(
            &pool,
            &user_id,
            dt(2026, 3, 15, 0, 0),
            dt(2026, 3, 21, 23, 59),
            Tz::UTC,
            None,
        )
        .await;

        assert_eq!(busy.len(), 1);
        assert_eq!(busy[0].0, dt(2026, 3, 16, 10, 0));
        assert_eq!(busy[0].1, dt(2026, 3, 16, 10, 30));
    }

    #[tokio::test]
    async fn fetch_busy_times_ignores_cancelled_bookings() {
        let pool = setup_test_db().await;
        let (user_id, _, et_id) = seed_test_data(&pool).await;

        let booking_id = uuid::Uuid::new_v4().to_string();
        let cancel_tok = uuid::Uuid::new_v4().to_string();
        let resched_tok = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?, ?, 'uid1', 'Guest', 'guest@example.com', 'UTC', '2026-03-16T10:00:00', '2026-03-16T10:30:00', 'cancelled', ?, ?)")
            .bind(&booking_id)
            .bind(&et_id)
            .bind(&cancel_tok)
            .bind(&resched_tok)
            .execute(&pool).await.unwrap();

        let busy = fetch_busy_times_for_user(
            &pool,
            &user_id,
            dt(2026, 3, 15, 0, 0),
            dt(2026, 3, 21, 23, 59),
            Tz::UTC,
            None,
        )
        .await;

        assert!(
            busy.is_empty(),
            "Cancelled bookings should not block availability"
        );
    }

    #[tokio::test]
    async fn compute_slots_basic_availability() {
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        let busy = BusySource::Individual(vec![]);

        // Start from tomorrow to avoid partial-day flakiness
        let slot_days = compute_slots(
            &pool,
            &et_id,
            30, // 30 min duration
            0,  // no buffer before
            0,  // no buffer after
            0,  // no min notice
            1,  // start from tomorrow
            14, // 14 days ahead
            Tz::UTC,
            Tz::UTC,
            busy,
        )
        .await;

        // Should have slots on weekdays only (Mon-Fri)
        assert!(!slot_days.is_empty(), "Should have slots on weekdays");

        // Each day should have 16 slots (09:00-17:00 in 30-min increments)
        for day in &slot_days {
            assert_eq!(
                day.slots.len(),
                16,
                "09:00-17:00 with 30min = 16 slots, got {} for {}",
                day.slots.len(),
                day.date
            );
        }
    }

    #[tokio::test]
    async fn compute_slots_with_busy_event() {
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        // Find the next Monday from now
        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date();
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let days_to_monday = (next_monday - now.date()).num_days() as i32;

        // Block 10:00-11:00 on that Monday
        let busy_start = next_monday.and_hms_opt(10, 0, 0).unwrap();
        let busy_end = next_monday.and_hms_opt(11, 0, 0).unwrap();
        let busy = BusySource::Individual(vec![(busy_start, busy_end)]);

        let slot_days = compute_slots(
            &pool,
            &et_id,
            30,
            0,
            0,
            0,
            days_to_monday,
            1, // just 1 day
            Tz::UTC,
            Tz::UTC,
            busy,
        )
        .await;

        assert!(!slot_days.is_empty(), "Should have the Monday");
        let monday = &slot_days[0];

        // 16 slots normally, minus 2 (10:00 and 10:30 blocked) = 14
        assert_eq!(monday.slots.len(), 14, "10:00 and 10:30 should be blocked");

        // Verify 10:00 and 10:30 are not in the slots
        let slot_times: Vec<&str> = monday.slots.iter().map(|s| s.start.as_str()).collect();
        assert!(!slot_times.contains(&"10:00"), "10:00 should be blocked");
        assert!(!slot_times.contains(&"10:30"), "10:30 should be blocked");
    }

    #[tokio::test]
    async fn compute_slots_with_buffer() {
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        // Find the next Monday
        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date();
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let days_to_monday = (next_monday - now.date()).num_days() as i32;

        // Block 10:00-10:30 on that Monday
        let busy_start = next_monday.and_hms_opt(10, 0, 0).unwrap();
        let busy_end = next_monday.and_hms_opt(10, 30, 0).unwrap();
        let busy = BusySource::Individual(vec![(busy_start, busy_end)]);

        let slot_days = compute_slots(
            &pool,
            &et_id,
            30,
            15, // 15 min buffer before
            15, // 15 min buffer after
            0,
            days_to_monday,
            1,
            Tz::UTC,
            Tz::UTC,
            busy,
        )
        .await;

        assert!(!slot_days.is_empty());
        let monday = &slot_days[0];

        // Event at 10:00-10:30 with 15min buffers blocks 09:45-10:45
        // So slots 09:30 (ends 10:00, buf_end=10:15 > 09:45), 10:00, 10:30 (buf_start=10:15 < 10:30) blocked
        let slot_times: Vec<&str> = monday.slots.iter().map(|s| s.start.as_str()).collect();
        assert!(
            !slot_times.contains(&"10:00"),
            "10:00 should be blocked (direct conflict)"
        );
        assert!(
            slot_times.contains(&"09:00"),
            "09:00 should be free (no buffer overlap)"
        );
    }

    #[tokio::test]
    async fn compute_slots_no_weekend_slots() {
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        // Find next Saturday
        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_sat = now.date();
        while next_sat.weekday() != chrono::Weekday::Sat {
            next_sat += Duration::days(1);
        }
        let days_to_sat = (next_sat - now.date()).num_days() as i32;

        let busy = BusySource::Individual(vec![]);
        let slot_days = compute_slots(
            &pool,
            &et_id,
            30,
            0,
            0,
            0,
            days_to_sat,
            2, // just Sat + Sun
            Tz::UTC,
            Tz::UTC,
            busy,
        )
        .await;

        assert!(
            slot_days.is_empty(),
            "Weekends should have no slots (Mon-Fri rules only)"
        );
    }

    #[tokio::test]
    async fn compute_slots_group_any_member_free() {
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        // Find the next Monday
        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date();
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let days_to_monday = (next_monday - now.date()).num_days() as i32;

        let ten_am = next_monday.and_hms_opt(10, 0, 0).unwrap();
        let ten_thirty = next_monday.and_hms_opt(10, 30, 0).unwrap();

        // Member A is busy at 10:00, Member B is free
        let mut member_busy = HashMap::new();
        member_busy.insert("member_a".to_string(), vec![(ten_am, ten_thirty)]);
        member_busy.insert("member_b".to_string(), vec![]); // free

        let busy = BusySource::Group(member_busy);
        let slot_days = compute_slots(
            &pool,
            &et_id,
            30,
            0,
            0,
            0,
            days_to_monday,
            1,
            Tz::UTC,
            Tz::UTC,
            busy,
        )
        .await;

        assert!(!slot_days.is_empty());
        let monday = &slot_days[0];
        // 10:00 should still be available because member B is free
        let slot_times: Vec<&str> = monday.slots.iter().map(|s| s.start.as_str()).collect();
        assert!(
            slot_times.contains(&"10:00"),
            "10:00 should be available (member B is free)"
        );
        assert_eq!(
            monday.slots.len(),
            16,
            "All slots available (at least one member free)"
        );
    }

    #[tokio::test]
    async fn compute_slots_group_all_busy_blocks() {
        let pool = setup_test_db().await;
        let (_, _, et_id) = seed_test_data(&pool).await;

        let now = Utc::now().with_timezone(&Tz::UTC).naive_local();
        let mut next_monday = now.date();
        while next_monday.weekday() != chrono::Weekday::Mon {
            next_monday += Duration::days(1);
        }
        let days_to_monday = (next_monday - now.date()).num_days() as i32;

        let ten_am = next_monday.and_hms_opt(10, 0, 0).unwrap();
        let ten_thirty = next_monday.and_hms_opt(10, 30, 0).unwrap();

        // Both members busy at 10:00
        let mut member_busy = HashMap::new();
        member_busy.insert("member_a".to_string(), vec![(ten_am, ten_thirty)]);
        member_busy.insert("member_b".to_string(), vec![(ten_am, ten_thirty)]);

        let busy = BusySource::Group(member_busy);
        let slot_days = compute_slots(
            &pool,
            &et_id,
            30,
            0,
            0,
            0,
            days_to_monday,
            1,
            Tz::UTC,
            Tz::UTC,
            busy,
        )
        .await;

        assert!(!slot_days.is_empty());
        let monday = &slot_days[0];
        let slot_times: Vec<&str> = monday.slots.iter().map(|s| s.start.as_str()).collect();
        assert!(
            !slot_times.contains(&"10:00"),
            "10:00 blocked when ALL members busy"
        );
        assert_eq!(monday.slots.len(), 15, "One slot blocked");
    }
}
