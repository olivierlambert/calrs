//! REST API v1 (#169): event types, slots and bookings as JSON under
//! `/api/v1`. Authenticated by a Bearer token only, never by cookies: an OAuth
//! access token bound to the resource `<base>/api/v1` (`oauth::api_resource`),
//! or a personal access token (`web::api_tokens`). Every request is checked
//! anew against the user, the token's scopes and the same ownership rules as
//! the dashboard; writes go through the booking form's and the dashboard's
//! own code (`book_personal`, `book_team`, `host_cancel_booking`,
//! `host_request_reschedule`). Documented in `docs/src/api.md`.
use super::*;
use axum::{body::Bytes, http::StatusCode, Json};
use serde_json::{json, Value};

/// Personal access tokens start with this, so a leaked one is recognisable
/// (secret scanners) and never mistaken for an OAuth access token.
pub(super) const PAT_PREFIX: &str = "calrs_pat_";
/// Request bodies are small JSON objects.
const BODY_LIMIT: usize = 16 * 1024;
/// Page size of the lists.
const PAGE: usize = 100;
/// Longest slot window, in days.
const MAX_SLOT_DAYS: i64 = 31;
/// Bookings one user may create through the API in 24 hours.
const API_DAILY_BOOKINGS: i64 = 50;
/// Additional attendees one API booking may carry, whatever the event type
/// allows on its booking page.
const API_MAX_ATTENDEES: usize = 10;

pub(super) fn router() -> Router<Arc<AppState>> {
    let v1 = Router::new()
        .route("/event-types", get(list_event_types))
        .route("/event-types/{id}", get(get_event_type))
        .route("/event-types/{id}/slots", get(event_type_slots))
        .route("/bookings", get(list_bookings).post(create_booking))
        .route("/bookings/{id}", get(get_booking))
        .route("/bookings/{id}/cancel", post(cancel_booking_api))
        .route(
            "/bookings/{id}/request-reschedule",
            post(request_reschedule_api),
        )
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed);
    // Only `/api/v1` is claimed: `/api` and `/api/book` stay the legacy
    // booking pages of an event type whose slug is `api`.
    Router::new()
        .nest("/api/v1", v1)
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
}

// --- Errors ---

/// The one error shape: `{"error": {"code", "message"}}`. `code` is stable,
/// `message` is English prose for a person reading the logs.
#[derive(Debug)]
pub(super) struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
    /// `WWW-Authenticate` value for 401 and 403.
    challenge: Option<String>,
}

impl ApiError {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            challenge: None,
        }
    }
    fn bad(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, message)
    }
    fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", "Not found.")
    }
    fn internal(context: &str, error: &dyn std::fmt::Display) -> Self {
        tracing::error!(%error, context, "API request failed");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Something went wrong on the server; try again later.",
        )
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        Self::internal("database", &error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = (
            self.status,
            Json(json!({"error": {"code": self.code, "message": self.message}})),
        )
            .into_response();
        let headers = response.headers_mut();
        headers.insert("cache-control", "no-store".parse().unwrap());
        if let Some(challenge) = self.challenge.and_then(|c| c.parse().ok()) {
            headers.insert("www-authenticate", challenge);
        }
        if self.status == StatusCode::TOO_MANY_REQUESTS {
            headers.insert("retry-after", "60".parse().unwrap());
        }
        response
    }
}

type ApiResult = Result<Response, ApiError>;

fn ok(status: StatusCode, value: Value) -> Response {
    let mut response = (status, Json(value)).into_response();
    response
        .headers_mut()
        .insert("cache-control", "no-store".parse().unwrap());
    response
}

async fn not_found() -> ApiError {
    ApiError::not_found()
}

async fn method_not_allowed() -> ApiError {
    ApiError::new(
        StatusCode::METHOD_NOT_ALLOWED,
        "method_not_allowed",
        "Method not allowed.",
    )
}

/// English text of a message id, for error messages.
fn en(key: &str) -> String {
    crate::i18n::translate("en", key, None)
}

// --- Authentication ---

/// Who is calling: the user, and the credential (`grant:<id>` or
/// `pat:<id>`) its rate limit counts against.
pub(super) struct Caller {
    pub(super) user: crate::models::User,
    scope: String,
    credential: String,
}

impl Caller {
    fn has(&self, scope: &str) -> bool {
        self.scope.split_whitespace().any(|s| s == scope)
    }

    /// A booking as a write answers with it: in full with `bookings:read`,
    /// else only its id and status, so a write-only token reads nothing.
    /// `booking_answer` of a `{"booking": ...}` answer; an error passes as is.
    fn booking_answer_of(&self, answer: Value) -> Value {
        match answer.get("booking") {
            Some(booking) => self.booking_answer(booking.clone()),
            None => answer,
        }
    }

    fn booking_answer(&self, booking: Value) -> Value {
        if self.has("bookings:read") {
            json!({ "booking": booking })
        } else {
            json!({"booking": {"id": booking["id"], "status": booking["status"]}})
        }
    }
}

fn challenge_value(error: &str, scope: &str) -> String {
    format!(
        "Bearer error=\"{error}\", resource_metadata=\"{}/.well-known/oauth-protected-resource/api/v1\", scope=\"{scope}\"",
        crate::oauth::issuer()
    )
}

/// Look up a personal access token: unexpired, its user enabled, still
/// allowed to hold one (admin, or the admin switch on), and the MFA policy
/// of the session that created it (`oauth::GRANT_MFA_POLICY`).
async fn personal_token_user(
    pool: &SqlitePool,
    token: &str,
) -> Result<Option<(crate::models::User, String, String)>, sqlx::Error> {
    let secret = &token[PAT_PREFIX.len()..];
    if secret.len() != 43
        || !secret
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Ok(None);
    }
    let now = Utc::now().timestamp();
    let query = format!(
        "SELECT g.id, g.user_id, g.scope FROM api_tokens g JOIN users u ON u.id = g.user_id
         WHERE g.token_hash = ? AND (g.expires_at IS NULL OR g.expires_at > ?) AND u.enabled = 1
           AND (u.role = 'admin' OR (SELECT api_tokens_allow_all_users FROM auth_config WHERE id = 'singleton') = 1)
           AND {}",
        crate::oauth::GRANT_MFA_POLICY
    );
    let row: Option<(String, String, String)> = sqlx::query_as(&query)
        .bind(crate::oauth::hash(token))
        .bind(now)
        .fetch_optional(pool)
        .await?;
    let Some((id, user_id, scope)) = row else {
        return Ok(None);
    };
    let user = sqlx::query_as::<_, crate::models::User>(
        "SELECT * FROM users WHERE id = ? AND enabled = 1",
    )
    .bind(&user_id)
    .fetch_optional(pool)
    .await?;
    if user.is_some() {
        sqlx::query("UPDATE api_tokens SET last_used_at = ? WHERE id = ? AND (last_used_at IS NULL OR last_used_at < ?)")
            .bind(now)
            .bind(&id)
            .bind(now - 60)
            .execute(pool)
            .await?;
    }
    Ok(user.map(|u| (u, scope, id)))
}

