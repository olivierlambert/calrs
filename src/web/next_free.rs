//! "Next free" hints on the public landing pages (`/u/{username}` and
//! `/team/{slug}`). After paint, each page asks once, for all the meetings
//! it lists: `GET /u/{username}/next-free` or
//! `GET /team/{team_slug}/next-free[?invite=]`, optionally `&tz=` (the
//! guest's IANA zone). The answer maps each listed slug to
//! `{"start": "<UTC>" | null, "days": ["YYYY-MM-DD", ...]}`: the first free
//! start within [`SEARCH_DAYS`] (or the booking window, if shorter), and the
//! days of the coming week, in the guest's zone, that have a free slot.
//!
//! It covers exactly the types the landing page lists, under its rules, and
//! answers "not found" where the page would. It never syncs a calendar or
//! fetches a resource feed: it reads the cache, like the submit-time check,
//! and keeps each type's answer for [`CACHE_SECS`]. No booking is ever made
//! from it, so a failing read answers `null` for that type (logged at most
//! once a minute per type) rather than an error.

use super::*;

/// How far ahead the hint looks for a free slot.
const SEARCH_DAYS: i32 = 60;

/// The spans searched, in order (host-day offset, days), so a type with a
/// slot this week costs one span of slot computation. The first span runs
/// eight host days, which covers the guest's seven whatever their offset
/// from the host (at most a day either way).
const SPANS: [(i32, i32); 3] = [(0, 8), (8, 13), (21, SEARCH_DAYS - 21)];

/// How long one type's answer for one zone is kept (`AppState::next_free_cache`).
const CACHE_SECS: u64 = 60;

/// Entries kept at most; past it the cache starts over.
const CACHE_MAX: usize = 2000;

#[derive(Deserialize)]
pub(super) struct NextFreeQuery {
    invite: Option<String>,
    tz: Option<String>,
}

#[derive(serde::Serialize, Clone, Debug, Default, PartialEq)]
pub struct NextFree {
    /// The first free start, UTC, `YYYY-MM-DDTHH:MM:SSZ`.
    pub start: Option<String>,
    /// The guest-local dates of the next seven days (today first) with at
    /// least one free slot.
    pub days: Vec<String>,
}

/// Per-type answers by `"{event type id}|{zone}"`, with the time each was
/// computed.
pub type NextFreeCache = std::sync::Mutex<HashMap<String, (std::time::Instant, NextFree)>>;

fn not_found() -> Response {
    (
        axum::http::StatusCode::NOT_FOUND,
        [("Cache-Control", "no-store")],
        "not found",
    )
        .into_response()
}

fn too_many() -> Response {
    (
        axum::http::StatusCode::TOO_MANY_REQUESTS,
        [
            ("Content-Type", "application/json"),
            ("Cache-Control", "no-store"),
        ],
        "{}",
    )
        .into_response()
}

/// The page's answer. `signed_in`: the team page shows a member more types,
/// so a signed-in answer is never stored, and a visitor's varies on the
/// cookie.
fn answer(found: &std::collections::BTreeMap<String, NextFree>, signed_in: bool) -> Response {
    let body = serde_json::to_string(found).unwrap_or_else(|_| "{}".to_string());
    if signed_in {
        (
            [
                ("Content-Type", "application/json"),
                ("Cache-Control", "private, no-store"),
            ],
            body,
        )
            .into_response()
    } else {
        (
            [
                ("Content-Type", "application/json"),
                ("Cache-Control", "private, max-age=60"),
                ("Vary", "Cookie"),
            ],
            body,
        )
            .into_response()
    }
}

/// The guest's timezone from `?tz=`, else the host's.
fn guest_tz(query: &NextFreeQuery, host_tz: Tz) -> Tz {
    query
        .tz
        .as_deref()
        .and_then(|tz| tz.parse::<Tz>().ok())
        .unwrap_or(host_tz)
}

/// Logs a failed read for `et_id` at most once a minute.
fn warn_read_failed(et_id: &str, error: &sqlx::Error) {
    static LAST: std::sync::OnceLock<std::sync::Mutex<HashMap<String, std::time::Instant>>> =
        std::sync::OnceLock::new();
    let now = std::time::Instant::now();
    let Ok(mut last) = LAST.get_or_init(Default::default).lock() else {
        return;
    };
    let due = last
        .get(et_id)
        .is_none_or(|at| now.duration_since(*at).as_secs() >= 60);
    if due {
        last.retain(|_, at| now.duration_since(*at).as_secs() < 60);
        last.insert(et_id.to_string(), now);
        tracing::warn!(%error, %et_id, "next-free: slot read failed, answering null");
    }
}

