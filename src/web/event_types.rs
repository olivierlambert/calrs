//! Event type forms (personal and team), availability overrides, form parsing helpers.

use super::*;

/// Parse a dynamic group username string like "alice+bob+carol" into individual usernames.
/// Returns the deduplicated list, or an error if fewer than 2 valid usernames.
/// Parse a numeric form field, defaulting to `default` if empty or invalid.
fn parse_int_field(s: &str, default: i32) -> i32 {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        default
    } else {
        trimmed.parse().unwrap_or(default)
    }
}

fn parse_optional_positive_int(s: &str) -> Option<i32> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        trimmed.parse::<i32>().ok().filter(|v| *v > 0)
    }
}

/// Parse a day count where zero is meaningful (a booking horizon of 0 means
/// "today only"). Blank, unparseable, or negative input stores NULL, which
/// every horizon check treats as "no limit".
pub(super) fn parse_optional_day_count(s: &str) -> Option<i32> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        trimmed.parse::<i32>().ok().filter(|v| *v >= 0)
    }
}

/// Convert a (value, unit) pair from the event type form into a minute count
/// suitable for storage. Returns `None` when the value is blank, zero, or
/// invalid (no restriction). Unit defaults to minutes if missing or unknown.
fn parse_notice_to_minutes(value: &str, unit: &str) -> Option<i32> {
    let v = value.trim();
    if v.is_empty() {
        return None;
    }
    let n: i32 = match v.parse() {
        Ok(n) if n > 0 => n,
        _ => return None,
    };
    let multiplier = match unit.trim() {
        "days" => 1440,
        "hours" => 60,
        _ => 1, // minutes (default)
    };
    n.checked_mul(multiplier).filter(|m| *m > 0)
}

/// Pick the most natural unit for displaying a stored minute value back in
/// the form: divisible by 1440 → days, divisible by 60 → hours, else
/// minutes. Returns `(value, unit)`. Zero / negative / NULL all collapse to
/// `(0, "minutes")` so the form shows an empty restriction.
fn minutes_to_notice_form(min: Option<i32>) -> (i32, &'static str) {
    let m = min.unwrap_or(0);
    if m <= 0 {
        return (0, "minutes");
    }
    if m % 1440 == 0 {
        (m / 1440, "days")
    } else if m % 60 == 0 {
        (m / 60, "hours")
    } else {
        (m, "minutes")
    }
}

#[derive(Deserialize)]
pub(super) struct PriorityForm {
    _csrf: Option<String>,
    priority: String,
}

/// Single source of truth for "is this user allowed to manage this event type?"
/// Policy:
///   - Global admin: always yes.
///   - Personal event type (team_id IS NULL): account owner only.
///   - Team event type: team admin only (regular members cannot mutate).
///
/// Takes an event_type_id (not slug) to avoid slug-collision ambiguity.
pub(super) async fn can_manage_event_type(
    pool: &SqlitePool,
    user: &crate::models::User,
    event_type_id: &str,
) -> bool {
    if user.role == "admin" {
        return true;
    }
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM event_types et
         LEFT JOIN accounts a ON a.id = et.account_id
         LEFT JOIN team_members tm ON tm.team_id = et.team_id AND tm.user_id = ? AND tm.role = 'admin'
         WHERE et.id = ? AND (
             (et.team_id IS NULL AND a.user_id = ?) OR
             (et.team_id IS NOT NULL AND tm.user_id IS NOT NULL)
         )",
    )
    .bind(&user.id)
    .bind(event_type_id)
    .bind(&user.id)
    .fetch_one(pool)
    .await
    .unwrap_or(0)
        > 0
}

/// Look up an event type by slug that the (non-admin) user is allowed to
/// manage. Personal events resolve via the account owner; team events resolve
/// via team admin role. Global admins should bypass this and look up by slug
/// directly. Returns (id, title, team_id).
///
/// If the user has both a personal event and a team event with the same slug,
/// the personal event wins. Branch order in `UNION ALL` is not guaranteed by
/// SQL, so we wrap it in a subquery and order by `team_id IS NULL DESC`
/// (i.e. NULLs first → personal first) to make the resolution deterministic.
pub(super) async fn find_manageable_event_type_by_slug(
    pool: &SqlitePool,
    user_id: &str,
    slug: &str,
) -> Option<(String, String, Option<String>)> {
    sqlx::query_as(
        "SELECT id, title, team_id FROM (
             SELECT et.id, et.title, et.team_id FROM event_types et
             JOIN accounts a ON a.id = et.account_id
             WHERE a.user_id = ? AND et.slug = ? AND et.team_id IS NULL
             UNION ALL
             SELECT et.id, et.title, et.team_id FROM event_types et
             JOIN team_members tm ON tm.team_id = et.team_id
             WHERE tm.user_id = ? AND tm.role = 'admin' AND et.slug = ? AND et.team_id IS NOT NULL
         )
         ORDER BY (team_id IS NULL) DESC
         LIMIT 1",
    )
    .bind(user_id)
    .bind(slug)
    .bind(user_id)
    .bind(slug)
    .fetch_optional(pool)
    .await
    .unwrap_or(None)
}

// --- Event type CRUD ---

#[derive(Deserialize)]
pub(super) struct EventTypeForm {
    pub(super) _csrf: Option<String>,
    pub(super) title: String,
    pub(super) slug: String,
    pub(super) description: Option<String>,
    #[serde(default)]
    pub(super) duration_min: String,
    #[serde(default)]
    slot_interval_min: String,
    #[serde(default)]
    pub(super) buffer_before: String,
    #[serde(default)]
    pub(super) buffer_after: String,
    #[serde(default)]
    pub(super) min_notice_min: String,
    pub(super) requires_confirmation: Option<String>, // checkbox: "on" or absent
    pub(super) sms_phone_mode: Option<String>,        // checkbox: "on" or absent
    pub(super) visibility: Option<String>,            // "public", "internal", or "private"
    pub(super) location_type: Option<String>, // "link", "phone", "in_person", "custom", "jitsi_auto", "webhook_auto", "google_meet"
    pub(super) location_value: Option<String>,
    // Optional per-event-type pattern override for the jitsi_auto provider.
    // NULL/empty falls back to the org-wide default pattern from auth_config.
    meeting_pattern_override: Option<String>,
    // Availability schedule
    avail_days: Option<String>,  // comma-separated: "1,2,3,4,5"
    avail_start: Option<String>, // legacy: "09:00"
    avail_end: Option<String>,   // legacy: "17:00"
    pub(super) avail_windows: Option<String>, // "09:00-12:00,13:00-17:00"
    pub(super) avail_schedule: Option<String>, // "1:09:00-17:00;2:09:00-12:00,13:00-17:00"
    // Scheduling mode (round_robin / collective)
    pub(super) scheduling_mode: Option<String>,
    // Team (optional)
    pub(super) team_id: Option<String>,
    // Calendar selection (comma-separated IDs)
    calendar_ids: Option<String>,
    pub(super) resource_ids: Option<String>,
    resource_scheduling_mode: Option<String>,
    // Reminder
    #[serde(default)]
    pub(super) reminder_minutes: String,
    // Additional guests
    #[serde(default)]
    pub(super) max_additional_guests: String,
    // Member priorities for round-robin (creation flow): "uid1:3,uid2:1,uid3:2"
    #[serde(default)]
    member_priorities: String,
    // Default calendar view for guests (month / week / column)
    pub(super) default_calendar_view: Option<String>,
    // Booking frequency limits: "1:day,5:week,10:month"
    #[serde(default)]
    frequency_limits: String,
    // Show only the earliest available slot per day
    first_slot_only: Option<String>, // checkbox: "on" or absent
    // Watcher teams (comma-separated team IDs)
    pub(super) watcher_team_ids: Option<String>,
    // Timezone in which the availability rules are interpreted. IANA name
    // (e.g. "Europe/Paris"). Optional on submit — if blank, create falls back
    // to the submitting user's timezone.
    pub(super) timezone: Option<String>,
    // Minimum notice required for guest-initiated cancel. The form posts a
    // numeric value plus a unit (minutes / hours / days); the handler
    // converts to minutes and stores NULL when the value is empty.
    #[serde(default)]
    cancel_notice_value: String,
    #[serde(default)]
    cancel_notice_unit: String,
    // Minimum notice required for guest-initiated reschedule. Same shape.
    #[serde(default)]
    reschedule_notice_value: String,
    #[serde(default)]
    reschedule_notice_unit: String,
    // Rolling booking horizon in days: how far ahead a guest may book. Blank
    // stores NULL (no limit); 0 means today only.
    #[serde(default)]
    pub(super) booking_horizon_days: String,
    // Pending requests (#234), shown when requires_confirmation is on.
    // Checkbox: "on" or absent.
    pub(super) pending_blocks_slot: Option<String>,
    // "never" or a number of hours from PENDING_EXPIRY_CHOICES. Its presence
    // is also the marker that the form carried the section at all.
    pub(super) pending_expiry_hours: Option<String>,
}

/// Longest event type title accepted, the same limit as team names.
const MAX_EVENT_TYPE_TITLE: usize = 255;
/// Longest meeting accepted: a day. The form's own control stops at 8 hours.
const MAX_EVENT_TYPE_DURATION: i32 = 24 * 60;

/// Server-side checks on an event type form (personal or team, create or
/// edit). Returns the slug to store, or the Fluent key of the first problem.
///
/// The slug is trimmed, lowercased and has spaces turned into dashes, as
/// both forms always did; left blank, it is derived from the title. Any
/// other character outside `[a-z0-9-]` is refused rather than dropped, the
/// rule teams already follow and the one the form's own pattern shows.
/// Numbers left blank keep their defaults; a value that is there has to
/// parse and be in range, instead of silently becoming the default.
pub(super) fn validate_event_type_form(form: &EventTypeForm) -> Result<String, &'static str> {
    let title = form.title.trim();
    if title.is_empty() {
        return Err("form-error-event-type-title-required");
    }
    if title.chars().count() > MAX_EVENT_TYPE_TITLE {
        return Err("form-error-event-type-title-length");
    }

    let optional_number = |s: &str| -> Result<Option<i32>, ()> {
        let s = s.trim();
        if s.is_empty() {
            Ok(None)
        } else {
            s.parse().map(Some).map_err(|_| ())
        }
    };
    match optional_number(&form.duration_min) {
        Ok(None) => {}
        Ok(Some(d)) if (1..=MAX_EVENT_TYPE_DURATION).contains(&d) => {}
        _ => return Err("form-error-event-type-duration"),
    }
    for field in [
        &form.buffer_before,
        &form.buffer_after,
        &form.min_notice_min,
        &form.reminder_minutes,
        &form.max_additional_guests,
    ] {
        if !matches!(optional_number(field), Ok(None) | Ok(Some(0..))) {
            return Err("form-error-event-type-number");
        }
    }
    if !matches!(
        optional_number(&form.booking_horizon_days),
        Ok(None) | Ok(Some(0..))
    ) {
        return Err("form-error-event-type-horizon");
    }

    let slug = form.slug.trim().to_lowercase().replace(' ', "-");
    if slug.is_empty() {
        let mut derived: String = title
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        while derived.contains("--") {
            derived = derived.replace("--", "-");
        }
        let derived = derived.trim_matches('-').to_string();
        if derived.is_empty() {
            return Err("form-error-title-required");
        }
        return Ok(derived);
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        || !slug.chars().any(|c| c.is_ascii_alphanumeric())
    {
        return Err("form-error-slug-charset");
    }
    Ok(slug)
}