/// Authenticate the request and require `scope`. Cookies are never read.
/// A request carrying an `Origin` from another site is refused: the API is
/// for servers and scripts, not for browser pages on other origins (no CORS).
async fn authenticate(
    state: &AppState,
    headers: &HeaderMap,
    scope: &'static str,
) -> Result<Caller, ApiError> {
    if let Some(origin) = headers.get("origin") {
        let expected = reqwest::Url::parse(&crate::oauth::issuer())
            .ok()
            .map(|u| u.origin().ascii_serialization());
        if origin.to_str().ok() != expected.as_deref() {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "origin_not_allowed",
                "Cross-origin browser requests are not accepted.",
            ));
        }
    }
    let token = crate::oauth::bearer_token(headers);
    let presented = token.is_some();
    let found = match token {
        Some(token) if token.starts_with(PAT_PREFIX) => personal_token_user(&state.pool, token)
            .await?
            .map(|(user, scope, id)| (user, scope, format!("pat:{id}"))),
        Some(_) => crate::oauth::bearer_grant(&state.pool, headers, &crate::oauth::api_resource())
            .await
            .map_err(|_| ApiError::internal("bearer", &"OAuth lookup"))?
            .map(|(user, scope, grant)| (user, scope, format!("grant:{grant}"))),
        None => None,
    };
    // An OAuth grant for the API stops working, like a personal token,
    // when the admin closes the API to users who are not admins.
    let found = match found {
        Some((user, scope, credential)) if credential.starts_with("grant:") => {
            let mut conn = state.pool.acquire().await?;
            crate::oauth::api_allowed(&mut conn, &user)
                .await?
                .then_some((user, scope, credential))
        }
        other => other,
    };
    let Some((user, scope_list, credential)) = found else {
        let mut error = ApiError::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "A valid Bearer token is required.",
        );
        // RFC 6750 section 3.1: no error code when no token was presented.
        error.challenge = Some(if presented {
            challenge_value("invalid_token", &crate::oauth::API_SCOPES.join(" "))
        } else {
            format!(
                "Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource/api/v1\", scope=\"{}\"",
                crate::oauth::issuer(),
                crate::oauth::API_SCOPES.join(" ")
            )
        });
        return Err(error);
    };
    if !scope_list.split_whitespace().any(|s| s == scope) {
        let mut error = ApiError::new(
            StatusCode::FORBIDDEN,
            "insufficient_scope",
            format!("This token lacks the {scope} scope."),
        );
        error.challenge = Some(challenge_value("insufficient_scope", scope));
        return Err(error);
    }
    if state
        .api_limiter
        .check_limited(&format!("api:{credential}"))
        .await
    {
        return Err(rate_limited());
    }
    Ok(Caller {
        user,
        scope: scope_list,
        credential,
    })
}

fn rate_limited() -> ApiError {
    ApiError::new(
        StatusCode::TOO_MANY_REQUESTS,
        "rate_limited",
        "Too many requests; slow down.",
    )
}

/// The second, tighter limit of the requests that do real work (slots, every
/// write), per user: several tokens of one user share it.
async fn heavy_limit(state: &AppState, caller: &Caller) -> Result<(), ApiError> {
    if state
        .api_heavy_limiter
        .check_limited(&format!("api-heavy:{}", caller.user.id))
        .await
    {
        return Err(rate_limited());
    }
    Ok(())
}

// --- Request helpers ---

type QueryParams = Result<Query<HashMap<String, String>>, axum::extract::rejection::QueryRejection>;

/// The query parameters, refusing any not in `allowed`.
fn query_params(query: QueryParams, allowed: &[&str]) -> Result<HashMap<String, String>, ApiError> {
    let Query(params) = query.map_err(|_| ApiError::bad("invalid_request", "Malformed query."))?;
    if let Some(unknown) = params.keys().find(|k| !allowed.contains(&k.as_str())) {
        return Err(ApiError::bad(
            "invalid_request",
            format!("Unknown parameter {unknown}."),
        ));
    }
    Ok(params)
}

type BodyBytes = Result<Bytes, axum::extract::rejection::BytesRejection>;

/// The raw body, or the JSON error for one over `BODY_LIMIT`.
fn body_bytes(body: BodyBytes) -> Result<Bytes, ApiError> {
    body.map_err(|rejection| {
        if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
            ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "payload_too_large",
                format!("The body is limited to {BODY_LIMIT} bytes."),
            )
        } else {
            ApiError::bad("invalid_request", "Unreadable body.")
        }
    })
}

/// A JSON body: `Content-Type: application/json`, one object. An empty body
/// reads as `{}` when `empty_ok`.
fn json_body<T: serde::de::DeserializeOwned>(
    headers: &HeaderMap,
    body: &Bytes,
    empty_ok: bool,
) -> Result<T, ApiError> {
    if empty_ok && body.is_empty() {
        return serde_json::from_str("{}")
            .map_err(|e| ApiError::bad("invalid_json", e.to_string()));
    }
    let content_type = headers
        .get("content-type")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if !content_type
        .split(';')
        .next()
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("application/json"))
    {
        return Err(ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            "Send the body as application/json.",
        ));
    }
    serde_json::from_slice(body).map_err(|e| ApiError::bad("invalid_json", e.to_string()))
}

type ApiPath = Result<Path<String>, axum::extract::rejection::PathRejection>;

/// The `{id}` of the path: a malformed one (bad percent-encoding) answers
/// the JSON 404 rather than axum's plain-text rejection.
fn path_id(path: ApiPath) -> Result<String, ApiError> {
    let Path(id) = path.map_err(|_| ApiError::not_found())?;
    check_id(&id)?;
    Ok(id)
}

/// Ids are UUIDs; anything long is not one of ours.
fn check_id(id: &str) -> Result<(), ApiError> {
    if id.is_empty() || id.len() > 64 {
        return Err(ApiError::not_found());
    }
    Ok(())
}