/// The types `/u/{username}` lists, as (slug, id): public, personal and
/// enabled, not past their last bookable day.
async fn user_page_types(pool: &SqlitePool, user_id: &str) -> Vec<(String, String)> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT et.slug, et.id
         FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         WHERE a.user_id = ? AND et.enabled = 1 AND et.visibility = 'public'
         AND et.team_id IS NULL
         ORDER BY et.created_at",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    without_expired(pool, rows, |row| &row.1).await
}

/// The types `/team/{slug}` lists, as (slug, id): every enabled type for a
/// member or an admin, public ones otherwise, not past their last bookable
/// day.
async fn team_page_types(
    pool: &SqlitePool,
    team_id: &str,
    member_or_admin: bool,
) -> Vec<(String, String)> {
    let visibility = if member_or_admin {
        ""
    } else {
        " AND et.visibility = 'public'"
    };
    let rows: Vec<(String, String)> = sqlx::query_as(&format!(
        "SELECT et.slug, et.id FROM event_types et
         WHERE et.team_id = ? AND et.enabled = 1{visibility}
         ORDER BY et.created_at"
    ))
    .bind(team_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    without_expired(pool, rows, |row| &row.1).await
}

pub(super) async fn user_next_free(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(username): Path<String>,
    Query(query): Query<NextFreeQuery>,
) -> Response {
    if state
        .next_free_limiter
        .check_limited(&client_ip_for_rate_limit(&headers))
        .await
    {
        return too_many();
    }
    let user_id: Option<String> =
        sqlx::query_scalar("SELECT id FROM users WHERE username = ? AND enabled = 1")
            .bind(&username)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);
    let Some(user_id) = user_id else {
        return not_found();
    };
    let types = user_page_types(&state.pool, &user_id).await;
    answer(&page_answers(&state, &types, &query).await, false)
}

pub(super) async fn team_next_free(
    State(state): State<Arc<AppState>>,
    optional_auth: crate::auth::OptionalAuthUser,
    headers: HeaderMap,
    Path(team_slug): Path<String>,
    Query(query): Query<NextFreeQuery>,
) -> Response {
    if state
        .next_free_limiter
        .check_limited(&client_ip_for_rate_limit(&headers))
        .await
    {
        return too_many();
    }
    let team: Option<(String, String, Option<String>)> =
        sqlx::query_as("SELECT id, visibility, invite_token FROM teams WHERE slug = ?")
            .bind(&team_slug)
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None);
    let Some((team_id, team_visibility, team_invite)) = team else {
        return not_found();
    };
    // The rules of /team/{slug}: members and admins see every enabled type
    // of any team; anyone else a private team only with its invite token,
    // and public types only.
    let member_or_admin = match &optional_auth.user {
        Some(u) if u.role == "admin" => true,
        Some(u) => is_team_member(&state.pool, &u.id, &team_id).await,
        None => false,
    };
    if team_visibility == "private" && !member_or_admin {
        let invite_ok = match (&team_invite, query.invite.as_deref()) {
            (Some(expected), Some(given)) if !given.is_empty() => {
                bool::from(expected.as_bytes().ct_eq(given.as_bytes()))
            }
            _ => false,
        };
        if !invite_ok {
            return not_found();
        }
    }
    let types = team_page_types(&state.pool, &team_id, member_or_admin).await;
    let signed_in = optional_auth.user.is_some();
    answer(&page_answers(&state, &types, &query).await, signed_in)
}