/// Template context for the "Required resources" section of the event type
/// form: (all resources, comma-joined selected ids, scheduling mode).
/// Resources the user may attach to an event type belonging to `team_id`.
/// Global admins: every resource. Team admins of `team_id`: that team's
/// allowlist (`resource_teams`). Anyone else, including personal event
/// types, gets none: attachment stays global-admin only there (a plain
/// user could otherwise read a resource's schedule through public slot
/// availability, or spam reservations into its calendar).
pub(super) async fn attachable_resources(
    pool: &SqlitePool,
    user_id: &str,
    is_admin: bool,
    team_id: Option<&str>,
) -> Vec<(String, String)> {
    if is_admin {
        return sqlx::query_as("SELECT id, name FROM resources ORDER BY name")
            .fetch_all(pool)
            .await
            .unwrap_or_default();
    }
    let Some(team_id) = team_id else {
        return Vec::new();
    };
    sqlx::query_as(
        "SELECT r.id, r.name FROM resources r
         JOIN resource_teams rt ON rt.resource_id = r.id
         JOIN team_members tm ON tm.team_id = rt.team_id
         WHERE rt.team_id = ? AND tm.user_id = ? AND tm.role = 'admin'
         ORDER BY r.name",
    )
    .bind(team_id)
    .bind(user_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

/// Union of the allowlists of every team the user administers, for create
/// forms where the team is picked in the form itself. The save path
/// re-validates against the team actually submitted.
pub(super) async fn attachable_resources_any_team(
    pool: &SqlitePool,
    user_id: &str,
) -> Vec<(String, String)> {
    sqlx::query_as(
        "SELECT DISTINCT r.id, r.name FROM resources r
         JOIN resource_teams rt ON rt.resource_id = r.id
         JOIN team_members tm ON tm.team_id = rt.team_id
         WHERE tm.user_id = ? AND tm.role = 'admin'
         ORDER BY r.name",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

async fn resources_form_ctx(
    pool: &SqlitePool,
    et_id: Option<&str>,
    available: &[(String, String)],
) -> (Vec<minijinja::Value>, String, String) {
    // An empty `available` hides the form section entirely, and the
    // matching save is skipped so existing attachments survive edits
    // untouched.
    let resources_all: Vec<minijinja::Value> = available
        .iter()
        .map(|(id, name)| context! { id => id, name => name })
        .collect();
    let (selected, mode) = match et_id {
        Some(et) => {
            let ids: Vec<(String,)> = sqlx::query_as(
                "SELECT resource_id FROM event_type_resources WHERE event_type_id = ?",
            )
            .bind(et)
            .fetch_all(pool)
            .await
            .unwrap_or_default();
            let mode: String =
                sqlx::query_scalar("SELECT resource_scheduling_mode FROM event_types WHERE id = ?")
                    .bind(et)
                    .fetch_optional(pool)
                    .await
                    .unwrap_or(None)
                    .unwrap_or_else(|| "all".to_string());
            (
                ids.iter()
                    .map(|(id,)| id.as_str())
                    .collect::<Vec<_>>()
                    .join(","),
                mode,
            )
        }
        None => (String::new(), "all".to_string()),
    };
    (resources_all, selected, mode)
}

/// Persist the event type's resource selection + scheduling mode
/// (delete-then-insert, mirroring the calendar junction). Unknown resource
/// ids are dropped by inserting through a SELECT on `resources`.
async fn save_event_type_resources(
    pool: &SqlitePool,
    et_id: &str,
    resource_ids: &Option<String>,
    mode: &Option<String>,
) {
    let _ = sqlx::query("DELETE FROM event_type_resources WHERE event_type_id = ?")
        .bind(et_id)
        .execute(pool)
        .await;
    if let Some(ids) = resource_ids {
        for rid in ids.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let _ = sqlx::query(
                "INSERT INTO event_type_resources (event_type_id, resource_id)
                 SELECT ?, id FROM resources WHERE id = ?",
            )
            .bind(et_id)
            .bind(rid)
            .execute(pool)
            .await;
        }
    }
    let mode = match mode.as_deref() {
        Some("round_robin") => "round_robin",
        _ => "all",
    };
    let _ = sqlx::query("UPDATE event_types SET resource_scheduling_mode = ? WHERE id = ?")
        .bind(mode)
        .bind(et_id)
        .execute(pool)
        .await;
}

/// Authorization wrapper around [`save_event_type_resources`]. Global
/// admins save as-is. Team admins are restricted to `team_id`'s allowlist:
/// submitted ids outside it are dropped, and existing attachments outside
/// it are preserved (the form never showed them, so a team admin's save
/// must not detach what a global admin attached). Users with no allowlist
/// skip the save entirely, scheduling mode included.
pub(super) async fn save_event_type_resources_authorized(
    pool: &SqlitePool,
    et_id: &str,
    user_id: &str,
    is_admin: bool,
    team_id: Option<&str>,
    resource_ids: &Option<String>,
    mode: &Option<String>,
) {
    if is_admin {
        save_event_type_resources(pool, et_id, resource_ids, mode).await;
        return;
    }
    let allowed = attachable_resources(pool, user_id, false, team_id).await;
    if allowed.is_empty() {
        return;
    }
    let allowed_ids: std::collections::HashSet<&str> =
        allowed.iter().map(|(id, _)| id.as_str()).collect();
    let existing: Vec<(String,)> =
        sqlx::query_as("SELECT resource_id FROM event_type_resources WHERE event_type_id = ?")
            .bind(et_id)
            .fetch_all(pool)
            .await
            .unwrap_or_default();
    let kept = existing
        .iter()
        .map(|(id,)| id.as_str())
        .filter(|id| !allowed_ids.contains(*id));
    let submitted = resource_ids
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty() && allowed_ids.contains(*s));
    let merged = kept.chain(submitted).collect::<Vec<_>>().join(",");
    save_event_type_resources(pool, et_id, &Some(merged), mode).await;
}

pub(super) async fn new_event_type_form(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let user = &auth_user.user;
    let preset = query.get("preset").map(|s| s.as_str()).unwrap_or("");

    // Get teams where the user is a team admin (global admins see all teams).
    // Only team admins can create team event types — regular members see no
    // teams in the dropdown.
    let groups: Vec<(String, String)> = if user.role == "admin" {
        sqlx::query_as("SELECT t.id, t.name FROM teams t ORDER BY t.name")
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default()
    } else {
        sqlx::query_as(
            "SELECT t.id, t.name FROM teams t JOIN team_members tm ON tm.team_id = t.id WHERE tm.user_id = ? AND tm.role = 'admin' ORDER BY t.name",
        )
        .bind(&user.id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default()
    };

    // Fetch members for each team (for client-side priority card)
    let mut groups_ctx: Vec<minijinja::Value> = Vec::new();
    for (id, name) in &groups {
        let team_members: Vec<(String, String, Option<String>, String)> = sqlx::query_as(
            "SELECT u.id, u.name, u.avatar_path, u.timezone FROM users u \
             JOIN team_members tm ON tm.user_id = u.id \
             WHERE tm.team_id = ? AND u.enabled = 1 ORDER BY u.name",
        )
        .bind(id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();
        let members_ctx: Vec<minijinja::Value> = team_members
            .iter()
            .map(|(uid, uname, ap, tz)| {
                context! {
                    user_id => uid,
                    name => uname,
                    has_avatar => ap.is_some(),
                    initials => compute_initials(uname),
                    timezone => tz,
                }
            })
            .collect();
        groups_ctx.push(context! { id => id, name => name, members => members_ctx });
    }

    // Every calendar can be selected; with none selected, only the busy ones
    // (`is_busy = 1`) are checked, which leaves out a colleague's shared one.
    let calendars: Vec<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT c.id, c.display_name, cs.name FROM calendars c
         JOIN caldav_sources cs ON cs.id = c.source_id
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ?
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

    // Pre-fill availability from user's default schedule
    ensure_user_avail_seeded(&state.pool, &user.id).await;
    let user_avail = load_user_avail_schedule(&state.pool, &user.id).await;

    let tmpl = match state.templates.get_template("event_type_form.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    let attachable = if user.role == "admin" {
        attachable_resources(&state.pool, &user.id, true, None).await
    } else {
        attachable_resources_any_team(&state.pool, &user.id).await
    };
    let (resources_all, selected_resource_ids, resource_scheduling_mode) =
        resources_form_ctx(&state.pool, None, &attachable).await;
    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);
    Html(
        tmpl.render(context! {
            sidebar => sidebar_context(&auth_user, "event-types"),
            lang => auth_user.lang,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
            editing => false,
            preset => preset,
            teams => groups_ctx,
            calendars => calendars_ctx,
            selected_calendar_ids => "",
            resources_all => resources_all,
            can_manage_resources => auth_user.user.role == "admin",
            can_enable_sms => can_enable_sms(&state.pool, &auth_user.user).await,
            selected_resource_ids => selected_resource_ids,
            resource_scheduling_mode => resource_scheduling_mode,
            form_title => "",
            form_slug => "",
            form_description => "",
            form_duration => 30,
            form_slot_interval => 0,
            form_buffer_before => 0,
            form_buffer_after => 0,
            form_min_notice => 60,
            form_requires_confirmation => matches!(preset, "private"),
            form_visibility => match preset { "private" => "private", "internal" => "internal", _ => "public" },
            form_location_type => "link",
            form_location_value => "",
            form_meeting_pattern_override => "",
            form_avail_schedule => user_avail,
            form_reminder_minutes => 1440,
            form_max_additional_guests => 0,
            form_default_calendar_view => "month",
            form_first_slot_only => false,
            form_frequency_limits => "",
            form_timezone => &user.timezone,
            form_cancel_notice_value => 0,
            form_cancel_notice_unit => "minutes",
            form_reschedule_notice_value => 0,
            form_reschedule_notice_unit => "minutes",
            form_booking_horizon_days => "",
            form_pending => PendingSettings::DEFAULT.context(),
            tz_options => timezone_options(&user.timezone, auth_user.lang),
            error => "",
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

pub(super) async fn create_event_type(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Form(form): Form<EventTypeForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
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
        None => return Redirect::to("/dashboard/event-types").into_response(),
    };

    let slug = match validate_event_type_form(&form) {
        Ok(slug) => slug,
        Err(key) => {
            return render_event_type_form_error(
                &state,
                &auth_user,
                &crate::i18n::translate(auth_user.lang, key, None),
                &form,
                false,
            )
            .await
            .into_response()
        }
    };

    // Check if a team_id was provided and it's non-empty
    let team_id = form.team_id.as_deref().filter(|s| !s.trim().is_empty());

    // Check uniqueness — scope to team_id when creating a team event type, otherwise to account_id
    let existing: Option<(String,)> = if let Some(tid) = team_id {
        sqlx::query_as("SELECT id FROM event_types WHERE team_id = ? AND slug = ?")
            .bind(tid)
            .bind(&slug)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None)
    } else {
        sqlx::query_as(
            "SELECT id FROM event_types WHERE account_id = ? AND slug = ? AND team_id IS NULL",
        )
        .bind(&account_id)
        .bind(&slug)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
    };

    if existing.is_some() {
        return render_event_type_form_error(
            &state,
            &auth_user,
            &crate::i18n::translate(auth_user.lang, "form-error-event-type-slug-taken", None),
            &form,
            false,
        )
        .await
        .into_response();
    }

    let et_id = uuid::Uuid::new_v4().to_string();
    let requires_confirmation = form.requires_confirmation.as_deref() == Some("on");
    let sms_phone_mode =
        resolve_phone_mode(&state.pool, user, form.sms_phone_mode.as_deref(), None).await;

    let location_type = form.location_type.as_deref().unwrap_or("link");
    let location_value = form
        .location_value
        .as_deref()
        .filter(|s| !s.trim().is_empty());

    let location_required = !meeting::is_auto_location(location_type);
    if location_required && location_value.is_none() {
        return render_event_type_form_error(
            &state,
            &auth_user,
            &crate::i18n::translate(auth_user.lang, "form-error-location-required", None),
            &form,
            false,
        )
        .await
        .into_response();
    }

    // Only team admins (or global admins) can create team event types.
    // Run this before the Google Meet prerequisite check so a non-admin
    // cannot probe whether a team has Google write-back configured.
    if let Some(tid) = team_id {
        let is_global_admin = user.role == "admin";
        if !is_global_admin && !is_team_admin(&state.pool, &user.id, tid).await {
            return render_event_type_form_error(
                &state,
                &auth_user,
                &crate::i18n::translate(auth_user.lang, "form-error-not-team-admin", None),
                &form,
                false,
            )
            .await
            .into_response();
        }
    }

    if let Some(err) =
        google_meet_location_error(&state.pool, location_type, Some(&user.id), team_id, None).await
    {
        return render_event_type_form_error(&state, &auth_user, &err, &form, false)
            .await
            .into_response();
    }

    let visibility = match form.visibility.as_deref().unwrap_or("public") {
        v @ ("public" | "internal" | "private") => v.to_string(),
        _ => "public".to_string(),
    };

    let reminder_minutes = {
        let v = parse_int_field(&form.reminder_minutes, 0);
        if v > 0 {
            Some(v)
        } else {
            None
        }
    };

    let default_calendar_view = match form.default_calendar_view.as_deref().unwrap_or("month") {
        v @ ("month" | "week" | "column") => v.to_string(),
        _ => "month".to_string(),
    };

    let first_slot_only = form.first_slot_only.as_deref() == Some("on");
    let timezone = normalize_event_type_tz(form.timezone.as_deref(), &user.timezone);
    let cancel_notice_min =
        parse_notice_to_minutes(&form.cancel_notice_value, &form.cancel_notice_unit);
    let reschedule_notice_min =
        parse_notice_to_minutes(&form.reschedule_notice_value, &form.reschedule_notice_unit);
    let booking_horizon_days = parse_optional_day_count(&form.booking_horizon_days);

    let meeting_pattern_override = form
        .meeting_pattern_override
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let _ = sqlx::query(
        "INSERT INTO event_types (id, account_id, slug, title, description, duration_min, slot_interval_min, buffer_before, buffer_after, min_notice_min, requires_confirmation, location_type, location_value, team_id, created_by_user_id, reminder_minutes, visibility, max_additional_guests, default_calendar_view, first_slot_only, timezone, cancel_notice_min, reschedule_notice_min, meeting_pattern_override, sms_phone_mode, booking_horizon_days)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&et_id)
    .bind(&account_id)
    .bind(&slug)
    .bind(form.title.trim())
    .bind(form.description.as_deref().filter(|s| !s.trim().is_empty()))
    .bind(parse_int_field(&form.duration_min, 30))
    .bind(parse_optional_positive_int(&form.slot_interval_min))
    .bind(parse_int_field(&form.buffer_before, 0))
    .bind(parse_int_field(&form.buffer_after, 0))
    .bind(parse_int_field(&form.min_notice_min, 60))
    .bind(requires_confirmation as i32)
    .bind(location_type)
    .bind(location_value)
    .bind(team_id)
    .bind(if team_id.is_some() { Some(&user.id) } else { None })
    .bind(reminder_minutes)
    .bind(&visibility)
    .bind(parse_int_field(&form.max_additional_guests, 0))
    .bind(&default_calendar_view)
    .bind(first_slot_only as i32)
    .bind(&timezone)
    .bind(cancel_notice_min)
    .bind(reschedule_notice_min)
    .bind(&meeting_pattern_override)
    .bind(&sms_phone_mode)
    .bind(booking_horizon_days)
    .execute(&state.pool)
    .await;
    save_pending_settings(&state.pool, &et_id, &form, true).await;

    tracing::info!(slug = %slug, user = %auth_user.user.email, "event type created");

    // Create availability rules. Pass the user's profile-default schedule as a
    // fallback so an empty submission falls back to it instead of hardcoded
    // Mon-Fri 09:00-17:00.
    ensure_user_avail_seeded(&state.pool, &auth_user.user.id).await;
    let user_default = load_user_avail_schedule(&state.pool, &auth_user.user.id).await;
    let schedule = parse_avail_schedule(
        form.avail_schedule.as_deref(),
        form.avail_days.as_deref(),
        form.avail_windows.as_deref(),
        form.avail_start.as_deref(),
        form.avail_end.as_deref(),
        Some(&user_default),
    );

    for (day, windows) in &schedule {
        for (ws, we) in windows {
            let rule_id = uuid::Uuid::new_v4().to_string();
            let _ = sqlx::query(
                "INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&rule_id)
            .bind(&et_id)
            .bind(day)
            .bind(ws)
            .bind(we)
            .execute(&state.pool)
            .await;
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

    // Save member priorities (creation flow: "uid1:3,uid2:1,uid3:2")
    // Only insert weights for users who are actually team members.
    if let Some(tid) = team_id {
        if !form.member_priorities.is_empty() {
            let valid_members: Vec<(String,)> =
                sqlx::query_as("SELECT user_id FROM team_members WHERE team_id = ?")
                    .bind(tid)
                    .fetch_all(&state.pool)
                    .await
                    .unwrap_or_default();
            let valid_set: std::collections::HashSet<&str> =
                valid_members.iter().map(|(id,)| id.as_str()).collect();

            for entry in form.member_priorities.split(',') {
                let parts: Vec<&str> = entry.split(':').collect();
                if parts.len() == 2 {
                    let uid = parts[0].trim();
                    let weight: i64 = parts[1].trim().parse().unwrap_or(1);
                    if !uid.is_empty() && valid_set.contains(uid) {
                        let _ = sqlx::query(
                            "INSERT OR REPLACE INTO event_type_member_weights (event_type_id, user_id, weight) VALUES (?, ?, ?)",
                        )
                        .bind(&et_id)
                        .bind(uid)
                        .bind(weight)
                        .execute(&state.pool)
                        .await;
                    }
                }
            }
        }
    }

    // Save booking frequency limits
    save_frequency_limits(&state.pool, &et_id, &form.frequency_limits).await;

    save_event_type_resources_authorized(
        &state.pool,
        &et_id,
        &auth_user.user.id,
        auth_user.user.role == "admin",
        team_id,
        &form.resource_ids,
        &form.resource_scheduling_mode,
    )
    .await;

    Redirect::to("/dashboard/event-types").into_response()
}

pub(super) async fn edit_event_type_form(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(slug): Path<String>,
) -> impl IntoResponse {
    let user = &auth_user.user;

    let et: Option<(String, String, String, Option<String>, i32, i32, i32, i32, i32, String, Option<String>, Option<i32>, String, i32, String, Option<String>)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.description, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min, et.requires_confirmation, et.location_type, et.location_value, et.reminder_minutes, et.visibility, et.max_additional_guests, et.scheduling_mode, et.team_id
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         WHERE a.user_id = ? AND et.slug = ? AND et.team_id IS NULL",
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
        visibility,
        max_additional_guests,
        scheduling_mode,
        team_id,
    ) = match et {
        Some(e) => e,
        None => {
            return Html(crate::i18n::translate(
                auth_user.lang,
                "error-event-type-not-found",
                None,
            ))
        }
    };

    let slot_interval: Option<i32> =
        sqlx::query_scalar("SELECT slot_interval_min FROM event_types WHERE id = ?")
            .bind(&et_id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(None);

    let default_calendar_view: String =
        sqlx::query_scalar("SELECT default_calendar_view FROM event_types WHERE id = ?")
            .bind(&et_id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or_else(|_| "month".to_string());

    let first_slot_only: i32 =
        sqlx::query_scalar("SELECT first_slot_only FROM event_types WHERE id = ?")
            .bind(&et_id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);

    // With no timezone of its own the event type follows its owner's, so
    // show that one: posted back unchanged, the save keeps the column NULL.
    let form_timezone: String = match stored_event_type_tz(&state.pool, &et_id).await {
        Some(tz) => tz,
        None => event_type_followed_tz(&state.pool, &et_id).await,
    };

    // Min-notice for guest-initiated cancel and reschedule (NULL = no
    // restriction). Pulled separately to keep the existing tuple compact.
    let cancel_notice_min: Option<i32> = sqlx::query_scalar::<_, Option<i32>>(
        "SELECT cancel_notice_min FROM event_types WHERE id = ?",
    )
    .bind(&et_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .flatten();
    let reschedule_notice_min: Option<i32> = sqlx::query_scalar::<_, Option<i32>>(
        "SELECT reschedule_notice_min FROM event_types WHERE id = ?",
    )
    .bind(&et_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .flatten();
    let (form_cancel_notice_value, form_cancel_notice_unit) =
        minutes_to_notice_form(cancel_notice_min);
    let (form_reschedule_notice_value, form_reschedule_notice_unit) =
        minutes_to_notice_form(reschedule_notice_min);
    // Blank renders as "no limit"; 0 is a real value meaning "today only".
    let form_booking_horizon_days = get_booking_horizon(&state.pool, &et_id)
        .await
        .map(|v| v.to_string())
        .unwrap_or_default();
    let form_pending = PendingSettings::load(&state.pool, &et_id).await.context();

    let meeting_pattern_override: Option<String> = sqlx::query_scalar::<_, Option<String>>(
        "SELECT meeting_pattern_override FROM event_types WHERE id = ?",
    )
    .bind(&et_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .flatten();

    // Get current availability rules
    let all_rules: Vec<(i32, String, String)> = sqlx::query_as(
        "SELECT day_of_week, start_time, end_time FROM availability_rules WHERE event_type_id = ? ORDER BY day_of_week, start_time",
    )
    .bind(&et_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let avail_days: String = {
        let mut days: Vec<i32> = all_rules.iter().map(|(d, _, _)| *d).collect();
        days.sort();
        days.dedup();
        days.iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join(",")
    };

    // Collect distinct time windows (preserving order)
    let mut windows: Vec<(String, String)> = Vec::new();
    for (_, s, e) in &all_rules {
        let pair = (s.clone(), e.clone());
        if !windows.contains(&pair) {
            windows.push(pair);
        }
    }
    let avail_windows: String = windows
        .iter()
        .map(|(s, e)| format!("{}-{}", s, e))
        .collect::<Vec<_>>()
        .join(",");
    // Legacy fields for backward compat
    let (avail_start, avail_end) = windows
        .first()
        .cloned()
        .unwrap_or_else(|| ("09:00".to_string(), "17:00".to_string()));

    // Build per-day schedule string
    let avail_schedule = build_avail_schedule(&all_rules);

    // Every calendar can be selected; with none selected, only the busy ones
    // (`is_busy = 1`) are checked, which leaves out a colleague's shared one.
    let calendars: Vec<(String, Option<String>, String)> = sqlx::query_as(
        "SELECT c.id, c.display_name, cs.name FROM calendars c
         JOIN caldav_sources cs ON cs.id = c.source_id
         JOIN accounts a ON a.id = cs.account_id
         WHERE a.user_id = ?
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

    // Fetch team members with per-ET weights (round-robin priority OR collective exclusion)
    let is_round_robin_group = team_id.is_some() && scheduling_mode == "round_robin";
    let is_collective_team = team_id.is_some() && scheduling_mode == "collective";
    let members_ctx: Vec<minijinja::Value> = if is_round_robin_group || is_collective_team {
        let tid = team_id.as_deref().unwrap();
        // Also selects timezone so admins can spot mis-configured user TZs at
        // a glance when setting up a team event (e.g. a US member whose TZ
        // is still the server default — which silently makes their personal
        // working hours land in the wrong local time).
        let members: Vec<(String, String, Option<String>, String)> = sqlx::query_as(
            "SELECT u.id, u.name, u.avatar_path, u.timezone \
             FROM users u JOIN team_members tm ON tm.user_id = u.id \
             WHERE tm.team_id = ? AND u.enabled = 1 \
             ORDER BY u.name",
        )
        .bind(tid)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();

        let et_weights: Vec<(String, i64)> = sqlx::query_as(
            "SELECT user_id, weight FROM event_type_member_weights WHERE event_type_id = ?",
        )
        .bind(&et_id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();
        let wmap: std::collections::HashMap<String, i64> = et_weights.into_iter().collect();

        members
            .iter()
            .map(|(uid, name, avatar_path, timezone)| {
                let w = wmap.get(uid).copied().unwrap_or(1);
                context! {
                    user_id => uid,
                    name => name,
                    has_avatar => avatar_path.is_some(),
                    initials => compute_initials(name),
                    weight => w,
                    timezone => timezone,
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    // Fetch booking frequency limits
    let freq_limits: Vec<(i32, String, i32)> = sqlx::query_as(
        "SELECT max_bookings, period, per_member FROM booking_frequency_limits WHERE event_type_id = ?",
    )
    .bind(&et_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let form_frequency_limits = freq_limits
        .iter()
        .map(|(c, p, m)| {
            if *m != 0 {
                format!("{}:{}:m", c, p)
            } else {
                format!("{}:{}", c, p)
            }
        })
        .collect::<Vec<_>>()
        .join(",");

    let tmpl = match state.templates.get_template("event_type_form.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);

    // Eligible users for dynamic group link picker (excluding self)
    let dg_eligible: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, username, name, avatar_path FROM users WHERE enabled = 1 AND allow_dynamic_group = 1 AND id != ? AND username IS NOT NULL ORDER BY name",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    let dg_eligible_ctx: Vec<minijinja::Value> = dg_eligible
        .iter()
        .map(|(id, username, name, avatar_path)| {
            context! {
                id => id,
                username => username,
                name => name,
                has_avatar => avatar_path.is_some(),
                initials => compute_initials(name),
            }
        })
        .collect();

    // Personal event types (this handler is scoped team_id IS NULL) stay
    // global-admin only for resource attachment.
    let attachable = attachable_resources(
        &state.pool,
        &auth_user.user.id,
        auth_user.user.role == "admin",
        None,
    )
    .await;
    let (resources_all, selected_resource_ids2, resource_scheduling_mode) =
        resources_form_ctx(&state.pool, Some(&et_id), &attachable).await;
    Html(
        tmpl.render(context! {
            editing => true,
            original_slug => et_slug,
            calendars => calendars_ctx,
            selected_calendar_ids => selected_calendar_ids,
            resources_all => resources_all,
            can_manage_resources => auth_user.user.role == "admin",
            can_enable_sms => can_enable_sms(&state.pool, &auth_user.user).await,
            selected_resource_ids => selected_resource_ids2,
            resource_scheduling_mode => resource_scheduling_mode,
            form_title => et_title,
            form_slug => et_slug,
            form_description => et_desc.unwrap_or_default(),
            form_duration => duration,
            form_slot_interval => slot_interval.unwrap_or(0),
            form_buffer_before => buf_before,
            form_buffer_after => buf_after,
            form_min_notice => min_notice,
            form_requires_confirmation => requires_conf != 0,
            form_sms_phone_mode => sqlx::query_scalar::<_, String>(
                "SELECT sms_phone_mode FROM event_types WHERE id = ?",
            )
            .bind(&et_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None)
            .unwrap_or_else(|| crate::sms::phone_mode::OFF.to_string()),
            form_visibility => visibility,
            form_location_type => loc_type,
            form_location_value => loc_value.unwrap_or_default(),
            form_meeting_pattern_override => meeting_pattern_override.clone().unwrap_or_default(),
            form_avail_days => avail_days,
            form_avail_start => avail_start,
            form_avail_end => avail_end,
            form_avail_windows => avail_windows,
            form_avail_schedule => avail_schedule,
            form_reminder_minutes => reminder_min.unwrap_or(0),
            form_max_additional_guests => max_additional_guests,
            form_scheduling_mode => scheduling_mode,
            form_default_calendar_view => default_calendar_view,
            form_first_slot_only => first_slot_only != 0,
            form_frequency_limits => form_frequency_limits,
            form_timezone => &form_timezone,
            form_cancel_notice_value => form_cancel_notice_value,
            form_cancel_notice_unit => form_cancel_notice_unit,
            form_reschedule_notice_value => form_reschedule_notice_value,
            form_reschedule_notice_unit => form_reschedule_notice_unit,
            form_booking_horizon_days => form_booking_horizon_days,
            form_pending => form_pending,
            tz_options => timezone_options(&form_timezone, auth_user.lang),
            is_group => team_id.is_some(),
            is_round_robin_group => is_round_robin_group,
            is_collective_team => is_collective_team,
            priority_members => members_ctx,
            owner_username => auth_user.user.username.as_deref().unwrap_or(""),
            dg_eligible_users => dg_eligible_ctx,
            error => "",
            sidebar => sidebar_context(&auth_user, "event-types"),
            lang => auth_user.lang,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

pub(super) async fn update_event_type(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Form(form): Form<EventTypeForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    let et: Option<(String, String)> = sqlx::query_as(
        "SELECT et.id, et.account_id
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         WHERE a.user_id = ? AND et.slug = ? AND et.team_id IS NULL",
    )
    .bind(&user.id)
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (et_id, account_id) = match et {
        Some(e) => e,
        None => return Redirect::to("/dashboard/event-types").into_response(),
    };

    let new_slug = match validate_event_type_form(&form) {
        Ok(slug) => slug,
        Err(key) => {
            return render_event_type_form_error(
                &state,
                &auth_user,
                &crate::i18n::translate(auth_user.lang, key, None),
                &form,
                true,
            )
            .await
            .into_response()
        }
    };
    let requires_confirmation = form.requires_confirmation.as_deref() == Some("on");
    let sms_phone_mode = resolve_phone_mode(
        &state.pool,
        user,
        form.sms_phone_mode.as_deref(),
        Some(&et_id),
    )
    .await;
    let visibility = match form.visibility.as_deref().unwrap_or("public") {
        v @ ("public" | "internal" | "private") => v.to_string(),
        _ => "public".to_string(),
    };

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
                &auth_user,
                &crate::i18n::translate(auth_user.lang, "form-error-event-type-slug-taken", None),
                &form,
                true,
            )
            .await
            .into_response();
        }
    }

    let location_type = form.location_type.as_deref().unwrap_or("link");
    let location_value = form
        .location_value
        .as_deref()
        .filter(|s| !s.trim().is_empty());

    let location_required = !meeting::is_auto_location(location_type);
    if location_required && location_value.is_none() {
        return render_event_type_form_error(
            &state,
            &auth_user,
            &crate::i18n::translate(auth_user.lang, "form-error-location-required", None),
            &form,
            true,
        )
        .await
        .into_response();
    }

    if let Some(err) = google_meet_location_error(
        &state.pool,
        location_type,
        Some(&user.id),
        None,
        Some(&et_id),
    )
    .await
    {
        return render_event_type_form_error(&state, &auth_user, &err, &form, true)
            .await
            .into_response();
    }

    let reminder_minutes = {
        let v = parse_int_field(&form.reminder_minutes, 0);
        if v > 0 {
            Some(v)
        } else {
            None
        }
    };

    let default_calendar_view = match form.default_calendar_view.as_deref().unwrap_or("month") {
        v @ ("month" | "week" | "column") => v.to_string(),
        _ => "month".to_string(),
    };

    let timezone = resolve_event_type_tz(
        form.timezone.as_deref(),
        stored_event_type_tz(&state.pool, &et_id).await.as_deref(),
        &event_type_followed_tz(&state.pool, &et_id).await,
    );
    let cancel_notice_min =
        parse_notice_to_minutes(&form.cancel_notice_value, &form.cancel_notice_unit);
    let reschedule_notice_min =
        parse_notice_to_minutes(&form.reschedule_notice_value, &form.reschedule_notice_unit);
    let booking_horizon_days = parse_optional_day_count(&form.booking_horizon_days);

    let meeting_pattern_override = form
        .meeting_pattern_override
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let _ = sqlx::query(
        "UPDATE event_types SET slug = ?, title = ?, description = ?, duration_min = ?, slot_interval_min = ?, buffer_before = ?, buffer_after = ?, min_notice_min = ?, requires_confirmation = ?, location_type = ?, location_value = ?, reminder_minutes = ?, visibility = ?, max_additional_guests = ?, scheduling_mode = ?, default_calendar_view = ?, first_slot_only = ?, timezone = ?, cancel_notice_min = ?, reschedule_notice_min = ?, meeting_pattern_override = ?, sms_phone_mode = ?, booking_horizon_days = ? WHERE id = ?",
    )
    .bind(&new_slug)
    .bind(form.title.trim())
    .bind(form.description.as_deref().filter(|s| !s.trim().is_empty()))
    .bind(parse_int_field(&form.duration_min, 30))
    .bind(parse_optional_positive_int(&form.slot_interval_min))
    .bind(parse_int_field(&form.buffer_before, 0))
    .bind(parse_int_field(&form.buffer_after, 0))
    .bind(parse_int_field(&form.min_notice_min, 60))
    .bind(requires_confirmation as i32)
    .bind(location_type)
    .bind(location_value)
    .bind(reminder_minutes)
    .bind(&visibility)
    .bind(parse_int_field(&form.max_additional_guests, 0))
    .bind(form.scheduling_mode.as_deref().unwrap_or("round_robin"))
    .bind(&default_calendar_view)
    .bind(if form.first_slot_only.as_deref() == Some("on") { 1 } else { 0 })
    .bind(&timezone)
    .bind(cancel_notice_min)
    .bind(reschedule_notice_min)
    .bind(&meeting_pattern_override)
    .bind(&sms_phone_mode)
    .bind(booking_horizon_days)
    .bind(&et_id)
    .execute(&state.pool)
    .await;
    save_pending_settings(&state.pool, &et_id, &form, false).await;

    // Update availability rules: delete old, insert new. Pass the user's
    // profile-default schedule as a fallback so an empty submission falls back
    // to it instead of hardcoded Mon-Fri 09:00-17:00.
    let _ = sqlx::query("DELETE FROM availability_rules WHERE event_type_id = ?")
        .bind(&et_id)
        .execute(&state.pool)
        .await;

    ensure_user_avail_seeded(&state.pool, &auth_user.user.id).await;
    let user_default = load_user_avail_schedule(&state.pool, &auth_user.user.id).await;
    let schedule = parse_avail_schedule(
        form.avail_schedule.as_deref(),
        form.avail_days.as_deref(),
        form.avail_windows.as_deref(),
        form.avail_start.as_deref(),
        form.avail_end.as_deref(),
        Some(&user_default),
    );

    for (day, windows) in &schedule {
        for (ws, we) in windows {
            let rule_id = uuid::Uuid::new_v4().to_string();
            let _ = sqlx::query(
                "INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&rule_id)
            .bind(&et_id)
            .bind(day)
            .bind(ws)
            .bind(we)
            .execute(&state.pool)
            .await;
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

    // Update booking frequency limits: delete old, insert new
    let _ = sqlx::query("DELETE FROM booking_frequency_limits WHERE event_type_id = ?")
        .bind(&et_id)
        .execute(&state.pool)
        .await;
    save_frequency_limits(&state.pool, &et_id, &form.frequency_limits).await;

    save_event_type_resources_authorized(
        &state.pool,
        &et_id,
        &auth_user.user.id,
        auth_user.user.role == "admin",
        None,
        &form.resource_ids,
        &form.resource_scheduling_mode,
    )
    .await;

    Redirect::to("/dashboard/event-types").into_response()
}

pub(super) async fn update_event_type_member_priority(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path((slug, target_user_id)): Path<(String, String)>,
    Form(form): Form<PriorityForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    // Resolve a personal event type owned by this user (team priority is
    // handled by the group-scoped route).
    let et: Option<(String,)> = sqlx::query_as(
        "SELECT et.id FROM event_types et \
         JOIN accounts a ON a.id = et.account_id \
         WHERE a.user_id = ? AND et.slug = ? AND et.team_id IS NULL",
    )
    .bind(&user.id)
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let et_id = match et {
        Some((id,)) => id,
        None => return Redirect::to("/dashboard/event-types").into_response(),
    };

    let weight: i64 = match form.priority.as_str() {
        "high" => 3,
        "medium" => 2,
        "exclude" => 0,
        _ => 1,
    };

    let _ = sqlx::query(
        "INSERT INTO event_type_member_weights (event_type_id, user_id, weight) \
         VALUES (?, ?, ?) \
         ON CONFLICT(event_type_id, user_id) DO UPDATE SET weight = excluded.weight",
    )
    .bind(&et_id)
    .bind(&target_user_id)
    .bind(weight)
    .execute(&state.pool)
    .await;

    tracing::info!(
        slug,
        target_user_id,
        priority = form.priority.as_str(),
        "updated member priority for event type"
    );

    Redirect::to(&format!("/dashboard/event-types/{}/edit", slug)).into_response()
}

pub(super) async fn update_group_event_type_member_priority(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path((team_id, slug, target_user_id)): Path<(String, String, String)>,
    Form(form): Form<PriorityForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;
    let is_admin = user.role == "admin";

    // Resolve event type within the addressed team. Two teams may share the
    // same slug, so the team_id from the route is required for the lookup
    // to be unambiguous.
    let et: Option<(String,)> = if is_admin {
        sqlx::query_as(
            "SELECT et.id FROM event_types et \
             WHERE et.team_id = ? AND et.slug = ?",
        )
        .bind(&team_id)
        .bind(&slug)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
    } else {
        sqlx::query_as(
            "SELECT et.id FROM event_types et \
             JOIN team_members tm ON tm.team_id = et.team_id \
             WHERE tm.user_id = ? AND tm.role = 'admin' AND et.team_id = ? AND et.slug = ?",
        )
        .bind(&user.id)
        .bind(&team_id)
        .bind(&slug)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
    };

    let et_id = match et {
        Some((id,)) => id,
        None => return Redirect::to("/dashboard/event-types").into_response(),
    };

    let weight: i64 = match form.priority.as_str() {
        "high" => 3,
        "medium" => 2,
        "exclude" => 0,
        _ => 1,
    };

    let _ = sqlx::query(
        "INSERT INTO event_type_member_weights (event_type_id, user_id, weight) \
         VALUES (?, ?, ?) \
         ON CONFLICT(event_type_id, user_id) DO UPDATE SET weight = excluded.weight",
    )
    .bind(&et_id)
    .bind(&target_user_id)
    .bind(weight)
    .execute(&state.pool)
    .await;

    tracing::info!(
        slug,
        target_user_id,
        priority = form.priority.as_str(),
        "updated member priority for group event type"
    );

    Redirect::to(&format!(
        "/dashboard/group-event-types/{}/{}/edit",
        team_id, slug
    ))
    .into_response()
}

pub(super) async fn toggle_event_type(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    let _ = sqlx::query(
        "UPDATE event_types SET enabled = CASE WHEN enabled = 1 THEN 0 ELSE 1 END
         WHERE slug = ? AND team_id IS NULL AND account_id IN (SELECT id FROM accounts WHERE user_id = ?)",
    )
    .bind(&slug)
    .bind(&user.id)
    .execute(&state.pool)
    .await;

    tracing::debug!(event_type_id = %slug, "event type toggled");

    Redirect::to("/dashboard/event-types").into_response()
}

pub(super) async fn delete_event_type(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    // Find the personal event type owned by this user (team event types are
    // managed via the team-scoped routes).
    let et: Option<(String,)> = sqlx::query_as(
        "SELECT et.id FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         WHERE et.slug = ? AND a.user_id = ? AND et.team_id IS NULL",
    )
    .bind(&slug)
    .bind(&user.id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let et_id = match et {
        Some((id,)) => id,
        None => return Redirect::to("/dashboard/event-types").into_response(),
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
        return Redirect::to("/dashboard/event-types").into_response();
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

    tracing::info!(event_type_id = %et_id, user = %auth_user.user.email, "event type deleted");

    Redirect::to("/dashboard/event-types").into_response()
}

async fn render_event_type_form_error(
    state: &AppState,
    auth_user: &crate::auth::AuthUser,
    error: &str,
    form: &EventTypeForm,
    editing: bool,
) -> Html<String> {
    let tmpl = match state.templates.get_template("event_type_form.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    // Echo the submitted resource selection back. Omitting resources_all
    // would hide the section entirely and a resubmit after a validation
    // error would then silently detach every resource.
    let attachable = if auth_user.user.role == "admin" {
        attachable_resources(&state.pool, &auth_user.user.id, true, None).await
    } else {
        attachable_resources_any_team(&state.pool, &auth_user.user.id).await
    };
    let resources_all: Vec<minijinja::Value> = attachable
        .iter()
        .map(|(id, name)| context! { id => id, name => name })
        .collect();

    let (impersonating, impersonating_name, _) = impersonation_ctx(auth_user);
    Html(
        tmpl.render(context! {
            resources_all => resources_all,
            can_manage_resources => auth_user.user.role == "admin",
            can_enable_sms => can_enable_sms(&state.pool, &auth_user.user).await,
            selected_resource_ids => form.resource_ids.clone().unwrap_or_default(),
            resource_scheduling_mode => form.resource_scheduling_mode.as_deref().unwrap_or("all"),
            editing => editing,
            form_title => form.title.as_str(),
            form_slug => form.slug.as_str(),
            form_description => form.description.as_deref().unwrap_or(""),
            form_duration => parse_int_field(&form.duration_min, 30),
            form_slot_interval => parse_optional_positive_int(&form.slot_interval_min).unwrap_or(0),
            form_buffer_before => parse_int_field(&form.buffer_before, 0),
            form_buffer_after => parse_int_field(&form.buffer_after, 0),
            form_min_notice => parse_int_field(&form.min_notice_min, 60),
            form_requires_confirmation => form.requires_confirmation.as_deref() == Some("on"),
            form_sms_phone_mode => crate::sms::phone_mode::parse(form.sms_phone_mode.as_deref().unwrap_or("")),
            form_visibility => form.visibility.as_deref().unwrap_or("public"),
            form_location_type => form.location_type.as_deref().unwrap_or("link"),
            form_location_value => form.location_value.as_deref().unwrap_or(""),
            form_meeting_pattern_override => form.meeting_pattern_override.as_deref().unwrap_or(""),
            form_avail_days => form.avail_days.as_deref().unwrap_or("1,2,3,4,5"),
            form_avail_start => form.avail_start.as_deref().unwrap_or("09:00"),
            form_avail_end => form.avail_end.as_deref().unwrap_or("17:00"),
            form_avail_windows => form.avail_windows.as_deref().unwrap_or(""),
            form_avail_schedule => form.avail_schedule.as_deref().unwrap_or(""),
            form_default_calendar_view => form.default_calendar_view.as_deref().unwrap_or("month"),
            form_first_slot_only => form.first_slot_only.as_deref() == Some("on"),
            form_frequency_limits => form.frequency_limits.as_str(),
            form_timezone => form.timezone.as_deref().unwrap_or(&auth_user.user.timezone),
            form_cancel_notice_value => parse_int_field(&form.cancel_notice_value, 0),
            form_cancel_notice_unit => match form.cancel_notice_unit.as_str() {
                "hours" => "hours",
                "days" => "days",
                _ => "minutes",
            },
            form_reschedule_notice_value => parse_int_field(&form.reschedule_notice_value, 0),
            form_reschedule_notice_unit => match form.reschedule_notice_unit.as_str() {
                "hours" => "hours",
                "days" => "days",
                _ => "minutes",
            },
            // Preserve what the user typed, so a validation error doesn't
            // silently blank the horizon back to "no limit".
            form_booking_horizon_days => form.booking_horizon_days.trim(),
            form_pending => PendingSettings::from_form(form)
                .unwrap_or(PendingSettings::DEFAULT)
                .context(),
            tz_options => timezone_options(form.timezone.as_deref().unwrap_or(&auth_user.user.timezone), auth_user.lang),
            error => error,
            sidebar => sidebar_context(auth_user, "event-types"),
            lang => auth_user.lang,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

// --- Availability overrides ---

pub(super) async fn overrides_page(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(slug): Path<String>,
) -> impl IntoResponse {
    let user = &auth_user.user;
    let is_global_admin = user.role == "admin";

    let et: Option<(String, String, Option<String>)> = if is_global_admin {
        sqlx::query_as(
            "SELECT et.id, et.title, et.team_id FROM event_types et WHERE et.slug = ? LIMIT 1",
        )
        .bind(&slug)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
    } else {
        find_manageable_event_type_by_slug(&state.pool, &user.id, &slug).await
    };

    let (et_id, et_title, team_id) = match et {
        Some(e) => e,
        None => return Redirect::to("/dashboard/event-types").into_response(),
    };
    let is_team = team_id.is_some();

    let overrides: Vec<(String, String, Option<String>, Option<String>, i32)> = sqlx::query_as(
        "SELECT id, date, start_time, end_time, is_blocked FROM availability_overrides WHERE event_type_id = ? ORDER BY date, start_time",
    )
    .bind(&et_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    // Dashboard handler: stays English until the dashboard surface is translated.
    let overrides_ctx: Vec<minijinja::Value> = overrides
        .iter()
        .map(|(id, date, start_time, end_time, is_blocked)| {
            let date_label = NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .map(|d| crate::i18n::format_long_date(d, "en"))
                .unwrap_or_else(|_| date.clone());
            context! {
                id => id,
                date => date,
                date_label => date_label,
                start_time => start_time,
                end_time => end_time,
                is_blocked => *is_blocked != 0,
            }
        })
        .collect();

    let tmpl = match state.templates.get_template("overrides.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("template render", &e),
    };

    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);

    Html(
        tmpl.render(context! {
            sidebar => sidebar_context(&auth_user, "event-types"),
            lang => auth_user.lang,
            event_type_title => et_title,
            event_type_slug => slug,
            overrides => overrides_ctx,
            is_team => is_team,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_default(),
    )
    .into_response()
}

#[derive(Deserialize)]
pub(super) struct OverrideForm {
    _csrf: Option<String>,
    date: String,
    override_type: String,
    start_time: Option<String>,
    end_time: Option<String>,
}

pub(super) async fn create_override(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Form(form): Form<OverrideForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;
    let is_global_admin = user.role == "admin";

    let et_id: Option<String> = if is_global_admin {
        sqlx::query_scalar("SELECT id FROM event_types WHERE slug = ? LIMIT 1")
            .bind(&slug)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None)
    } else {
        find_manageable_event_type_by_slug(&state.pool, &user.id, &slug)
            .await
            .map(|(id, _, _)| id)
    };

    let et_id = match et_id {
        Some(id) => id,
        None => return Redirect::to("/dashboard/event-types").into_response(),
    };

    // Validate date
    if NaiveDate::parse_from_str(&form.date, "%Y-%m-%d").is_err() {
        return Redirect::to(&format!("/dashboard/event-types/{}/overrides", slug)).into_response();
    }

    let is_blocked = form.override_type != "custom";
    let (start_time, end_time) = if !is_blocked {
        let s = form.start_time.as_deref().unwrap_or("");
        let e = form.end_time.as_deref().unwrap_or("");
        if s.is_empty() || e.is_empty() || s >= e {
            return Redirect::to(&format!("/dashboard/event-types/{}/overrides", slug))
                .into_response();
        }
        (Some(s.to_string()), Some(e.to_string()))
    } else {
        (None, None)
    };

    let id = uuid::Uuid::new_v4().to_string();
    let _ = sqlx::query(
        "INSERT INTO availability_overrides (id, event_type_id, date, start_time, end_time, is_blocked) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&et_id)
    .bind(&form.date)
    .bind(&start_time)
    .bind(&end_time)
    .bind(if is_blocked { 1 } else { 0 })
    .execute(&state.pool)
    .await;

    tracing::info!(
        override_id = %id,
        event_type = %slug,
        date = %form.date,
        is_blocked = is_blocked,
        "availability override created"
    );

    Redirect::to(&format!("/dashboard/event-types/{}/overrides", slug)).into_response()
}

#[derive(Deserialize)]
pub(super) struct DeleteOverrideForm {
    _csrf: Option<String>,
}

pub(super) async fn delete_override(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path((slug, override_id)): Path<(String, String)>,
    Form(form): Form<DeleteOverrideForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    // Look up the override's event type, then check management policy via
    // `can_manage_event_type` so the rule lives in one place.
    let target_et_id: Option<String> =
        sqlx::query_scalar("SELECT event_type_id FROM availability_overrides WHERE id = ?")
            .bind(&override_id)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    if let Some(et_id) = target_et_id {
        if can_manage_event_type(&state.pool, user, &et_id).await {
            let _ = sqlx::query("DELETE FROM availability_overrides WHERE id = ?")
                .bind(&override_id)
                .execute(&state.pool)
                .await;
            tracing::info!(override_id = %override_id, deleted_by = %user.email, "availability override deleted");
        }
    }

    Redirect::to(&format!("/dashboard/event-types/{}/overrides", slug)).into_response()
}

// --- Group event type handlers ---

pub(super) async fn new_group_event_type_form(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
) -> impl IntoResponse {
    let user = &auth_user.user;
    let is_admin = user.role == "admin";

    let groups: Vec<(String, String)> = if is_admin {
        sqlx::query_as("SELECT t.id, t.name FROM teams t ORDER BY t.name")
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default()
    } else {
        sqlx::query_as(
            "SELECT t.id, t.name FROM teams t JOIN team_members tm ON tm.team_id = t.id WHERE tm.user_id = ? AND tm.role = 'admin' ORDER BY t.name",
        )
        .bind(&user.id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default()
    };

    if groups.is_empty() {
        return Html(if is_admin {
            crate::i18n::translate(auth_user.lang, "team-event-type-no-teams", None)
        } else {
            crate::i18n::translate(auth_user.lang, "team-event-type-not-team-admin", None)
        });
    }

    let groups_ctx: Vec<minijinja::Value> = groups
        .iter()
        .map(|(id, name)| context! { id => id, name => name })
        .collect();

    // Pre-fill availability from user's default schedule
    ensure_user_avail_seeded(&state.pool, &user.id).await;
    let user_avail = load_user_avail_schedule(&state.pool, &user.id).await;

    // Load all teams for watcher selection (exclude the selected team itself in template)
    let watcher_teams_ctx: Vec<minijinja::Value> = groups
        .iter()
        .map(|(id, name)| context! { id => id, name => name })
        .collect();

    let tmpl = match state.templates.get_template("event_type_form.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);
    let attachable = if auth_user.user.role == "admin" {
        attachable_resources(&state.pool, &auth_user.user.id, true, None).await
    } else {
        attachable_resources_any_team(&state.pool, &auth_user.user.id).await
    };
    let (resources_all, selected_resource_ids, resource_scheduling_mode) =
        resources_form_ctx(&state.pool, None, &attachable).await;
    Html(
        tmpl.render(context! {
            editing => false,
            is_group => true,
            teams => groups_ctx,
            resources_all => resources_all,
            can_manage_resources => auth_user.user.role == "admin",
            can_enable_sms => can_enable_sms(&state.pool, &auth_user.user).await,
            selected_resource_ids => selected_resource_ids,
            resource_scheduling_mode => resource_scheduling_mode,
            form_team_id => groups.first().map(|(id, _)| id.as_str()).unwrap_or(""),
            form_title => "",
            form_slug => "",
            form_description => "",
            form_duration => 30,
            form_slot_interval => 0,
            form_buffer_before => 0,
            form_buffer_after => 0,
            form_min_notice => 60,
            form_requires_confirmation => false,
            form_location_type => "link",
            form_location_value => "",
            form_meeting_pattern_override => "",
            form_avail_schedule => user_avail,
            form_default_calendar_view => "month",
            form_first_slot_only => false,
            form_frequency_limits => "",
            form_booking_horizon_days => "",
            form_pending => PendingSettings::DEFAULT.context(),
            form_timezone => &user.timezone,
            tz_options => timezone_options(&user.timezone, auth_user.lang),
            watcher_teams => watcher_teams_ctx,
            selected_watcher_team_ids => "",
            error => "",
            sidebar => sidebar_context(&auth_user, "event-types"),
            lang => auth_user.lang,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

pub(super) async fn create_group_event_type(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Form(form): Form<EventTypeForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    let team_id = match form.team_id.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(tid) => tid.to_string(),
        None => return Redirect::to("/dashboard/event-types").into_response(),
    };

    let is_admin = user.role == "admin";

    // Only team admins (and global admins) can create team event types
    if !is_admin && !is_team_admin(&state.pool, &user.id, &team_id).await {
        return Html(crate::i18n::translate(
            auth_user.lang,
            "form-error-not-team-admin",
            None,
        ))
        .into_response();
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
        None => return Redirect::to("/dashboard/event-types").into_response(),
    };

    let slug = match validate_event_type_form(&form) {
        Ok(slug) => slug,
        Err(key) => {
            return render_event_type_form_error(
                &state,
                &auth_user,
                &crate::i18n::translate(auth_user.lang, key, None),
                &form,
                false,
            )
            .await
            .into_response()
        }
    };

    // Check uniqueness within the team
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT id FROM event_types WHERE team_id = ? AND slug = ?")
            .bind(&team_id)
            .bind(&slug)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);

    if existing.is_some() {
        return Html(crate::i18n::translate(
            auth_user.lang,
            "form-error-event-type-slug-taken-team",
            None,
        ))
        .into_response();
    }

    let et_id = uuid::Uuid::new_v4().to_string();
    let requires_confirmation = form.requires_confirmation.as_deref() == Some("on");
    let sms_phone_mode =
        resolve_phone_mode(&state.pool, user, form.sms_phone_mode.as_deref(), None).await;
    let location_type = form.location_type.as_deref().unwrap_or("link");
    let location_value = form
        .location_value
        .as_deref()
        .filter(|s| !s.trim().is_empty());

    // Auto-meeting providers compute their own URL per booking; the static
    // location_value field is not used and may be empty.
    let location_required = !meeting::is_auto_location(location_type);
    if location_required && location_value.is_none() {
        return render_event_type_form_error(
            &state,
            &auth_user,
            &crate::i18n::translate(auth_user.lang, "form-error-location-required", None),
            &form,
            false,
        )
        .await
        .into_response();
    }

    if let Some(err) =
        google_meet_location_error(&state.pool, location_type, None, Some(&team_id), None).await
    {
        return render_event_type_form_error(&state, &auth_user, &err, &form, false)
            .await
            .into_response();
    }

    let default_calendar_view = match form.default_calendar_view.as_deref().unwrap_or("month") {
        v @ ("month" | "week" | "column") => v.to_string(),
        _ => "month".to_string(),
    };

    let first_slot_only = form.first_slot_only.as_deref() == Some("on");
    let timezone = normalize_event_type_tz(form.timezone.as_deref(), &user.timezone);
    let cancel_notice_min =
        parse_notice_to_minutes(&form.cancel_notice_value, &form.cancel_notice_unit);
    let reschedule_notice_min =
        parse_notice_to_minutes(&form.reschedule_notice_value, &form.reschedule_notice_unit);
    let booking_horizon_days = parse_optional_day_count(&form.booking_horizon_days);

    let meeting_pattern_override = form
        .meeting_pattern_override
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let _ = sqlx::query(
        "INSERT INTO event_types (id, account_id, slug, title, description, duration_min, slot_interval_min, buffer_before, buffer_after, min_notice_min, requires_confirmation, location_type, location_value, team_id, created_by_user_id, default_calendar_view, first_slot_only, timezone, cancel_notice_min, reschedule_notice_min, meeting_pattern_override, sms_phone_mode, booking_horizon_days)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&et_id)
    .bind(&account_id)
    .bind(&slug)
    .bind(form.title.trim())
    .bind(form.description.as_deref().filter(|s| !s.trim().is_empty()))
    .bind(parse_int_field(&form.duration_min, 30))
    .bind(parse_optional_positive_int(&form.slot_interval_min))
    .bind(parse_int_field(&form.buffer_before, 0))
    .bind(parse_int_field(&form.buffer_after, 0))
    .bind(parse_int_field(&form.min_notice_min, 60))
    .bind(requires_confirmation as i32)
    .bind(location_type)
    .bind(location_value)
    .bind(&team_id)
    .bind(&user.id)
    .bind(&default_calendar_view)
    .bind(first_slot_only as i32)
    .bind(&timezone)
    .bind(cancel_notice_min)
    .bind(reschedule_notice_min)
    .bind(&meeting_pattern_override)
    .bind(&sms_phone_mode)
    .bind(booking_horizon_days)
    .execute(&state.pool)
    .await;
    save_pending_settings(&state.pool, &et_id, &form, true).await;

    // Create availability rules. Pass the creating user's profile-default
    // schedule as a fallback so an empty submission falls back to it instead
    // of hardcoded Mon-Fri 09:00-17:00.
    ensure_user_avail_seeded(&state.pool, &user.id).await;
    let user_default = load_user_avail_schedule(&state.pool, &user.id).await;
    let schedule = parse_avail_schedule(
        form.avail_schedule.as_deref(),
        form.avail_days.as_deref(),
        form.avail_windows.as_deref(),
        form.avail_start.as_deref(),
        form.avail_end.as_deref(),
        Some(&user_default),
    );

    for (day, windows) in &schedule {
        for (ws, we) in windows {
            let rule_id = uuid::Uuid::new_v4().to_string();
            let _ = sqlx::query(
                "INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&rule_id)
            .bind(&et_id)
            .bind(day)
            .bind(ws)
            .bind(we)
            .execute(&state.pool)
            .await;
        }
    }

    // Save watcher teams
    if let Some(ref watcher_ids) = form.watcher_team_ids {
        for wid in watcher_ids.split(',') {
            let wid = wid.trim();
            if !wid.is_empty() {
                let _ = sqlx::query(
                    "INSERT INTO event_type_watchers (event_type_id, team_id) VALUES (?, ?)",
                )
                .bind(&et_id)
                .bind(wid)
                .execute(&state.pool)
                .await;
            }
        }
    }

    // Save booking frequency limits
    save_frequency_limits(&state.pool, &et_id, &form.frequency_limits).await;

    save_event_type_resources_authorized(
        &state.pool,
        &et_id,
        &auth_user.user.id,
        auth_user.user.role == "admin",
        Some(&team_id),
        &form.resource_ids,
        &form.resource_scheduling_mode,
    )
    .await;

    Redirect::to("/dashboard/event-types").into_response()
}

pub(super) async fn edit_group_event_type_form(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path((team_id, slug)): Path<(String, String)>,
) -> impl IntoResponse {
    let user = &auth_user.user;

    let is_admin = user.role == "admin";

    let et: Option<(
        String,
        String,
        String,
        Option<String>,
        i32,
        i32,
        i32,
        i32,
        i32,
        String,
        Option<String>,
        Option<i32>,
        String,
        String,
        i32,
        String,
    )> = if is_admin {
        sqlx::query_as(
            "SELECT et.id, et.slug, et.title, et.description, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min, et.requires_confirmation, et.location_type, et.location_value, et.reminder_minutes, et.team_id, et.visibility, et.max_additional_guests, et.scheduling_mode
             FROM event_types et
             WHERE et.team_id = ? AND et.slug = ?",
        )
        .bind(&team_id)
        .bind(&slug)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
    } else {
        sqlx::query_as(
            "SELECT et.id, et.slug, et.title, et.description, et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min, et.requires_confirmation, et.location_type, et.location_value, et.reminder_minutes, et.team_id, et.visibility, et.max_additional_guests, et.scheduling_mode
             FROM event_types et
             JOIN team_members tm ON tm.team_id = et.team_id
             WHERE tm.user_id = ? AND tm.role = 'admin' AND et.team_id = ? AND et.slug = ?",
        )
        .bind(&user.id)
        .bind(&team_id)
        .bind(&slug)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
    };

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
        team_id,
        visibility,
        max_additional_guests,
        scheduling_mode,
    ) = match et {
        Some(e) => e,
        None => {
            return Html(crate::i18n::translate(
                auth_user.lang,
                "error-event-type-not-found",
                None,
            ))
        }
    };

    let slot_interval: Option<i32> =
        sqlx::query_scalar("SELECT slot_interval_min FROM event_types WHERE id = ?")
            .bind(&et_id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(None);

    let default_calendar_view: String =
        sqlx::query_scalar("SELECT default_calendar_view FROM event_types WHERE id = ?")
            .bind(&et_id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or_else(|_| "month".to_string());

    let first_slot_only: i32 =
        sqlx::query_scalar("SELECT first_slot_only FROM event_types WHERE id = ?")
            .bind(&et_id)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);

    // With no timezone of its own the event type follows its owner's, so
    // show that one: posted back unchanged, the save keeps the column NULL.
    let form_timezone: String = match stored_event_type_tz(&state.pool, &et_id).await {
        Some(tz) => tz,
        None => event_type_followed_tz(&state.pool, &et_id).await,
    };

    // Min-notice for guest-initiated cancel and reschedule (NULL = no
    // restriction). Pulled separately to keep the existing tuple compact.
    let cancel_notice_min: Option<i32> = sqlx::query_scalar::<_, Option<i32>>(
        "SELECT cancel_notice_min FROM event_types WHERE id = ?",
    )
    .bind(&et_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .flatten();
    let reschedule_notice_min: Option<i32> = sqlx::query_scalar::<_, Option<i32>>(
        "SELECT reschedule_notice_min FROM event_types WHERE id = ?",
    )
    .bind(&et_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .flatten();
    let (form_cancel_notice_value, form_cancel_notice_unit) =
        minutes_to_notice_form(cancel_notice_min);
    let (form_reschedule_notice_value, form_reschedule_notice_unit) =
        minutes_to_notice_form(reschedule_notice_min);
    // Blank renders as "no limit"; 0 is a real value meaning "today only".
    let form_booking_horizon_days = get_booking_horizon(&state.pool, &et_id)
        .await
        .map(|v| v.to_string())
        .unwrap_or_default();
    let form_pending = PendingSettings::load(&state.pool, &et_id).await.context();

    let meeting_pattern_override: Option<String> = sqlx::query_scalar::<_, Option<String>>(
        "SELECT meeting_pattern_override FROM event_types WHERE id = ?",
    )
    .bind(&et_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .flatten();

    // Get current availability rules
    let all_rules: Vec<(i32, String, String)> = sqlx::query_as(
        "SELECT day_of_week, start_time, end_time FROM availability_rules WHERE event_type_id = ? ORDER BY day_of_week, start_time",
    )
    .bind(&et_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let avail_days: String = {
        let mut days: Vec<i32> = all_rules.iter().map(|(d, _, _)| *d).collect();
        days.sort();
        days.dedup();
        days.iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join(",")
    };

    let mut windows: Vec<(String, String)> = Vec::new();
    for (_, s, e) in &all_rules {
        let pair = (s.clone(), e.clone());
        if !windows.contains(&pair) {
            windows.push(pair);
        }
    }
    let avail_windows: String = windows
        .iter()
        .map(|(s, e)| format!("{}-{}", s, e))
        .collect::<Vec<_>>()
        .join(",");
    let (avail_start, avail_end) = windows
        .first()
        .cloned()
        .unwrap_or_else(|| ("09:00".to_string(), "17:00".to_string()));

    // Build per-day schedule string
    let avail_schedule = build_avail_schedule(&all_rules);

    // Fetch team members with per-ET weights (round-robin priority OR collective exclusion)
    let is_round_robin_group = scheduling_mode == "round_robin";
    let is_collective_team = scheduling_mode == "collective";
    let members_ctx: Vec<minijinja::Value> = if is_round_robin_group || is_collective_team {
        // See the note on the personal-ET edit path: pulling `timezone` makes
        // wrong-TZ users visible on the Member priority list.
        let members: Vec<(String, String, Option<String>, String)> = sqlx::query_as(
            "SELECT u.id, u.name, u.avatar_path, u.timezone \
             FROM users u JOIN team_members tm ON tm.user_id = u.id \
             WHERE tm.team_id = ? AND u.enabled = 1 \
             ORDER BY u.name",
        )
        .bind(&team_id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();

        let et_weights: Vec<(String, i64)> = sqlx::query_as(
            "SELECT user_id, weight FROM event_type_member_weights WHERE event_type_id = ?",
        )
        .bind(&et_id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();
        let wmap: std::collections::HashMap<String, i64> = et_weights.into_iter().collect();

        members
            .iter()
            .map(|(uid, name, avatar_path, timezone)| {
                let w = wmap.get(uid).copied().unwrap_or(1);
                context! {
                    user_id => uid,
                    name => name,
                    has_avatar => avatar_path.is_some(),
                    initials => compute_initials(name),
                    weight => w,
                    timezone => timezone,
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    // Load all teams for watcher selection
    let all_teams: Vec<(String, String)> = if is_admin {
        sqlx::query_as("SELECT id, name FROM teams ORDER BY name")
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default()
    } else {
        sqlx::query_as(
            "SELECT t.id, t.name FROM teams t JOIN team_members tm ON tm.team_id = t.id WHERE tm.user_id = ? ORDER BY t.name",
        )
        .bind(&user.id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default()
    };
    let watcher_teams_ctx: Vec<minijinja::Value> = all_teams
        .iter()
        .map(|(id, name)| context! { id => id, name => name })
        .collect();

    // Load selected watcher team IDs
    let selected_watchers: Vec<(String,)> =
        sqlx::query_as("SELECT team_id FROM event_type_watchers WHERE event_type_id = ?")
            .bind(&et_id)
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default();
    let selected_watcher_team_ids: String = selected_watchers
        .iter()
        .map(|(id,)| id.as_str())
        .collect::<Vec<_>>()
        .join(",");

    // Fetch booking frequency limits
    let freq_limits: Vec<(i32, String, i32)> = sqlx::query_as(
        "SELECT max_bookings, period, per_member FROM booking_frequency_limits WHERE event_type_id = ?",
    )
    .bind(&et_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let form_frequency_limits = freq_limits
        .iter()
        .map(|(c, p, m)| {
            if *m != 0 {
                format!("{}:{}:m", c, p)
            } else {
                format!("{}:{}", c, p)
            }
        })
        .collect::<Vec<_>>()
        .join(",");

    let tmpl = match state.templates.get_template("event_type_form.html") {
        Ok(t) => t,
        Err(e) => return internal_error_html("template render", &e),
    };

    let attachable = attachable_resources(
        &state.pool,
        &auth_user.user.id,
        auth_user.user.role == "admin",
        Some(&team_id),
    )
    .await;
    let (resources_all, selected_resource_ids, resource_scheduling_mode) =
        resources_form_ctx(&state.pool, Some(&et_id), &attachable).await;
    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);
    Html(
        tmpl.render(context! {
            editing => true,
            is_group => true,
            form_team_id => team_id,
            original_slug => et_slug,
            resources_all => resources_all,
            can_manage_resources => auth_user.user.role == "admin",
            can_enable_sms => can_enable_sms(&state.pool, &auth_user.user).await,
            selected_resource_ids => selected_resource_ids,
            resource_scheduling_mode => resource_scheduling_mode,
            form_title => et_title,
            form_slug => et_slug,
            form_description => et_desc.unwrap_or_default(),
            form_duration => duration,
            form_slot_interval => slot_interval.unwrap_or(0),
            form_buffer_before => buf_before,
            form_buffer_after => buf_after,
            form_min_notice => min_notice,
            form_requires_confirmation => requires_conf != 0,
            form_visibility => visibility,
            form_location_type => loc_type,
            form_location_value => loc_value.unwrap_or_default(),
            form_meeting_pattern_override => meeting_pattern_override.clone().unwrap_or_default(),
            form_avail_days => avail_days,
            form_avail_start => avail_start,
            form_avail_end => avail_end,
            form_avail_windows => avail_windows,
            form_avail_schedule => avail_schedule,
            form_reminder_minutes => reminder_min.unwrap_or(0),
            form_max_additional_guests => max_additional_guests,
            form_scheduling_mode => scheduling_mode,
            form_default_calendar_view => default_calendar_view,
            form_first_slot_only => first_slot_only != 0,
            form_frequency_limits => form_frequency_limits,
            form_timezone => &form_timezone,
            form_cancel_notice_value => form_cancel_notice_value,
            form_cancel_notice_unit => form_cancel_notice_unit,
            form_reschedule_notice_value => form_reschedule_notice_value,
            form_reschedule_notice_unit => form_reschedule_notice_unit,
            form_booking_horizon_days => form_booking_horizon_days,
            form_pending => form_pending,
            tz_options => timezone_options(&form_timezone, auth_user.lang),
            is_round_robin_group => is_round_robin_group,
            is_collective_team => is_collective_team,
            priority_members => members_ctx,
            watcher_teams => watcher_teams_ctx,
            selected_watcher_team_ids => selected_watcher_team_ids,
            error => "",
            sidebar => sidebar_context(&auth_user, "event-types"),
            lang => auth_user.lang,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_else(|e| internal_error_body("template render", &e)),
    )
}

pub(super) async fn update_group_event_type(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path((team_id, slug)): Path<(String, String)>,
    Form(form): Form<EventTypeForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &form._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    let is_admin = user.role == "admin";

    let et: Option<(String, String)> = if is_admin {
        sqlx::query_as(
            "SELECT et.id, et.team_id
             FROM event_types et
             WHERE et.team_id = ? AND et.slug = ?",
        )
        .bind(&team_id)
        .bind(&slug)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
    } else {
        sqlx::query_as(
            "SELECT et.id, et.team_id
             FROM event_types et
             JOIN team_members tm ON tm.team_id = et.team_id
             WHERE tm.user_id = ? AND tm.role = 'admin' AND et.team_id = ? AND et.slug = ?",
        )
        .bind(&user.id)
        .bind(&team_id)
        .bind(&slug)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
    };

    let (et_id, team_id) = match et {
        Some(e) => e,
        None => return Redirect::to("/dashboard/event-types").into_response(),
    };

    let new_slug = match validate_event_type_form(&form) {
        Ok(slug) => slug,
        Err(key) => {
            return render_event_type_form_error(
                &state,
                &auth_user,
                &crate::i18n::translate(auth_user.lang, key, None),
                &form,
                true,
            )
            .await
            .into_response()
        }
    };
    let requires_confirmation = form.requires_confirmation.as_deref() == Some("on");
    let sms_phone_mode = resolve_phone_mode(
        &state.pool,
        user,
        form.sms_phone_mode.as_deref(),
        Some(&et_id),
    )
    .await;
    let visibility = form.visibility.as_deref().unwrap_or("public").to_string();

    // Check slug uniqueness within the team if changed
    if new_slug != slug {
        let existing: Option<(String,)> =
            sqlx::query_as("SELECT id FROM event_types WHERE team_id = ? AND slug = ?")
                .bind(&team_id)
                .bind(&new_slug)
                .fetch_optional(&state.pool)
                .await
                .unwrap_or(None);

        if existing.is_some() {
            return render_event_type_form_error(
                &state,
                &auth_user,
                &crate::i18n::translate(
                    auth_user.lang,
                    "form-error-event-type-slug-taken-team",
                    None,
                ),
                &form,
                true,
            )
            .await
            .into_response();
        }
    }

    let location_type = form.location_type.as_deref().unwrap_or("link");
    let location_value = form
        .location_value
        .as_deref()
        .filter(|s| !s.trim().is_empty());

    let location_required = !meeting::is_auto_location(location_type);
    if location_required && location_value.is_none() {
        return render_event_type_form_error(
            &state,
            &auth_user,
            &crate::i18n::translate(auth_user.lang, "form-error-location-required", None),
            &form,
            true,
        )
        .await
        .into_response();
    }

    if let Some(err) = google_meet_location_error(
        &state.pool,
        location_type,
        None,
        Some(&team_id),
        Some(&et_id),
    )
    .await
    {
        return render_event_type_form_error(&state, &auth_user, &err, &form, true)
            .await
            .into_response();
    }

    let reminder_minutes = {
        let v = parse_int_field(&form.reminder_minutes, 0);
        if v > 0 {
            Some(v)
        } else {
            None
        }
    };

    let default_calendar_view = match form.default_calendar_view.as_deref().unwrap_or("month") {
        v @ ("month" | "week" | "column") => v.to_string(),
        _ => "month".to_string(),
    };

    let timezone = resolve_event_type_tz(
        form.timezone.as_deref(),
        stored_event_type_tz(&state.pool, &et_id).await.as_deref(),
        &event_type_followed_tz(&state.pool, &et_id).await,
    );
    let cancel_notice_min =
        parse_notice_to_minutes(&form.cancel_notice_value, &form.cancel_notice_unit);
    let reschedule_notice_min =
        parse_notice_to_minutes(&form.reschedule_notice_value, &form.reschedule_notice_unit);
    let booking_horizon_days = parse_optional_day_count(&form.booking_horizon_days);

    let meeting_pattern_override = form
        .meeting_pattern_override
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let _ = sqlx::query(
        "UPDATE event_types SET slug = ?, title = ?, description = ?, duration_min = ?, slot_interval_min = ?, buffer_before = ?, buffer_after = ?, min_notice_min = ?, requires_confirmation = ?, location_type = ?, location_value = ?, reminder_minutes = ?, visibility = ?, max_additional_guests = ?, scheduling_mode = ?, default_calendar_view = ?, first_slot_only = ?, timezone = ?, cancel_notice_min = ?, reschedule_notice_min = ?, meeting_pattern_override = ?, sms_phone_mode = ?, booking_horizon_days = ? WHERE id = ?",
    )
    .bind(&new_slug)
    .bind(form.title.trim())
    .bind(form.description.as_deref().filter(|s| !s.trim().is_empty()))
    .bind(parse_int_field(&form.duration_min, 30))
    .bind(parse_optional_positive_int(&form.slot_interval_min))
    .bind(parse_int_field(&form.buffer_before, 0))
    .bind(parse_int_field(&form.buffer_after, 0))
    .bind(parse_int_field(&form.min_notice_min, 60))
    .bind(requires_confirmation as i32)
    .bind(location_type)
    .bind(location_value)
    .bind(reminder_minutes)
    .bind(&visibility)
    .bind(parse_int_field(&form.max_additional_guests, 0))
    .bind(form.scheduling_mode.as_deref().unwrap_or("round_robin"))
    .bind(&default_calendar_view)
    .bind(if form.first_slot_only.as_deref() == Some("on") { 1 } else { 0 })
    .bind(&timezone)
    .bind(cancel_notice_min)
    .bind(reschedule_notice_min)
    .bind(&meeting_pattern_override)
    .bind(&sms_phone_mode)
    .bind(booking_horizon_days)
    .bind(&et_id)
    .execute(&state.pool)
    .await;
    save_pending_settings(&state.pool, &et_id, &form, false).await;

    // Update availability rules. Pass the editing user's profile-default
    // schedule as a fallback so an empty submission falls back to it instead
    // of hardcoded Mon-Fri 09:00-17:00.
    let _ = sqlx::query("DELETE FROM availability_rules WHERE event_type_id = ?")
        .bind(&et_id)
        .execute(&state.pool)
        .await;

    ensure_user_avail_seeded(&state.pool, &user.id).await;
    let user_default = load_user_avail_schedule(&state.pool, &user.id).await;
    let schedule = parse_avail_schedule(
        form.avail_schedule.as_deref(),
        form.avail_days.as_deref(),
        form.avail_windows.as_deref(),
        form.avail_start.as_deref(),
        form.avail_end.as_deref(),
        Some(&user_default),
    );

    for (day, windows) in &schedule {
        for (ws, we) in windows {
            let rule_id = uuid::Uuid::new_v4().to_string();
            let _ = sqlx::query(
                "INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&rule_id)
            .bind(&et_id)
            .bind(day)
            .bind(ws)
            .bind(we)
            .execute(&state.pool)
            .await;
        }
    }

    // Update watcher teams: delete old, insert new
    let _ = sqlx::query("DELETE FROM event_type_watchers WHERE event_type_id = ?")
        .bind(&et_id)
        .execute(&state.pool)
        .await;

    if let Some(ref watcher_ids) = form.watcher_team_ids {
        for wid in watcher_ids.split(',') {
            let wid = wid.trim();
            if !wid.is_empty() {
                let _ = sqlx::query(
                    "INSERT INTO event_type_watchers (event_type_id, team_id) VALUES (?, ?)",
                )
                .bind(&et_id)
                .bind(wid)
                .execute(&state.pool)
                .await;
            }
        }
    }

    // Update booking frequency limits: delete old, insert new
    let _ = sqlx::query("DELETE FROM booking_frequency_limits WHERE event_type_id = ?")
        .bind(&et_id)
        .execute(&state.pool)
        .await;
    save_frequency_limits(&state.pool, &et_id, &form.frequency_limits).await;

    save_event_type_resources_authorized(
        &state.pool,
        &et_id,
        &auth_user.user.id,
        auth_user.user.role == "admin",
        Some(&team_id),
        &form.resource_ids,
        &form.resource_scheduling_mode,
    )
    .await;

    Redirect::to("/dashboard/event-types").into_response()
}

pub(super) async fn toggle_group_event_type(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path((team_id, slug)): Path<(String, String)>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    let is_admin = user.role == "admin";

    // Scope by team_id from the route to disambiguate slugs shared across teams.
    let et: Option<(String,)> = if is_admin {
        sqlx::query_as("SELECT id FROM event_types WHERE team_id = ? AND slug = ?")
            .bind(&team_id)
            .bind(&slug)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None)
    } else {
        sqlx::query_as(
            "SELECT et.id FROM event_types et \
             JOIN team_members tm ON tm.team_id = et.team_id \
             WHERE et.team_id = ? AND et.slug = ? AND tm.user_id = ? AND tm.role = 'admin' \
             LIMIT 1",
        )
        .bind(&team_id)
        .bind(&slug)
        .bind(&user.id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
    };

    if let Some((et_id,)) = et {
        let _ = sqlx::query(
            "UPDATE event_types SET enabled = CASE WHEN enabled = 1 THEN 0 ELSE 1 END WHERE id = ?",
        )
        .bind(&et_id)
        .execute(&state.pool)
        .await;

        tracing::debug!(event_type_id = %et_id, event_type_slug = %slug, "group event type toggled");
    }

    Redirect::to("/dashboard/event-types").into_response()
}

pub(super) async fn delete_group_event_type(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    headers: HeaderMap,
    Path((team_id, slug)): Path<(String, String)>,
    Form(csrf): Form<CsrfForm>,
) -> impl IntoResponse {
    if let Err(resp) = verify_csrf_token(&headers, &csrf._csrf) {
        return resp;
    }
    let user = &auth_user.user;

    let is_admin = user.role == "admin";

    let et: Option<(String,)> = if is_admin {
        sqlx::query_as(
            "SELECT et.id FROM event_types et
             WHERE et.team_id = ? AND et.slug = ?",
        )
        .bind(&team_id)
        .bind(&slug)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
    } else {
        sqlx::query_as(
            "SELECT et.id FROM event_types et
             JOIN team_members tm ON tm.team_id = et.team_id
             WHERE et.team_id = ? AND et.slug = ? AND tm.user_id = ? AND tm.role = 'admin' LIMIT 1",
        )
        .bind(&team_id)
        .bind(&slug)
        .bind(&user.id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
    };

    let et_id = match et {
        Some((id,)) => id,
        None => return Redirect::to("/dashboard/event-types").into_response(),
    };

    // Check for active bookings
    let active_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM bookings WHERE event_type_id = ? AND status IN ('confirmed', 'pending')",
    )
    .bind(&et_id)
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    if active_count > 0 {
        return Redirect::to("/dashboard/event-types").into_response();
    }

    // Delete in order: availability_rules, availability_overrides, event_type_calendars, bookings, event_type
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

    tracing::info!(event_type_id = %et_id, user = %auth_user.user.email, "group event type deleted");

    Redirect::to("/dashboard/event-types").into_response()
}