fn parse_timezone(value: &str) -> Result<Tz, ApiError> {
    value.parse::<Tz>().map_err(|_| {
        ApiError::bad(
            "invalid_timezone",
            "timezone must be an IANA name such as Europe/Paris.",
        )
    })
}

/// A stored instant (`created_at`, `datetime('now')` in UTC) as RFC 3339.
fn utc_text(value: &str) -> Option<String> {
    crate::booking_time::naive(value).map(|t| t.and_utc().format("%Y-%m-%dT%H:%M:%SZ").to_string())
}

// --- Event types ---

/// What `caller` may do with event type `id`: manage it (owner or team
/// admin), only see it (an internal type the directory offers them), or
/// nothing (`None`, answered as not found). Disabled types are not found.
async fn event_type_access(
    pool: &SqlitePool,
    user_id: &str,
    id: &str,
) -> Result<Option<bool>, ApiError> {
    let row: Option<(String, bool)> = sqlx::query_as(&format!(
        "SELECT et.visibility, {MANAGES_EVENT_TYPE}
         FROM event_types et LEFT JOIN accounts a ON a.id = et.account_id
         WHERE et.id = ? AND et.enabled = 1"
    ))
    .bind(user_id)
    .bind(user_id)
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(match row {
        Some((_, true)) => Some(true),
        Some((visibility, false)) if visibility == "internal" => {
            is_listed(pool, user_id, id).await?.then_some(false)
        }
        _ => None,
    })
}

async fn list_event_types(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    query: QueryParams,
) -> ApiResult {
    let caller = authenticate(&state, &headers, "event-types:read").await?;
    let params = query_params(query, &["cursor"])?;
    let cursor = params.get("cursor").map(String::as_str);
    if cursor.is_some_and(|c| c.is_empty() || c.len() > 64) {
        return Err(ApiError::bad("invalid_cursor", "Invalid cursor."));
    }
    let (listed, next) = distributable_event_types(&state.pool, &caller.user.id, cursor).await?;
    let items: Vec<Value> = listed
        .into_iter()
        .map(|e| {
            json!({
                "id": e.id, "slug": e.slug, "title": e.title, "duration_min": e.duration_min,
                "visibility": e.visibility, "team_id": e.team_id, "manageable": e.manages,
            })
        })
        .collect();
    Ok(ok(
        StatusCode::OK,
        json!({"event_types": items, "next_cursor": next}),
    ))
}

async fn get_event_type(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    path: ApiPath,
) -> ApiResult {
    let caller = authenticate(&state, &headers, "event-types:read").await?;
    let id = path_id(path)?;
    let Some(manages) = event_type_access(&state.pool, &caller.user.id, &id).await? else {
        return Err(ApiError::not_found());
    };
    #[allow(clippy::type_complexity)]
    let row: (
        String,
        String,
        Option<String>,
        i32,
        String,
        Option<String>,
        i32,
        String,
        String,
        i32,
        String,
    ) = sqlx::query_as(
        "SELECT et.slug, et.title, et.description, et.duration_min, et.visibility, et.team_id,
                et.requires_confirmation, et.location_type, et.scheduling_mode,
                et.max_additional_guests, et.sms_phone_mode
         FROM event_types et WHERE et.id = ?",
    )
    .bind(&id)
    .fetch_one(&state.pool)
    .await?;
    let (
        slug,
        title,
        description,
        duration,
        visibility,
        team_id,
        requires_confirmation,
        location_type,
        scheduling_mode,
        max_guests,
        phone_mode,
    ) = row;
    let timezone = get_host_tz(&state.pool, &id).await;
    Ok(ok(
        StatusCode::OK,
        json!({"event_type": {
            "id": id, "slug": slug, "title": title, "description": description,
            "duration_min": duration, "visibility": visibility, "team_id": team_id,
            "manageable": manages, "requires_confirmation": requires_confirmation != 0,
            "location_type": location_type, "timezone": timezone.name(),
            "scheduling_mode": team_id.as_ref().map(|_| scheduling_mode),
            "max_additional_guests": max_guests, "phone": phone_mode,
        }}),
    ))
}

// --- Slots ---

async fn event_type_slots(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    path: ApiPath,
    query: QueryParams,
) -> ApiResult {
    let caller = authenticate(&state, &headers, "slots:read").await?;
    let id = path_id(path)?;
    let params = query_params(query, &["from", "to", "timezone"])?;
    let date = |name: &str| -> Result<NaiveDate, ApiError> {
        let value = params
            .get(name)
            .ok_or_else(|| ApiError::bad("invalid_request", format!("{name} is required.")))?;
        NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| {
            ApiError::bad(
                "invalid_request",
                format!("{name} must be a YYYY-MM-DD date."),
            )
        })
    };
    let (from, to) = (date("from")?, date("to")?);
    if to < from || (to - from).num_days() >= MAX_SLOT_DAYS {
        return Err(ApiError::bad(
            "invalid_range",
            format!("to must be on or after from, at most {MAX_SLOT_DAYS} days in all."),
        ));
    }
    match event_type_access(&state.pool, &caller.user.id, &id).await? {
        Some(true) => {}
        Some(false) => {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "forbidden",
                "Only the owner or a team admin of this event type can read its slots.",
            ))
        }
        None => return Err(ApiError::not_found()),
    }
    heavy_limit(&state, &caller).await?;
    let host_tz = get_host_tz(&state.pool, &id).await;
    let guest_tz = match params.get("timezone") {
        Some(tz) => parse_timezone(tz)?,
        None => host_tz,
    };
    let today = Utc::now().with_timezone(&guest_tz).date_naive();
    if from < today - Duration::days(1) || from > today + Duration::days(MAX_DAYS_AHEAD + 1) {
        return Err(ApiError::bad(
            "invalid_range",
            "from must be between yesterday and a year ahead.",
        ));
    }
    let slots = api_slots(&state, &id, host_tz, guest_tz, from, to).await?;
    Ok(ok(
        StatusCode::OK,
        json!({"event_type_id": id, "timezone": guest_tz.name(),
               "from": from.to_string(), "to": to.to_string(), "slots": slots}),
    ))
}