/// Each listed type's answer, from the cache when it is fresh.
async fn page_answers(
    state: &AppState,
    types: &[(String, String)],
    query: &NextFreeQuery,
) -> std::collections::BTreeMap<String, NextFree> {
    let mut all = std::collections::BTreeMap::new();
    for (slug, et_id) in types {
        let host_tz = get_host_tz(&state.pool, et_id).await;
        let tz = guest_tz(query, host_tz);
        let key = format!("{et_id}|{}", tz.name());
        let cached = state.next_free_cache.lock().ok().and_then(|cache| {
            cache
                .get(&key)
                .filter(|(at, _)| at.elapsed().as_secs() < CACHE_SECS)
                .map(|(_, found)| found.clone())
        });
        let found = match cached {
            Some(found) => found,
            None => {
                let found = match next_free(&state.pool, et_id, host_tz, tz).await {
                    Ok(found) => found,
                    Err(error) => {
                        warn_read_failed(et_id, &error);
                        NextFree::default()
                    }
                };
                if let Ok(mut cache) = state.next_free_cache.lock() {
                    if cache.len() >= CACHE_MAX {
                        cache.retain(|_, (at, _)| at.elapsed().as_secs() < CACHE_SECS);
                        if cache.len() >= CACHE_MAX {
                            cache.clear();
                        }
                    }
                    cache.insert(key, (std::time::Instant::now(), found.clone()));
                }
                found
            }
        };
        all.insert(slug.clone(), found);
    }
    all
}

/// The first free start of event type `et_id` within [`SEARCH_DAYS`] (the
/// booking window applies inside the slot engine), and the free days of the
/// coming week in `guest_tz`. Cache only: no sync, no feed fetch.
pub(super) async fn next_free(
    pool: &SqlitePool,
    et_id: &str,
    host_tz: Tz,
    guest_tz: Tz,
) -> Result<NextFree, sqlx::Error> {
    let et: Option<(i32, i32, i32, i32, Option<String>, String, String)> = sqlx::query_as(
        "SELECT et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min,
                et.team_id, et.scheduling_mode, COALESCE(a.user_id, '')
         FROM event_types et LEFT JOIN accounts a ON a.id = et.account_id WHERE et.id = ?",
    )
    .bind(et_id)
    .fetch_optional(pool)
    .await?;
    let Some((duration, buffer_before, buffer_after, min_notice, team_id, mode, owner)) = et else {
        return Ok(NextFree::default());
    };
    let window = try_get_booking_window(pool, et_id).await?;
    let now = Utc::now();
    let host_today = now.with_timezone(&host_tz).date_naive();
    let guest_today = now.with_timezone(&guest_tz).date_naive();
    let week: Vec<NaiveDate> = (0..7).map(|i| guest_today + Duration::days(i)).collect();
    let mut found = NextFree::default();
    for (start_offset, days_ahead) in SPANS {
        let day = |offset: i32| host_today + Duration::days(offset as i64);
        // Padded by a day on each side, as the submit-time check pads it, so
        // buffers at the edges see the same events the slot page does.
        let busy_start = day(start_offset - 1).and_time(NaiveTime::MIN);
        let busy_end = day(start_offset + days_ahead + 1)
            .and_hms_opt(23, 59, 59)
            .unwrap_or(busy_start);
        let busy = match &team_id {
            Some(team_id) => {
                try_team_busy_source(
                    pool,
                    et_id,
                    team_id,
                    mode == "collective",
                    busy_start,
                    busy_end,
                    host_tz,
                )
                .await?
            }
            None => BusySource::Individual(
                try_fetch_busy_times_for_user(
                    pool,
                    &owner,
                    busy_start,
                    busy_end,
                    host_tz,
                    Some(et_id),
                )
                .await?,
            ),
        };
        let days = try_compute_slots_cached(
            pool,
            et_id,
            duration,
            buffer_before,
            buffer_after,
            min_notice,
            start_offset,
            days_ahead,
            window,
            host_tz,
            guest_tz,
            busy,
            None,
        )
        .await?;
        for slot in days.iter().flat_map(|d| &d.slots) {
            let Ok(local) = NaiveDateTime::parse_from_str(
                &format!("{} {}", slot.host_date, slot.host_time),
                "%Y-%m-%d %H:%M",
            ) else {
                continue;
            };
            // In a clock change's repeated hour, the earlier instant.
            let Some(start) = host_tz.from_local_datetime(&local).earliest() else {
                continue;
            };
            let start = start.with_timezone(&Utc);
            if start <= now {
                continue;
            }
            if found.start.is_none() {
                found.start = Some(start.format("%Y-%m-%dT%H:%M:%SZ").to_string());
            }
            let guest_day = start.with_timezone(&guest_tz).date_naive();
            if week.contains(&guest_day) {
                let day = guest_day.format("%Y-%m-%d").to_string();
                if !found.days.contains(&day) {
                    found.days.push(day);
                }
            }
        }
        // The first span holds the whole guest week, so once a start is
        // known, both answers are complete.
        if found.start.is_some() {
            break;
        }
    }
    found.days.sort();
    Ok(found)
}
