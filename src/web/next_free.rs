//! "Next free" hints on the public landing pages (`/u/{username}` and
//! `/team/{slug}`). Each meeting row asks
//! `GET /u/{username}/{slug}/next-free` or
//! `GET /team/{team_slug}/{slug}/next-free[?invite=]` after paint and gets
//! `{"start": "<UTC>" | null, "days": ["YYYY-MM-DD", ...]}`: the first free
//! start within [`SEARCH_DAYS`] (or the booking window, if shorter), and the
//! days of the coming week, in the guest's `tz`, that have a free slot.
//!
//! It answers only for event types the landing page would list, with the same
//! "not found" as the slot page otherwise. It never syncs a calendar or
//! fetches a resource feed: it reads the cache, like the submit-time check.
//! No booking is ever made from it, so a failing read answers `null` (and is
//! logged) rather than an error.

use super::*;

/// How far ahead the hint looks for a free slot.
const SEARCH_DAYS: i32 = 60;

/// The spans searched, in order (host-day offset, days), so a type with a
/// slot this week costs one span of slot computation. The first span runs
/// eight host days, which covers the guest's seven whatever their offset
/// from the host (at most a day either way).
const SPANS: [(i32, i32); 3] = [(0, 8), (8, 13), (21, SEARCH_DAYS - 21)];

#[derive(Deserialize)]
pub(super) struct NextFreeQuery {
    invite: Option<String>,
    tz: Option<String>,
}

#[derive(serde::Serialize, Debug, Default, PartialEq)]
pub(super) struct NextFree {
    /// The first free start, UTC, `YYYY-MM-DDTHH:MM:SSZ`.
    pub(super) start: Option<String>,
    /// The guest-local dates of the next seven days (today first) with at
    /// least one free slot.
    pub(super) days: Vec<String>,
}

fn not_found() -> Response {
    (
        axum::http::StatusCode::NOT_FOUND,
        [("Cache-Control", "no-store")],
        "not found",
    )
        .into_response()
}

fn answer(found: NextFree) -> Response {
    (
        [
            ("Content-Type", "application/json"),
            // The answer can depend on who asks (a team member sees more
            // types), so only the visitor's own browser may keep it.
            ("Cache-Control", "private, max-age=60"),
        ],
        serde_json::to_string(&found).unwrap_or_else(|_| "{}".to_string()),
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
        r#"{"start":null,"days":[]}"#,
    )
        .into_response()
}

/// The guest's timezone from `?tz=`, else the host's.
fn guest_tz(query: &NextFreeQuery, host_tz: Tz) -> Tz {
    query
        .tz
        .as_deref()
        .and_then(|tz| tz.parse::<Tz>().ok())
        .unwrap_or(host_tz)
}

pub(super) async fn user_next_free(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((username, slug)): Path<(String, String)>,
    Query(query): Query<NextFreeQuery>,
) -> Response {
    if state
        .next_free_limiter
        .check_limited(&client_ip_for_rate_limit(&headers))
        .await
    {
        return too_many();
    }
    // Exactly the types /u/{username} lists: public, personal, enabled, of an
    // enabled user, not past their last bookable day.
    let row: Result<Option<(String,)>, sqlx::Error> = sqlx::query_as(
        "SELECT et.id FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         JOIN users u ON u.id = a.user_id
         WHERE u.username = ? AND u.enabled = 1 AND et.slug = ? AND et.enabled = 1
           AND et.visibility = 'public' AND et.team_id IS NULL",
    )
    .bind(&username)
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await;
    let et_id = match row {
        Ok(Some((id,))) => id,
        Ok(None) => return not_found(),
        Err(error) => {
            tracing::error!(%error, "next-free: event type lookup failed");
            return answer(NextFree::default());
        }
    };
    respond(&state, &et_id, &query).await
}

pub(super) async fn team_next_free(
    State(state): State<Arc<AppState>>,
    optional_auth: crate::auth::OptionalAuthUser,
    headers: HeaderMap,
    Path((team_slug, slug)): Path<(String, String)>,
    Query(query): Query<NextFreeQuery>,
) -> Response {
    if state
        .next_free_limiter
        .check_limited(&client_ip_for_rate_limit(&headers))
        .await
    {
        return too_many();
    }
    let row: Result<Option<(String, String, String, String, Option<String>)>, sqlx::Error> =
        sqlx::query_as(
            "SELECT et.id, et.visibility, t.id, t.visibility, t.invite_token
             FROM event_types et JOIN teams t ON t.id = et.team_id
             WHERE t.slug = ? AND et.slug = ? AND et.enabled = 1",
        )
        .bind(&team_slug)
        .bind(&slug)
        .fetch_optional(&state.pool)
        .await;
    let (et_id, et_visibility, team_id, team_visibility, team_invite) = match row {
        Ok(Some(row)) => row,
        Ok(None) => return not_found(),
        Err(error) => {
            tracing::error!(%error, "next-free: team event type lookup failed");
            return answer(NextFree::default());
        }
    };
    // The rules of /team/{slug}: members and admins see every enabled type;
    // anyone else only public types, and of a private team only with its
    // invite token.
    let member_or_admin = match &optional_auth.user {
        Some(u) if u.role == "admin" => true,
        Some(u) => is_team_member(&state.pool, &u.id, &team_id).await,
        None => false,
    };
    if !member_or_admin {
        let invite_ok = match (&team_invite, query.invite.as_deref()) {
            (Some(expected), Some(given)) if !given.is_empty() => {
                bool::from(expected.as_bytes().ct_eq(given.as_bytes()))
            }
            _ => false,
        };
        if et_visibility != "public" || (team_visibility == "private" && !invite_ok) {
            return not_found();
        }
    }
    respond(&state, &et_id, &query).await
}

async fn respond(state: &AppState, et_id: &str, query: &NextFreeQuery) -> Response {
    // A type past its last bookable day is not listed on the landing page.
    if without_expired(&state.pool, vec![et_id.to_string()], |id| id)
        .await
        .is_empty()
    {
        return not_found();
    }
    let host_tz = get_host_tz(&state.pool, et_id).await;
    let guest_tz = guest_tz(query, host_tz);
    match next_free(&state.pool, et_id, host_tz, guest_tz).await {
        Ok(found) => answer(found),
        Err(error) => {
            tracing::error!(%error, %et_id, "next-free: slot read failed");
            answer(NextFree::default())
        }
    }
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
            let Some(start) = host_tz.from_local_datetime(&local).single() else {
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