/// The slots of event type `id` whose guest-local day is in `from..=to`,
/// computed as its booking page computes them: the same calendar sync, busy
/// source (the owner, or the team's members by scheduling mode) and
/// `compute_slots`, over a window padded by a day each side.
async fn api_slots(
    state: &AppState,
    id: &str,
    host_tz: Tz,
    guest_tz: Tz,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<Value>, ApiError> {
    let et: (i32, i32, i32, i32, Option<String>, String, String) = sqlx::query_as(
        "SELECT et.duration_min, et.buffer_before, et.buffer_after, et.min_notice_min, et.team_id,
                et.scheduling_mode, COALESCE(a.user_id, '')
         FROM event_types et LEFT JOIN accounts a ON a.id = et.account_id WHERE et.id = ?",
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await?;
    let (duration, buffer_before, buffer_after, min_notice, team_id, mode, owner) = et;
    let now_host = Utc::now().with_timezone(&host_tz).naive_local();
    let host_today = now_host.date();
    let start_offset = ((from - host_today).num_days() - 1).max(0) as i32;
    let end_offset = (to - host_today).num_days() as i32 + 2;
    if end_offset <= 0 {
        return Ok(Vec::new());
    }
    let days_ahead = (end_offset - start_offset).max(1);
    let end_date = host_today + Duration::days((start_offset + days_ahead) as i64);
    let window_end = end_date.and_hms_opt(23, 59, 59).unwrap_or(now_host);
    let busy = match &team_id {
        Some(team_id) => {
            let mut sync_tasks = tokio::task::JoinSet::new();
            for uid in team_member_ids(&state.pool, id, team_id).await {
                let pool = state.pool.clone();
                let key = state.secret_key;
                sync_tasks.spawn(async move {
                    crate::commands::sync::sync_if_stale(&pool, &key, &uid).await;
                });
            }
            while sync_tasks.join_next().await.is_some() {}
            team_busy_source(
                &state.pool,
                id,
                team_id,
                mode == "collective",
                now_host,
                window_end,
                host_tz,
            )
            .await
        }
        None => {
            crate::commands::sync::sync_if_stale(&state.pool, &state.secret_key, &owner).await;
            BusySource::Individual(
                fetch_busy_times_for_user(
                    &state.pool,
                    &owner,
                    now_host,
                    window_end,
                    host_tz,
                    Some(id),
                )
                .await,
            )
        }
    };
    let days = compute_slots(
        &state.pool,
        id,
        duration,
        buffer_before,
        buffer_after,
        min_notice,
        start_offset,
        days_ahead,
        get_booking_window(&state.pool, id).await,
        host_tz,
        guest_tz,
        busy,
        None,
    )
    .await;
    let mut slots = Vec::new();
    for slot in days.iter().flat_map(|d| &d.slots) {
        let Ok(local) = NaiveDateTime::parse_from_str(
            &format!("{} {}", slot.host_date, slot.host_time),
            "%Y-%m-%d %H:%M",
        ) else {
            continue;
        };
        // The engine only offers wall times that name one instant.
        let Some(start) = host_tz.from_local_datetime(&local).single() else {
            continue;
        };
        let start = start.with_timezone(&Utc);
        let guest_start = start.with_timezone(&guest_tz);
        if guest_start.date_naive() < from || guest_start.date_naive() > to {
            continue;
        }
        let end = start + Duration::minutes(duration as i64);
        slots.push(json!({
            "start": start.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            "end": end.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            "date": guest_start.format("%Y-%m-%d").to_string(),
            "time": guest_start.format("%H:%M").to_string(),
        }));
    }
    Ok(slots)
}

// --- Bookings ---

/// The columns `booking_json` reads. Aliases `b`, `et`, `a` (LEFT JOIN of the
/// event type's account) and `u` (LEFT JOIN of its owner).
const BOOKING_COLUMNS: &str = "SELECT b.id, b.uid, b.status,
        CASE b.time_version WHEN 1 THEN b.start_at ELSE rtrim(b.start_at, 'Z') END AS start_at,
        CASE b.time_version WHEN 1 THEN b.end_at ELSE rtrim(b.end_at, 'Z') END AS end_at,
        b.created_at, b.guest_name, b.guest_email, b.guest_phone,
        COALESCE(NULLIF(b.guest_timezone, ''), 'UTC') AS guest_tz, b.notes, b.language,
        b.host_notes, et.id AS et_id, et.slug AS et_slug, et.title AS et_title, et.team_id,
        et.location_type, COALESCE(NULLIF(b.meeting_url, ''), et.location_value) AS location_value,
        COALESCE(NULLIF(et.timezone, ''), u.timezone, 'UTC') AS stored_tz,
        (SELECT json_group_array(ba.email) FROM booking_attendees ba WHERE ba.booking_id = b.id) AS attendees,
        b.start_at AS raw_start
     FROM bookings b
     JOIN event_types et ON et.id = b.event_type_id
     LEFT JOIN accounts a ON a.id = et.account_id
     LEFT JOIN users u ON u.id = a.user_id";

#[derive(sqlx::FromRow)]
struct BookingRow {
    id: String,
    uid: String,
    status: String,
    start_at: String,
    end_at: String,
    created_at: String,
    guest_name: String,
    guest_email: String,
    guest_phone: Option<String>,
    guest_tz: String,
    notes: Option<String>,
    language: Option<String>,
    host_notes: Option<String>,
    et_id: String,
    et_slug: String,
    et_title: String,
    team_id: Option<String>,
    location_type: String,
    location_value: Option<String>,
    stored_tz: String,
    attendees: Option<String>,
    /// The stored `start_at`, which the list's cursor pages on.
    raw_start: String,
}

/// A booking as the API shows it to one of its hosts. Never its cancel,
/// reschedule or approval tokens, nor anything about calendars.
fn booking_json(row: BookingRow, with_host_notes: bool) -> Value {
    let stored_tz: Tz = row.stored_tz.parse().unwrap_or(Tz::UTC);
    let instant = |value: &str| {
        crate::booking_time::local(value, stored_tz, Tz::UTC)
            .map(|t| t.and_utc().format("%Y-%m-%dT%H:%M:%SZ").to_string())
    };
    let attendees: Vec<String> = row
        .attendees
        .and_then(|a| serde_json::from_str(&a).ok())
        .unwrap_or_default();
    let host_notes = if with_host_notes {
        row.host_notes
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
    } else {
        None
    };
    json!({
        "id": row.id, "uid": row.uid, "status": row.status,
        "start": instant(&row.start_at), "end": instant(&row.end_at),
        "created_at": utc_text(&row.created_at),
        "event_type": {"id": row.et_id, "slug": row.et_slug, "title": row.et_title, "team_id": row.team_id},
        "guest": {"name": row.guest_name, "email": row.guest_email, "phone": row.guest_phone,
                  "timezone": row.guest_tz, "language": row.language, "notes": row.notes},
        "attendees": attendees,
        "location": {"type": row.location_type, "value": row.location_value},
        "host_notes": host_notes,
    })
}

/// Booking `id` when `user_id` hosts it (the host-notes rule, `NOTE_HOSTS`).
async fn hosted_booking(
    pool: &SqlitePool,
    user_id: &str,
    id: &str,
) -> Result<Option<Value>, sqlx::Error> {
    let row: Option<BookingRow> = sqlx::query_as(&format!(
        "{BOOKING_COLUMNS} WHERE b.id = ? AND {NOTE_HOSTS}"
    ))
    .bind(id)
    .bind(user_id)
    .bind(user_id)
    .bind(user_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| booking_json(r, true)))
}

fn encode_cursor(start_at: &str, id: &str) -> String {
    base64::Engine::encode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        format!("{start_at}\n{id}"),
    )
}

fn decode_cursor(cursor: &str) -> Option<(String, String)> {
    if cursor.len() > 256 {
        return None;
    }
    let bytes =
        base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, cursor).ok()?;
    let text = String::from_utf8(bytes).ok()?;
    let (start, id) = text.split_once('\n')?;
    Some((start.to_string(), id.to_string()))
}

async fn list_bookings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    query: QueryParams,
) -> ApiResult {
    let caller = authenticate(&state, &headers, "bookings:read").await?;
    let params = query_params(query, &["cursor", "status", "from", "to"])?;
    let status = params.get("status").map(String::as_str);
    if status.is_some_and(|s| !matches!(s, "pending" | "confirmed" | "cancelled" | "declined")) {
        return Err(ApiError::bad(
            "invalid_request",
            "status must be pending, confirmed, cancelled or declined.",
        ));
    }
    let instant = |name: &str| -> Result<Option<chrono::DateTime<Utc>>, ApiError> {
        params
            .get(name)
            .map(|v| {
                chrono::DateTime::parse_from_rfc3339(v)
                    .map(|t| t.with_timezone(&Utc))
                    .map_err(|_| {
                        ApiError::bad("invalid_request", format!("{name} must be RFC 3339."))
                    })
            })
            .transpose()
    };
    let (from, to) = (instant("from")?, instant("to")?);
    let (after_start, after_id) = match params.get("cursor") {
        Some(c) => {
            decode_cursor(c).ok_or_else(|| ApiError::bad("invalid_cursor", "Invalid cursor."))?
        }
        None => (String::new(), String::new()),
    };
    // Stored times are UTC instants, or wall clocks in the event type's zone
    // for bookings older than 1.20: the SQL bounds are widened by a day and
    // the exact filter runs on the converted instants below.
    let lower = from.map(|t| {
        (t - Duration::days(1))
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string()
    });
    let upper = to.map(|t| {
        (t + Duration::days(1))
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string()
    });
    let rows: Vec<BookingRow> = sqlx::query_as(&format!(
        "{BOOKING_COLUMNS} WHERE {NOTE_HOSTS}
           AND (b.start_at > ? OR (b.start_at = ? AND b.id > ?))
           AND (? IS NULL OR b.status = ?)
           AND (? IS NULL OR b.start_at >= ?)
           AND (? IS NULL OR b.start_at < ?)
         ORDER BY b.start_at, b.id LIMIT {}",
        PAGE + 1
    ))
    .bind(&caller.user.id)
    .bind(&caller.user.id)
    .bind(&caller.user.id)
    .bind(&caller.user.id)
    .bind(&after_start)
    .bind(&after_start)
    .bind(&after_id)
    .bind(status)
    .bind(status)
    .bind(&lower)
    .bind(&lower)
    .bind(&upper)
    .bind(&upper)
    .fetch_all(&state.pool)
    .await?;
    let next =
        (rows.len() > PAGE).then(|| encode_cursor(&rows[PAGE - 1].raw_start, &rows[PAGE - 1].id));
    let mut bookings = Vec::new();
    for row in rows.into_iter().take(PAGE) {
        let value = booking_json(row, true);
        let start = value["start"]
            .as_str()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|t| t.with_timezone(&Utc));
        let inside = match start {
            Some(start) => from.is_none_or(|f| start >= f) && to.is_none_or(|t| start < t),
            None => from.is_none() && to.is_none(),
        };
        if inside {
            bookings.push(value);
        }
    }
    Ok(ok(
        StatusCode::OK,
        json!({"bookings": bookings, "next_cursor": next}),
    ))
}

async fn get_booking(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    path: ApiPath,
) -> ApiResult {
    let caller = authenticate(&state, &headers, "bookings:read").await?;
    let id = path_id(path)?;
    match hosted_booking(&state.pool, &caller.user.id, &id).await? {
        Some(booking) => Ok(ok(StatusCode::OK, json!({ "booking": booking }))),
        None => Err(ApiError::not_found()),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateBooking {
    event_type_id: String,
    /// RFC 3339 instant.
    start: String,
    /// IANA zone the guest books in; the event type's by default.
    timezone: Option<String>,
    name: String,
    email: String,
    notes: Option<String>,
    phone: Option<String>,
    #[serde(default)]
    attendees: Vec<String>,
    /// Language of the guest's emails and SMS.
    language: Option<String>,
}

async fn create_booking(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: BodyBytes,
) -> ApiResult {
    let caller = authenticate(&state, &headers, "bookings:write").await?;
    heavy_limit(&state, &caller).await?;
    let body = body_bytes(body)?;
    let request: CreateBooking = json_body(&headers, &body, false)?;
    let key = idempotency_key(&headers)?;
    let Some(key) = key else {
        return create_booking_inner(&state, &caller, request)
            .await
            .map(|(status, value)| ok(status, caller.booking_answer_of(value)));
    };
    match idempotency_begin(&state.pool, &caller.user.id, key, &body).await? {
        Idempotency::Replay(status, stored) => {
            // The stored answer is only handed back to someone who may still
            // book this event type, and in the shape this token may read.
            check_id(&request.event_type_id)?;
            if event_type_access(&state.pool, &caller.user.id, &request.event_type_id).await?
                != Some(true)
            {
                return Err(ApiError::not_found());
            }
            let mut response = ok(status, caller.booking_answer_of(stored));
            response
                .headers_mut()
                .insert("idempotency-replayed", "true".parse().unwrap());
            Ok(response)
        }
        Idempotency::Proceed => {
            // Keep the key's lease alive for as long as the booking runs,
            // and no longer: the guard stops the renewals when this future
            // ends or is dropped (a client gone, a panic).
            let lease = spawn_lease(
                state.pool.clone(),
                caller.user.id.clone(),
                key.to_string(),
                LEASE_RENEWAL,
            );
            let result = create_booking_inner(&state, &caller, request).await;
            drop(lease);
            // The full answer is kept; this token gets what its scopes allow.
            let (status, value) =
                idempotency_finish(&state.pool, &caller.user.id, key, result).await;
            Ok(ok(status, caller.booking_answer_of(value)))
        }
    }
}

/// Map a refusal of the booking cores to the API's error.
fn refusal_error(refusal: BookingRefusal) -> ApiError {
    match refusal {
        BookingRefusal::Plain(key) => match key {
            "error-slot-unavailable" | "error-no-members-available" => {
                ApiError::new(StatusCode::CONFLICT, "slot_unavailable", en(key))
            }
            "error-slot-too-soon" => {
                ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, "too_soon", en(key))
            }
            "error-invalid-date" | "error-invalid-time" => ApiError::bad("invalid_start", en(key)),
            _ => ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, "booking_refused", en(key)),
        },
        BookingRefusal::Keys("bae-title-unavailable", body) => {
            ApiError::new(StatusCode::CONFLICT, "limit_reached", en(body))
        }
        BookingRefusal::Keys(_, body) => ApiError::bad("invalid_request", en(body)),
        BookingRefusal::Message(_, message) => ApiError::bad("invalid_request", message),
        BookingRefusal::Html(_) => ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "outside_booking_window",
            "This time is outside the event type's booking window.",
        ),
        BookingRefusal::Internal(context, error) => ApiError::internal(context, &error),
    }
}

async fn create_booking_inner(
    state: &AppState,
    caller: &Caller,
    request: CreateBooking,
) -> Result<(StatusCode, Value), ApiError> {
    check_id(&request.event_type_id)?;
    let user = &caller.user;
    match event_type_access(&state.pool, &user.id, &request.event_type_id).await? {
        Some(true) => {}
        Some(false) => {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "forbidden",
                "Only the owner or a team admin of this event type can book it for a guest.",
            ))
        }
        None => return Err(ApiError::not_found()),
    }
    if let Err(key) = validate_booking_input(&request.name, &request.email, &request.notes) {
        return Err(ApiError::bad("invalid_request", en(key)));
    }
    let lang = match request.language.as_deref() {
        Some(code) => crate::i18n::supported_code(code)
            .ok_or_else(|| ApiError::bad("invalid_request", "Unsupported language."))?,
        None => user
            .language
            .as_deref()
            .and_then(crate::i18n::supported_code)
            .unwrap_or("en"),
    };
    // The instance addresses German speakers as "Sie": the guest's emails
    // follow, as they would from the booking page (`detect_from_headers`).
    let lang = if lang == "de"
        && state
            .german_formal
            .load(std::sync::atomic::Ordering::Relaxed)
    {
        "de-formal"
    } else {
        lang
    };
    let start = chrono::DateTime::parse_from_rfc3339(&request.start)
        .map_err(|_| ApiError::bad("invalid_start", "start must be an RFC 3339 instant."))?
        .with_timezone(&Utc);
    let host_tz = get_host_tz(&state.pool, &request.event_type_id).await;
    let guest_tz = match request.timezone.as_deref() {
        Some(tz) => parse_timezone(tz)?,
        None => host_tz,
    };
    let local = start.with_timezone(&guest_tz).naive_local();
    if local.second() != 0 || start.timestamp_subsec_nanos() != 0 {
        return Err(ApiError::bad(
            "invalid_start",
            "start must fall on a whole minute.",
        ));
    }
    if request.attendees.len() > API_MAX_ATTENDEES {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "too_many_attendees",
            format!(
                "At most {API_MAX_ATTENDEES} additional attendees per booking through the API."
            ),
        ));
    }
    if request
        .attendees
        .iter()
        .any(|a| a.contains(',') || a.len() > 255)
    {
        return Err(ApiError::bad("invalid_request", "Invalid attendees."));
    }
    let date = local.format("%Y-%m-%d").to_string();
    let time = local.format("%H:%M").to_string();
    let additional_guests = (!request.attendees.is_empty()).then(|| request.attendees.join(","));
    let guest = GuestRequest {
        name: &request.name,
        email: &request.email,
        notes: &request.notes,
        phone: request.phone.as_deref(),
        additional_guests: &additional_guests,
        date: &date,
        time: &time,
        // A manager booking for a guest: no booking questions (#210).
        fields: None,
    };
    let resolve_tz = move |_: Tz| guest_tz;
    let team: Option<Option<String>> =
        sqlx::query_scalar("SELECT team_id FROM event_types WHERE id = ?")
            .bind(&request.event_type_id)
            .fetch_optional(&state.pool)
            .await?;
    let reservation: String;
    let booked = match team.flatten() {
        Some(_) => {
            let et: Option<TeamEventType> = sqlx::query_as(&format!(
                "{TEAM_BOOKING_COLUMNS} WHERE et.id = ? AND et.enabled = 1"
            ))
            .bind(&request.event_type_id)
            .fetch_optional(&state.pool)
            .await?;
            let et = et.ok_or_else(ApiError::not_found)?;
            let phone_mode = stored_sms_phone_mode(&state.pool, &et.id).await;
            precheck_guest(state, &phone_mode, &guest, et.max_additional_guests).await?;
            let slug = et.slug.clone();
            reservation = reserve_api_booking(&state.pool, &user.id).await?;
            book_team(
                state,
                et,
                &slug,
                &guest,
                BookingAccess::Manager,
                &resolve_tz,
                lang,
            )
            .await
        }
        None => {
            let et: Option<PersonalEventType> = sqlx::query_as(&format!(
                "{PERSONAL_BOOKING_COLUMNS} WHERE et.id = ? AND et.team_id IS NULL AND et.enabled = 1 AND u.enabled = 1"
            ))
            .bind(&request.event_type_id)
            .fetch_optional(&state.pool)
            .await?;
            let et = et.ok_or_else(ApiError::not_found)?;
            precheck_guest(state, &et.sms_phone_mode, &guest, et.max_additional_guests).await?;
            let slug: String = sqlx::query_scalar("SELECT slug FROM event_types WHERE id = ?")
                .bind(&request.event_type_id)
                .fetch_one(&state.pool)
                .await?;
            reservation = reserve_api_booking(&state.pool, &user.id).await?;
            book_personal(
                state,
                et,
                &slug,
                &guest,
                BookingAccess::Manager,
                &resolve_tz,
                lang,
            )
            .await
        }
    };
    let booked = match booked {
        Ok(booked) => booked,
        Err(refusal) => {
            // A refused booking sent nothing: its place under the cap is freed.
            sqlx::query("DELETE FROM api_booking_log WHERE booking_id = ?")
                .bind(&reservation)
                .execute(&state.pool)
                .await?;
            return Err(refusal_error(refusal));
        }
    };
    sqlx::query("UPDATE api_booking_log SET booking_id = ? WHERE booking_id = ?")
        .bind(&booked.id)
        .bind(&reservation)
        .execute(&state.pool)
        .await?;
    tracing::info!(booking_id = %booked.id, user = %user.email, credential = %caller.credential, "booking created through the API");
    let row: BookingRow = sqlx::query_as(&format!("{BOOKING_COLUMNS} WHERE b.id = ?"))
        .bind(&booked.id)
        .fetch_one(&state.pool)
        .await?;
    Ok((
        StatusCode::CREATED,
        json!({"booking": booking_json(row, false)}),
    ))
}

/// Take one of `user_id`'s `API_DAILY_BOOKINGS` places for the next 24 hours,
/// before the booking is made: every booking emails the guest, the attendees
/// and the hosts, so a token cannot make the instance's SMTP send without
/// bound. Count and insert are one statement, so concurrent requests at the
/// cap cannot all pass, and the row is written before the booking runs, so
/// a client that disconnects still counts. Returns the reservation id,
/// replaced by the booking's id once it is made.
pub(super) async fn reserve_api_booking(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<String, ApiError> {
    let reservation = format!("reserved:{}", uuid::Uuid::new_v4());
    let now = Utc::now().timestamp();
    let inserted = sqlx::query(
        "INSERT INTO api_booking_log (user_id, booking_id, created_at)
         SELECT ?, ?, ? WHERE (SELECT COUNT(*) FROM api_booking_log WHERE user_id = ? AND created_at > ?) < ?",
    )
    .bind(user_id)
    .bind(&reservation)
    .bind(now)
    .bind(user_id)
    .bind(now - 86400)
    .bind(API_DAILY_BOOKINGS)
    .execute(pool)
    .await?;
    if inserted.rows_affected() == 0 {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "daily_limit_reached",
            format!("At most {API_DAILY_BOOKINGS} bookings per user per 24 hours through the API."),
        ));
    }
    Ok(reservation)
}

/// The phone number and the additional attendees, checked as the booking
/// cores check them but with English messages, so the API's errors do not
/// depend on the guest's language. The cores check them again.
async fn precheck_guest(
    state: &AppState,
    phone_mode: &str,
    guest: &GuestRequest<'_>,
    max_additional_guests: i32,
) -> Result<(), ApiError> {
    let country = crate::sms::default_country_code(&state.pool).await;
    if let Err((_, message)) = resolve_guest_phone(phone_mode, guest.phone, &country, "en") {
        return Err(ApiError::bad("invalid_phone", message));
    }
    if let Err(message) = parse_additional_guests(
        guest.additional_guests,
        max_additional_guests,
        guest.email,
        "en",
    ) {
        return Err(ApiError::bad("invalid_attendees", message));
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CancelBody {
    reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RescheduleBody {
    /// Email the guest the pending request again instead of making one.
    #[serde(default)]
    resend: bool,
}

/// Why a write on booking `id` found nothing to do. A booking of the
/// caller's own event types that is no longer active is a 409. With
/// `bookings:read`, a booking the caller hosts without owning it is a 403
/// (or a 409 once ended), as they can read it anyway. Everything else, and
/// everything for a write-only token, is the same 404, so a write-only token
/// learns nothing about bookings it cannot change.
async fn write_refusal(pool: &SqlitePool, caller: &Caller, id: &str, action: &str) -> ApiError {
    let owned: Result<Option<String>, sqlx::Error> = sqlx::query_scalar(
        "SELECT b.status FROM bookings b JOIN event_types et ON et.id = b.event_type_id
         JOIN accounts a ON a.id = et.account_id WHERE b.id = ? AND a.user_id = ?",
    )
    .bind(id)
    .bind(&caller.user.id)
    .fetch_optional(pool)
    .await;
    let not_active = || {
        ApiError::new(
            StatusCode::CONFLICT,
            "booking_not_active",
            "This booking is no longer confirmed or pending.",
        )
    };
    match owned {
        Ok(Some(_)) => return not_active(),
        Ok(None) => {}
        Err(e) => return e.into(),
    }
    if !caller.has("bookings:read") {
        return ApiError::not_found();
    }
    match hosted_booking(pool, &caller.user.id, id).await {
        Ok(Some(booking))
            if matches!(booking["status"].as_str(), Some("confirmed" | "pending")) =>
        {
            ApiError::new(
                StatusCode::FORBIDDEN,
                "forbidden",
                format!("Only the owner of the event type can {action} this booking."),
            )
        }
        Ok(Some(_)) => not_active(),
        Ok(None) => ApiError::not_found(),
        Err(e) => e.into(),
    }
}

async fn cancel_booking_api(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    path: ApiPath,
    body: BodyBytes,
) -> ApiResult {
    let caller = authenticate(&state, &headers, "bookings:write").await?;
    let id = path_id(path)?;
    heavy_limit(&state, &caller).await?;
    let body = body_bytes(body)?;
    let request: CancelBody = json_body(&headers, &body, true)?;
    if request.reason.as_ref().is_some_and(|r| r.len() > 5000) {
        return Err(ApiError::bad(
            "invalid_request",
            en("validate-notes-length"),
        ));
    }
    match host_cancel_booking(&state, &caller.user, &id, request.reason).await {
        HostCancel::Done(status) => {
            tracing::info!(booking_id = %id, status, credential = %caller.credential, "booking ended through the API");
            let booking = hosted_booking(&state.pool, &caller.user.id, &id).await?;
            Ok(ok(
                StatusCode::OK,
                caller.booking_answer(booking.unwrap_or(Value::Null)),
            ))
        }
        HostCancel::Unchanged => Err(ApiError::new(
            StatusCode::CONFLICT,
            "booking_not_active",
            "This booking changed state meanwhile.",
        )),
        HostCancel::NotFound => Err(write_refusal(&state.pool, &caller, &id, "cancel").await),
    }
}

async fn request_reschedule_api(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    path: ApiPath,
    body: BodyBytes,
) -> ApiResult {
    let caller = authenticate(&state, &headers, "bookings:write").await?;
    let id = path_id(path)?;
    heavy_limit(&state, &caller).await?;
    let body = body_bytes(body)?;
    let request: RescheduleBody = json_body(&headers, &body, true)?;
    match host_request_reschedule(&state, &caller.user, &id, request.resend).await {
        HostReschedule::Requested => {
            tracing::info!(booking_id = %id, credential = %caller.credential, "reschedule requested through the API");
            let booking = hosted_booking(&state.pool, &caller.user.id, &id).await?;
            Ok(ok(
                StatusCode::OK,
                caller.booking_answer(booking.unwrap_or(Value::Null)),
            ))
        }
        HostReschedule::AlreadyRequested => Err(ApiError::new(
            StatusCode::CONFLICT,
            "reschedule_already_requested",
            en("bookings-error-reschedule-pending"),
        )),
        HostReschedule::ResendTooSoon => Err(ApiError::new(
            StatusCode::CONFLICT,
            "resend_too_soon",
            en("bookings-error-reschedule-resend-too-soon"),
        )),
        HostReschedule::NotRequested => Err(ApiError::new(
            StatusCode::CONFLICT,
            "no_reschedule_pending",
            "No reschedule request is waiting for the guest.",
        )),
        HostReschedule::NoBookableDates => Err(ApiError::new(
            StatusCode::CONFLICT,
            "no_bookable_dates",
            en("bookings-error-no-bookable-dates"),
        )),
        HostReschedule::NotFound => {
            Err(write_refusal(&state.pool, &caller, &id, "reschedule").await)
        }
    }
}

// --- Idempotency-Key ---

/// How often a running request renews its key's lease.
const LEASE_RENEWAL: std::time::Duration = std::time::Duration::from_secs(30);

/// Aborts the lease renewal when dropped, whichever way the request ends.
pub(super) struct LeaseGuard(tokio::task::JoinHandle<()>);

impl Drop for LeaseGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Renew the lease of `user`'s idempotency `key` every `every` while the
/// first request with it runs.
pub(super) fn spawn_lease(
    pool: SqlitePool,
    user: String,
    key: String,
    every: std::time::Duration,
) -> LeaseGuard {
    LeaseGuard(tokio::spawn(async move {
        loop {
            tokio::time::sleep(every).await;
            let _ = sqlx::query(
                "UPDATE api_idempotency SET lease_at = ? WHERE user_id = ? AND key = ? AND response IS NULL",
            )
            .bind(Utc::now().timestamp())
            .bind(&user)
            .bind(&key)
            .execute(&pool)
            .await;
        }
    }))
}

enum Idempotency {
    /// First time this key is seen: do the work, then `idempotency_finish`.
    Proceed,
    /// The stored answer of the first request: its status and body.
    Replay(StatusCode, Value),
}

/// The `Idempotency-Key` header: 1 to 255 visible ASCII characters.
fn idempotency_key(headers: &HeaderMap) -> Result<Option<&str>, ApiError> {
    let Some(value) = headers.get("idempotency-key") else {
        return Ok(None);
    };
    match value.to_str() {
        Ok(key)
            if !key.is_empty() && key.len() <= 255 && key.bytes().all(|b| b.is_ascii_graphic()) =>
        {
            Ok(Some(key))
        }
        _ => Err(ApiError::bad(
            "invalid_idempotency_key",
            "Idempotency-Key must be 1 to 255 visible ASCII characters.",
        )),
    }
}

/// Claim `key` for `user_id`. A key is kept 24 hours; one whose first request
/// is still running answers 409, one reused with another body 422.
async fn idempotency_begin(
    pool: &SqlitePool,
    user_id: &str,
    key: &str,
    body: &[u8],
) -> Result<Idempotency, ApiError> {
    let now = Utc::now().timestamp();
    let request_hash = crate::oauth::hash(&String::from_utf8_lossy(body));
    // An expired key, or a first request whose lease stopped being renewed
    // (the process stopped), is forgotten.
    sqlx::query(
        "DELETE FROM api_idempotency WHERE user_id = ? AND key = ?
           AND (created_at <= ? OR (response IS NULL AND lease_at <= ?))",
    )
    .bind(user_id)
    .bind(key)
    .bind(now - 86400)
    .bind(now - 120)
    .execute(pool)
    .await?;
    let inserted = sqlx::query(
        "INSERT OR IGNORE INTO api_idempotency (user_id, key, request_hash, created_at, lease_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(key)
    .bind(&request_hash)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;
    if inserted.rows_affected() == 1 {
        return Ok(Idempotency::Proceed);
    }
    let row: Option<(String, Option<i64>, Option<String>)> = sqlx::query_as(
        "SELECT request_hash, status, response FROM api_idempotency WHERE user_id = ? AND key = ?",
    )
    .bind(user_id)
    .bind(key)
    .fetch_optional(pool)
    .await?;
    let Some((stored_hash, status, response)) = row else {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "idempotency_in_progress",
            "A request with this Idempotency-Key is in progress.",
        ));
    };
    if stored_hash != request_hash {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "idempotency_key_reused",
            "This Idempotency-Key was used with a different request.",
        ));
    }
    match (status, response) {
        (Some(status), Some(body)) => {
            let status = StatusCode::from_u16(status as u16).unwrap_or(StatusCode::OK);
            let body = serde_json::from_str(&body).unwrap_or(Value::Null);
            Ok(Idempotency::Replay(status, body))
        }
        _ => Err(ApiError::new(
            StatusCode::CONFLICT,
            "idempotency_in_progress",
            "A request with this Idempotency-Key is in progress.",
        )),
    }
}

/// Store the answer of the request that claimed `key`, so a retry gets it
/// back. Server errors are not stored: the key is released for a retry.
async fn idempotency_finish(
    pool: &SqlitePool,
    user_id: &str,
    key: &str,
    result: Result<(StatusCode, Value), ApiError>,
) -> (StatusCode, Value) {
    let (status, value) = match result {
        Ok(answer) => answer,
        Err(error) => {
            let status = error.status;
            let value = json!({"error": {"code": error.code, "message": error.message}});
            (status, value)
        }
    };
    let stored = if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
        sqlx::query("DELETE FROM api_idempotency WHERE user_id = ? AND key = ?")
            .bind(user_id)
            .bind(key)
            .execute(pool)
            .await
    } else {
        sqlx::query(
            "UPDATE api_idempotency SET status = ?, response = ? WHERE user_id = ? AND key = ?",
        )
        .bind(status.as_u16() as i64)
        .bind(value.to_string())
        .bind(user_id)
        .bind(key)
        .execute(pool)
        .await
    };
    if let Err(e) = stored {
        tracing::error!(error = %e, "API idempotency record failed");
    }
    (status, value)
}
