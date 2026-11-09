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
use serde::Serialize;
use serde_json::{json, Value};
use utoipa::ToSchema;

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
        .route("/openapi.json", get(super::api_doc::openapi_json))
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed);
    // Only `/api/v1` is claimed: `/api` and `/api/book` stay the legacy
    // booking pages of an event type whose slug is `api`.
    Router::new()
        .nest("/api/v1", v1)
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
}

// --- Errors ---

/// The stable `code` of an error answer. Every refusal of the API names one
/// of these, so the OpenAPI document (`ErrorCode`) lists them all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(super) enum ErrorCode {
    InvalidRequest,
    InvalidJson,
    InvalidStart,
    InvalidTimezone,
    InvalidPhone,
    InvalidAttendees,
    InvalidRange,
    InvalidCursor,
    InvalidIdempotencyKey,
    Unauthorized,
    InsufficientScope,
    Forbidden,
    OriginNotAllowed,
    NotFound,
    MethodNotAllowed,
    SlotUnavailable,
    LimitReached,
    BookingNotActive,
    NoBookableDates,
    RescheduleAlreadyRequested,
    ResendTooSoon,
    NoReschedulePending,
    IdempotencyInProgress,
    PayloadTooLarge,
    UnsupportedMediaType,
    TooSoon,
    OutsideBookingWindow,
    BookingRefused,
    TooManyAttendees,
    IdempotencyKeyReused,
    RateLimited,
    DailyLimitReached,
    InternalError,
}

/// The one error shape: `{"error": {"code", "message"}}`.
#[derive(Serialize, ToSchema)]
pub(super) struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Serialize, ToSchema)]
pub(super) struct ErrorDetail {
    /// Stable: what a client branches on.
    code: ErrorCode,
    /// English text for a person reading the logs; may change.
    message: String,
}

/// An error answer: its status, `code` and `message`.
#[derive(Debug)]
pub(super) struct ApiError {
    status: StatusCode,
    code: ErrorCode,
    message: String,
    /// `WWW-Authenticate` value for 401 and 403.
    challenge: Option<String>,
}

impl ApiError {
    fn new(status: StatusCode, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            challenge: None,
        }
    }
    fn bad(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code, message)
    }
    fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, ErrorCode::NotFound, "Not found.")
    }
    fn internal(context: &str, error: &dyn std::fmt::Display) -> Self {
        tracing::error!(%error, context, "API request failed");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::InternalError,
            "Something went wrong on the server; try again later.",
        )
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        Self::internal("database", &error)
    }
}

impl ApiError {
    /// The answer's body.
    fn body(&self) -> Value {
        to_json(&ErrorBody {
            error: ErrorDetail {
                code: self.code,
                message: self.message.clone(),
            },
        })
    }
}

/// An answer type as JSON. Serializing these plain structs cannot fail.
fn to_json(value: &impl Serialize) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = (self.status, Json(self.body())).into_response();
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
        ErrorCode::MethodNotAllowed,
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
        let booking = if self.has("bookings:read") {
            booking
        } else {
            to_json(&BookingRef {
                id: booking["id"].clone(),
                status: booking["status"].clone(),
            })
        };
        to_json(&WriteAnswer { booking })
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
                ErrorCode::OriginNotAllowed,
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
            ErrorCode::Unauthorized,
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
            ErrorCode::InsufficientScope,
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
        ErrorCode::RateLimited,
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
    let Query(params) =
        query.map_err(|_| ApiError::bad(ErrorCode::InvalidRequest, "Malformed query."))?;
    if let Some(unknown) = params.keys().find(|k| !allowed.contains(&k.as_str())) {
        return Err(ApiError::bad(
            ErrorCode::InvalidRequest,
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
                ErrorCode::PayloadTooLarge,
                format!("The body is limited to {BODY_LIMIT} bytes."),
            )
        } else {
            ApiError::bad(ErrorCode::InvalidRequest, "Unreadable body.")
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
            .map_err(|e| ApiError::bad(ErrorCode::InvalidJson, e.to_string()));
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
            ErrorCode::UnsupportedMediaType,
            "Send the body as application/json.",
        ));
    }
    serde_json::from_slice(body).map_err(|e| ApiError::bad(ErrorCode::InvalidJson, e.to_string()))
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
            ErrorCode::InvalidTimezone,
            "timezone must be an IANA name such as Europe/Paris.",
        )
    })
}

/// A stored instant (`created_at`, `datetime('now')` in UTC) as RFC 3339.
fn utc_text(value: &str) -> Option<String> {
    crate::booking_time::naive(value).map(|t| t.and_utc().format("%Y-%m-%dT%H:%M:%SZ").to_string())
}

// --- Answers ---
//
// The types the handlers serialize, which are also the OpenAPI schemas
// (`api_doc.rs`). Every field is always present, null when it has no value.

/// A string schema limited to `values`, plus null when `nullable`.
fn string_enum(values: &[&str], nullable: bool, description: &str) -> utoipa::openapi::Object {
    let mut values: Vec<Value> = values.iter().map(|v| json!(v)).collect();
    let schema_type = if nullable {
        values.push(Value::Null);
        utoipa::openapi::schema::SchemaType::from_iter([
            utoipa::openapi::Type::String,
            utoipa::openapi::Type::Null,
        ])
    } else {
        utoipa::openapi::schema::SchemaType::new(utoipa::openapi::Type::String)
    };
    utoipa::openapi::ObjectBuilder::new()
        .schema_type(schema_type)
        .enum_values(Some(values))
        .description(Some(description))
        .build()
}

fn visibility_schema() -> utoipa::openapi::Object {
    string_enum(
        &["public", "internal", "private"],
        false,
        "Who can see the event type: listed on profile pages, offered in the booking directory to signed-in users, or reachable by invite only.",
    )
}

fn scheduling_mode_schema() -> utoipa::openapi::Object {
    string_enum(
        &["round_robin", "collective"],
        true,
        "For a team event type: one free member is enough (`round_robin`), or every member must be free (`collective`). Null for a personal event type.",
    )
}

fn phone_mode_schema() -> utoipa::openapi::Object {
    string_enum(
        &["off", "optional", "required"],
        false,
        "Whether a booking asks for the guest's phone number (for SMS notifications).",
    )
}

fn booking_status_schema() -> utoipa::openapi::Object {
    string_enum(
        &["pending", "confirmed", "cancelled", "declined"],
        false,
        "`pending` awaits the host's approval; `declined` is a request the host refused or that expired.",
    )
}

/// An event type in the list.
#[derive(Serialize, ToSchema)]
pub(super) struct EventTypeSummary {
    /// Its id, for the other event type operations and `POST /bookings`.
    id: String,
    /// The last segment of its booking page URL.
    slug: String,
    title: String,
    /// Length of a meeting, in minutes.
    duration_min: i32,
    #[schema(schema_with = visibility_schema)]
    visibility: String,
    /// The team of a team event type; null for a personal one.
    #[schema(required = true)]
    team_id: Option<String>,
    /// True for the event types you manage (you own it, or administer its
    /// team): their slots can be read and they can be booked for a guest.
    /// False for an internal event type the booking directory offers you,
    /// which is listed read-only.
    manageable: bool,
}

/// A page of event types.
#[derive(Serialize, ToSchema)]
pub(super) struct EventTypeList {
    event_types: Vec<EventTypeSummary>,
    /// Pass it back as `cursor` for the next page; null on the last page.
    #[schema(required = true)]
    next_cursor: Option<String>,
}

/// An event type in full.
#[derive(Serialize, ToSchema)]
pub(super) struct EventTypeDetail {
    id: String,
    slug: String,
    title: String,
    #[schema(required = true)]
    description: Option<String>,
    /// Length of a meeting, in minutes.
    duration_min: i32,
    #[schema(schema_with = visibility_schema)]
    visibility: String,
    /// The team of a team event type; null for a personal one.
    #[schema(required = true)]
    team_id: Option<String>,
    /// Whether you manage it (see `EventTypeSummary`).
    manageable: bool,
    /// A booking is a pending request until a host approves it.
    requires_confirmation: bool,
    /// Kind of meeting place: `link`, `jitsi_auto`, `webhook_auto`,
    /// `google_meet`, `phone`, `in_person` or `custom`. Its value (a link,
    /// an address) is not returned here.
    location_type: String,
    /// IANA timezone of its availability, the default of `timezone` in the
    /// slot and booking operations.
    timezone: String,
    #[schema(schema_with = scheduling_mode_schema, required = true)]
    scheduling_mode: Option<String>,
    /// Additional attendees a guest may add on its booking page.
    max_additional_guests: i32,
    #[schema(schema_with = phone_mode_schema)]
    phone: String,
}

#[derive(Serialize, ToSchema)]
pub(super) struct EventTypeAnswer {
    event_type: EventTypeDetail,
}

/// A bookable time.
#[derive(Serialize, ToSchema)]
pub(super) struct Slot {
    /// Start, in UTC. Pass it as `start` to `POST /bookings`.
    #[schema(format = DateTime)]
    start: String,
    /// End, in UTC.
    #[schema(format = DateTime)]
    end: String,
    /// The start's day in the requested timezone.
    #[schema(format = Date)]
    date: String,
    /// The start's wall time in the requested timezone, `HH:MM`.
    #[schema(pattern = r"^\d{2}:\d{2}$")]
    time: String,
}

/// The bookable times of an event type over a range of days.
#[derive(Serialize, ToSchema)]
pub(super) struct SlotList {
    event_type_id: String,
    /// The timezone `date`, `time`, `from` and `to` are in.
    timezone: String,
    #[schema(format = Date)]
    from: String,
    #[schema(format = Date)]
    to: String,
    /// In start order.
    slots: Vec<Slot>,
}

/// The event type a booking is of.
#[derive(Serialize, ToSchema)]
pub(super) struct BookingEventType {
    id: String,
    slug: String,
    title: String,
    #[schema(required = true)]
    team_id: Option<String>,
}

/// The guest who booked.
#[derive(Serialize, ToSchema)]
pub(super) struct BookingGuest {
    name: String,
    email: String,
    /// E.164, when the guest gave one. Null once the event type no longer
    /// asks for a phone number (its `phone` setting is `off`), as in every
    /// email, webhook and calendar copy.
    #[schema(required = true)]
    phone: Option<String>,
    /// IANA timezone of the guest's emails.
    timezone: String,
    /// Language of the guest's emails and SMS.
    #[schema(required = true)]
    language: Option<String>,
    /// What the guest wrote with the booking.
    #[schema(required = true)]
    notes: Option<String>,
}

/// Where the meeting takes place.
#[derive(Serialize, ToSchema)]
pub(super) struct BookingLocation {
    /// As `location_type` of the event type.
    #[serde(rename = "type")]
    kind: String,
    /// The meeting link (the booking's own one when it has one), phone
    /// number or address.
    #[schema(required = true)]
    value: Option<String>,
}

fn answer_kind_schema() -> utoipa::openapi::Object {
    string_enum(
        &[
            "short_text",
            "long_text",
            "single_choice",
            "multiple_choice",
            "checkbox",
        ],
        false,
        "The kind of question.",
    )
}

/// A guest's answer to one of the event type's booking questions, as the
/// guest read it (the same object booking webhooks carry).
#[derive(Serialize, ToSchema)]
pub(super) struct QuestionAnswer {
    /// The question's stable key: it survives a rename of the question.
    key: String,
    /// The question as the guest read it.
    label: String,
    #[schema(schema_with = answer_kind_schema)]
    kind: String,
    /// For display: the text typed, the chosen labels joined by ", " for a
    /// choice, `yes` or `no` for a checkbox. Match choices on `option_ids`.
    value: String,
    /// The chosen options' stable ids; empty for other kinds.
    option_ids: Vec<String>,
    /// A consent checkbox (a policy, a recording).
    consent: bool,
    /// The policy a consent checkbox linked to.
    #[schema(required = true)]
    link_url: Option<String>,
    /// When the guest sent the booking form, in UTC.
    #[schema(format = DateTime)]
    answered_at: String,
}

fn api_answers(answers: Vec<crate::email::BookingAnswer>) -> Vec<QuestionAnswer> {
    answers
        .into_iter()
        .map(|a| QuestionAnswer {
            key: a.key,
            label: a.label,
            kind: a.kind,
            value: a.value,
            option_ids: a.option_ids,
            consent: a.consent,
            link_url: a.link_url,
            answered_at: a.answered_at,
        })
        .collect()
}

/// A booking, as its hosts see it. Never its guest's cancel, reschedule or
/// approval links, nor anything about calendars.
#[derive(Serialize, ToSchema)]
pub(super) struct Booking {
    id: String,
    /// iCalendar UID of the invitation, the one webhooks carry.
    uid: String,
    #[schema(schema_with = booking_status_schema)]
    status: String,
    /// Start, in UTC. Null only when the stored time cannot be read.
    #[schema(format = DateTime, required = true)]
    start: Option<String>,
    /// End, in UTC.
    #[schema(format = DateTime, required = true)]
    end: Option<String>,
    /// When the booking was made, in UTC.
    #[schema(format = DateTime, required = true)]
    created_at: Option<String>,
    event_type: BookingEventType,
    guest: BookingGuest,
    /// The guest's answers to the event type's booking questions, in form
    /// order. Empty when it asks none, and for a booking made through the
    /// API, which asks none.
    answers: Vec<QuestionAnswer>,
    /// Email addresses of the additional attendees.
    attendees: Vec<String>,
    location: BookingLocation,
    /// The hosts' own notes on the booking. Always null in the answer of
    /// `POST /bookings`.
    #[schema(required = true)]
    host_notes: Option<String>,
}

/// A page of bookings.
#[derive(Serialize, ToSchema)]
pub(super) struct BookingList {
    bookings: Vec<Booking>,
    /// Pass it back as `cursor` for the next page; null on the last page.
    #[schema(required = true)]
    next_cursor: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub(super) struct BookingAnswer {
    booking: Booking,
}

/// What a write answers to a token without `bookings:read`.
#[derive(Serialize, ToSchema)]
pub(super) struct BookingRef {
    /// Copied from the booking as read back (null only if it vanished
    /// meanwhile), as the answer always was.
    #[schema(value_type = String)]
    id: Value,
    #[schema(schema_with = booking_status_schema)]
    status: Value,
}

/// `anyOf`, not `oneOf`: OpenAPI 3.1 objects are open, so a whole `Booking`
/// also matches `BookingRef`, and a `oneOf` would make strict validators
/// refuse every full answer.
fn written_booking_schema() -> utoipa::openapi::Schema {
    let any_of = utoipa::openapi::schema::AnyOfBuilder::new()
        .item(utoipa::openapi::Ref::from_schema_name("Booking"))
        .item(utoipa::openapi::Ref::from_schema_name("BookingRef"))
        .description(Some(
            "The whole booking when the token also has `bookings:read`, else only its id and status.",
        ))
        .build();
    utoipa::openapi::Schema::AnyOf(any_of)
}

/// The answer of a write on a booking.
#[derive(Serialize, ToSchema)]
pub(super) struct WriteAnswer {
    #[schema(schema_with = written_booking_schema)]
    booking: Value,
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

/// List event types
///
/// The event types you manage (your own, and those of the teams you
/// administer), then the internal event types the booking directory offers
/// you, read-only. Disabled event types are left out. Scope: `event-types:read`.
#[utoipa::path(
    get,
    path = "/event-types",
    tag = "event-types",
    operation_id = "listEventTypes",
    params(("cursor" = Option<String>, Query, description = "`next_cursor` of the previous page.")),
    responses(
        (status = 200, description = "A page of event types.", body = EventTypeList),
        (status = 400, description = "`invalid_request`: an unknown or malformed query parameter. `invalid_cursor`."),
    ),
    security(("oauth2" = ["event-types:read"]), ("personalAccessToken" = ["event-types:read"])),
)]
pub(super) async fn list_event_types(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    query: QueryParams,
) -> ApiResult {
    let caller = authenticate(&state, &headers, "event-types:read").await?;
    let params = query_params(query, &["cursor"])?;
    let cursor = params.get("cursor").map(String::as_str);
    if cursor.is_some_and(|c| c.is_empty() || c.len() > 64) {
        return Err(ApiError::bad(ErrorCode::InvalidCursor, "Invalid cursor."));
    }
    let (listed, next) = distributable_event_types(&state.pool, &caller.user.id, cursor).await?;
    let event_types = listed
        .into_iter()
        .map(|e| EventTypeSummary {
            id: e.id,
            slug: e.slug,
            title: e.title,
            duration_min: e.duration_min,
            visibility: e.visibility,
            team_id: e.team_id,
            manageable: e.manages,
        })
        .collect();
    let answer = EventTypeList {
        event_types,
        next_cursor: next,
    };
    Ok(ok(StatusCode::OK, to_json(&answer)))
}

/// Read an event type
///
/// Any event type `GET /event-types` lists. Scope: `event-types:read`.
#[utoipa::path(
    get,
    path = "/event-types/{id}",
    tag = "event-types",
    operation_id = "getEventType",
    params(("id" = String, Path, description = "The event type's id."),),
    responses(
        (status = 200, description = "The event type.", body = EventTypeAnswer),
        (status = 404, description = "`not_found`: no such event type, a disabled one, or one you cannot see."),
    ),
    security(("oauth2" = ["event-types:read"]), ("personalAccessToken" = ["event-types:read"])),
)]
pub(super) async fn get_event_type(
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
    let event_type = EventTypeDetail {
        id,
        slug,
        title,
        description,
        duration_min: duration,
        visibility,
        scheduling_mode: team_id.as_ref().map(|_| scheduling_mode),
        team_id,
        manageable: manages,
        requires_confirmation: requires_confirmation != 0,
        location_type,
        timezone: timezone.name().to_string(),
        max_additional_guests: max_guests,
        phone: phone_mode,
    };
    Ok(ok(StatusCode::OK, to_json(&EventTypeAnswer { event_type })))
}

// --- Slots ---

/// List bookable times
///
/// The slots of an event type you manage, computed by the booking page's own
/// engine after the same on-demand calendar sync: weekly hours and date
/// overrides, minimum notice, buffers, the booking horizon and bookable
/// dates, frequency limits, time off, synced calendar events and bookings,
/// shared resources, and for a team event type its members by scheduling
/// mode. Wall times that do not name a single instant (the hour repeated when
/// clocks go back) are left out. Scope: `slots:read`.
#[utoipa::path(
    get,
    path = "/event-types/{id}/slots",
    tag = "slots",
    operation_id = "listSlots",
    params(
        ("id" = String, Path, description = "The event type's id."),
        ("from" = String, Query, format = Date, description = "First day, `YYYY-MM-DD`, in `timezone`: from yesterday to a year ahead."),
        ("to" = String, Query, format = Date, description = "Last day, inclusive: on or after `from`, at most 31 days in all."),
        ("timezone" = Option<String>, Query, description = "IANA timezone of the days and of `date` and `time`, such as `Europe/Paris`. The event type's by default."),
    ),
    responses(
        (status = 200, description = "The slots whose start falls on those days.", body = SlotList),
        (status = 400, description = "`invalid_request`: `from` or `to` missing or not a date, or an unknown parameter. `invalid_range`. `invalid_timezone`."),
        (status = 403, description = "`forbidden`: an internal event type listed read-only; only its owner or a team admin can read its slots."),
        (status = 404, description = "`not_found`: no such event type, a disabled one, or one you cannot see."),
    ),
    security(("oauth2" = ["slots:read"]), ("personalAccessToken" = ["slots:read"])),
)]
pub(super) async fn event_type_slots(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    path: ApiPath,
    query: QueryParams,
) -> ApiResult {
    let caller = authenticate(&state, &headers, "slots:read").await?;
    let id = path_id(path)?;
    let params = query_params(query, &["from", "to", "timezone"])?;
    let date = |name: &str| -> Result<NaiveDate, ApiError> {
        let value = params.get(name).ok_or_else(|| {
            ApiError::bad(ErrorCode::InvalidRequest, format!("{name} is required."))
        })?;
        NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| {
            ApiError::bad(
                ErrorCode::InvalidRequest,
                format!("{name} must be a YYYY-MM-DD date."),
            )
        })
    };
    let (from, to) = (date("from")?, date("to")?);
    if to < from || (to - from).num_days() >= MAX_SLOT_DAYS {
        return Err(ApiError::bad(
            ErrorCode::InvalidRange,
            format!("to must be on or after from, at most {MAX_SLOT_DAYS} days in all."),
        ));
    }
    match event_type_access(&state.pool, &caller.user.id, &id).await? {
        Some(true) => {}
        Some(false) => {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                ErrorCode::Forbidden,
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
            ErrorCode::InvalidRange,
            "from must be between yesterday and a year ahead.",
        ));
    }
    let slots = api_slots(&state, &id, host_tz, guest_tz, from, to).await?;
    let answer = SlotList {
        event_type_id: id,
        timezone: guest_tz.name().to_string(),
        from: from.to_string(),
        to: to.to_string(),
        slots,
    };
    Ok(ok(StatusCode::OK, to_json(&answer)))
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
) -> Result<Vec<Slot>, ApiError> {
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
        slots.push(Slot {
            start: start.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            end: end.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            date: guest_start.format("%Y-%m-%d").to_string(),
            time: guest_start.format("%H:%M").to_string(),
        });
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
        et.sms_phone_mode, COALESCE(NULLIF(et.timezone, ''), u.timezone, 'UTC') AS stored_tz,
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
    sms_phone_mode: String,
    stored_tz: String,
    attendees: Option<String>,
    /// The stored `start_at`, which the list's cursor pages on.
    raw_start: String,
}

/// A booking as the API shows it to one of its hosts. Never its cancel,
/// reschedule or approval tokens, nor anything about calendars.
fn booking_json(row: BookingRow, with_host_notes: bool, answers: Vec<QuestionAnswer>) -> Booking {
    let stored_tz: Tz = row.stored_tz.parse().unwrap_or(Tz::UTC);
    let instant = |value: &str| {
        crate::booking_time::local(value, stored_tz, Tz::UTC)
            .map(|t| t.and_utc().format("%Y-%m-%dT%H:%M:%SZ").to_string())
    };
    let attendees: Vec<String> = row
        .attendees
        .and_then(|a| serde_json::from_str(&a).ok())
        .unwrap_or_default();
    // The rule every other channel follows (`booking_guest_input`): a phone
    // stored before the event type stopped collecting one is not shown.
    let phone = row
        .guest_phone
        .filter(|p| !p.trim().is_empty())
        .filter(|_| crate::sms::collects_phone(&row.sms_phone_mode));
    let host_notes = if with_host_notes {
        row.host_notes
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
    } else {
        None
    };
    Booking {
        id: row.id,
        uid: row.uid,
        status: row.status,
        start: instant(&row.start_at),
        end: instant(&row.end_at),
        created_at: utc_text(&row.created_at),
        event_type: BookingEventType {
            id: row.et_id,
            slug: row.et_slug,
            title: row.et_title,
            team_id: row.team_id,
        },
        guest: BookingGuest {
            name: row.guest_name,
            email: row.guest_email,
            phone,
            timezone: row.guest_tz,
            language: row.language,
            notes: row.notes,
        },
        answers,
        attendees,
        location: BookingLocation {
            kind: row.location_type,
            value: row.location_value,
        },
        host_notes,
    }
}

/// Booking `id` when `user_id` hosts it (the host-notes rule, `NOTE_HOSTS`).
async fn hosted_booking(
    pool: &SqlitePool,
    user_id: &str,
    id: &str,
) -> Result<Option<Booking>, sqlx::Error> {
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
    let Some(row) = row else {
        return Ok(None);
    };
    // The guest's answers to the booking questions (#210), as webhooks carry them.
    let answers = super::questions::answers_by_booking(pool, &[row.id.as_str()])
        .await
        .remove(&row.id)
        .unwrap_or_default();
    Ok(Some(booking_json(row, true, api_answers(answers))))
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

/// List bookings
///
/// The bookings you host: those of event types you own, those a round-robin
/// team assigned to you, collective bookings of your teams (unless you are
/// excluded from that event type), and bookings you claimed as a watcher.
/// Ordered by start. Scope: `bookings:read`.
#[utoipa::path(
    get,
    path = "/bookings",
    tag = "bookings",
    operation_id = "listBookings",
    params(
        ("cursor" = Option<String>, Query, description = "`next_cursor` of the previous page."),
        ("status" = Option<String>, Query, description = "Only bookings in this status: `pending`, `confirmed`, `cancelled` or `declined`."),
        ("from" = Option<String>, Query, format = DateTime, description = "Only bookings starting at or after this RFC 3339 instant."),
        ("to" = Option<String>, Query, format = DateTime, description = "Only bookings starting before this RFC 3339 instant."),
    ),
    responses(
        (status = 200, description = "A page of bookings. It can be short, even empty, and still have a next cursor.", body = BookingList),
        (status = 400, description = "`invalid_request`: a bad `status`, `from` or `to`, or an unknown parameter. `invalid_cursor`."),
    ),
    security(("oauth2" = ["bookings:read"]), ("personalAccessToken" = ["bookings:read"])),
)]
pub(super) async fn list_bookings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    query: QueryParams,
) -> ApiResult {
    let caller = authenticate(&state, &headers, "bookings:read").await?;
    let params = query_params(query, &["cursor", "status", "from", "to"])?;
    let status = params.get("status").map(String::as_str);
    if status.is_some_and(|s| !matches!(s, "pending" | "confirmed" | "cancelled" | "declined")) {
        return Err(ApiError::bad(
            ErrorCode::InvalidRequest,
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
                        ApiError::bad(
                            ErrorCode::InvalidRequest,
                            format!("{name} must be RFC 3339."),
                        )
                    })
            })
            .transpose()
    };
    let (from, to) = (instant("from")?, instant("to")?);
    let (after_start, after_id) = match params.get("cursor") {
        Some(c) => decode_cursor(c)
            .ok_or_else(|| ApiError::bad(ErrorCode::InvalidCursor, "Invalid cursor."))?,
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
    let ids: Vec<&str> = rows.iter().take(PAGE).map(|r| r.id.as_str()).collect();
    let mut answers = super::questions::answers_by_booking(&state.pool, &ids).await;
    let mut bookings = Vec::new();
    for row in rows.into_iter().take(PAGE) {
        let row_answers = answers.remove(&row.id).unwrap_or_default();
        let booking = booking_json(row, true, api_answers(row_answers));
        let start = booking
            .start
            .as_deref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|t| t.with_timezone(&Utc));
        let inside = match start {
            Some(start) => from.is_none_or(|f| start >= f) && to.is_none_or(|t| start < t),
            None => from.is_none() && to.is_none(),
        };
        if inside {
            bookings.push(booking);
        }
    }
    let answer = BookingList {
        bookings,
        next_cursor: next,
    };
    Ok(ok(StatusCode::OK, to_json(&answer)))
}

/// Read a booking
///
/// A booking you host (see `GET /bookings`). Scope: `bookings:read`.
#[utoipa::path(
    get,
    path = "/bookings/{id}",
    tag = "bookings",
    operation_id = "getBooking",
    params(("id" = String, Path, description = "The booking's id."),),
    responses(
        (status = 200, description = "The booking.", body = BookingAnswer),
        (status = 404, description = "`not_found`: no such booking, or one you do not host."),
    ),
    security(("oauth2" = ["bookings:read"]), ("personalAccessToken" = ["bookings:read"])),
)]
pub(super) async fn get_booking(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    path: ApiPath,
) -> ApiResult {
    let caller = authenticate(&state, &headers, "bookings:read").await?;
    let id = path_id(path)?;
    match hosted_booking(&state.pool, &caller.user.id, &id).await? {
        Some(booking) => Ok(ok(StatusCode::OK, to_json(&BookingAnswer { booking }))),
        None => Err(ApiError::not_found()),
    }
}

/// A booking to make for a guest.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct CreateBooking {
    /// An event type you manage.
    event_type_id: String,
    /// RFC 3339 instant on a whole minute: one of the event type's slots,
    /// as `GET /event-types/{id}/slots` lists them.
    #[schema(format = DateTime)]
    start: String,
    /// IANA zone the guest books in, stored with the booking and used in the
    /// guest's emails; the event type's by default.
    timezone: Option<String>,
    /// The guest's name, 1 to 255 characters.
    #[schema(min_length = 1, max_length = 255)]
    name: String,
    /// The guest's email address.
    #[schema(format = Email, max_length = 255)]
    email: String,
    /// What the guest wants to discuss, at most 5000 characters.
    #[schema(max_length = 5000)]
    notes: Option<String>,
    /// The guest's phone number, in any common format, stored in E.164.
    /// Follows the event type's `phone`: required, optional, or ignored.
    phone: Option<String>,
    /// Email addresses of additional attendees, within the event type's
    /// `max_additional_guests` and at most 10.
    #[serde(default)]
    #[schema(max_items = 10)]
    attendees: Vec<String>,
    /// Language of the guest's emails and SMS: `en`, `fr`, `es`, `pl`,
    /// `de`, `de-formal`, `it`, `et`, `pt` or `nl`. By default yours, else
    /// English. `de` becomes `de-formal` when the instance addresses German
    /// speakers formally.
    language: Option<String>,
}

/// Book for a guest
///
/// Books an event type you manage for a guest, through the booking page's
/// own code: the same validation, a re-check of the time against the slot
/// engine and of the shared resources, the same pending-or-confirmed
/// decision, emails, calendar write-back, meeting link, SMS and webhooks. No
/// invite is needed for an internal or private event type, and none is used
/// up. At most 50 bookings per user per 24 hours. Scope: `bookings:write`.
///
/// With an `Idempotency-Key`, the first answer is kept 24 hours per user and
/// replayed for a retry with the same body; server errors are not kept.
#[utoipa::path(
    post,
    path = "/bookings",
    tag = "bookings",
    operation_id = "createBooking",
    params(
        ("Idempotency-Key" = Option<String>, Header, max_length = 255, description = "1 to 255 visible ASCII characters making a retry safe."),
    ),
    request_body(content = CreateBooking, content_type = "application/json"),
    responses(
        (status = 201, description = "Booked: confirmed, or a pending request when the event type requires approval.", body = WriteAnswer,
            headers(("Idempotency-Replayed" = String, description = "`true` when this is the kept answer of an earlier request with the same key."))),
        (status = 400, description = "`invalid_request`: a missing or invalid field (name, email, notes, language, attendees). `invalid_json`: not a JSON object of the documented fields. `invalid_start`: not an RFC 3339 instant on a whole minute. `invalid_timezone`. `invalid_phone`: missing when the event type requires one, or unreadable. `invalid_attendees`: over the event type's limit, or not email addresses. `invalid_idempotency_key`."),
        (status = 403, description = "`forbidden`: an internal event type listed read-only; only its owner or a team admin can book it for a guest."),
        (status = 404, description = "`not_found`: no such event type, a disabled one, or one you cannot see; also on a replay once you may no longer book it."),
        (status = 409, description = "`slot_unavailable`: the time is not, or no longer, offered. `limit_reached`: a booking frequency limit. `idempotency_in_progress`: the first request with this key still runs."),
        (status = 413, description = "`payload_too_large`: the body is over 16 KiB."),
        (status = 415, description = "`unsupported_media_type`: the body is not `application/json`."),
        (status = 422, description = "`too_soon`: inside the minimum notice. `outside_booking_window`. `booking_refused`: refused for another reason the booking page gives. `too_many_attendees`: over 10 additional attendees. `idempotency_key_reused`: the key came with another body."),
        (status = 429, description = "`daily_limit_reached`: 50 bookings through the API in 24 hours."),
    ),
    security(("oauth2" = ["bookings:write"]), ("personalAccessToken" = ["bookings:write"])),
)]
pub(super) async fn create_booking(
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
                ApiError::new(StatusCode::CONFLICT, ErrorCode::SlotUnavailable, en(key))
            }
            "error-slot-too-soon" => ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                ErrorCode::TooSoon,
                en(key),
            ),
            "error-invalid-date" | "error-invalid-time" => {
                ApiError::bad(ErrorCode::InvalidStart, en(key))
            }
            _ => ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                ErrorCode::BookingRefused,
                en(key),
            ),
        },
        BookingRefusal::Keys("bae-title-unavailable", body) => {
            ApiError::new(StatusCode::CONFLICT, ErrorCode::LimitReached, en(body))
        }
        BookingRefusal::Keys(_, body) => ApiError::bad(ErrorCode::InvalidRequest, en(body)),
        BookingRefusal::Message(_, message) => ApiError::bad(ErrorCode::InvalidRequest, message),
        BookingRefusal::Html(_) => ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            ErrorCode::OutsideBookingWindow,
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
                ErrorCode::Forbidden,
                "Only the owner or a team admin of this event type can book it for a guest.",
            ))
        }
        None => return Err(ApiError::not_found()),
    }
    if let Err(key) = validate_booking_input(&request.name, &request.email, &request.notes) {
        return Err(ApiError::bad(ErrorCode::InvalidRequest, en(key)));
    }
    let lang = match request.language.as_deref() {
        Some(code) => crate::i18n::supported_code(code)
            .ok_or_else(|| ApiError::bad(ErrorCode::InvalidRequest, "Unsupported language."))?,
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
        .map_err(|_| {
            ApiError::bad(
                ErrorCode::InvalidStart,
                "start must be an RFC 3339 instant.",
            )
        })?
        .with_timezone(&Utc);
    let host_tz = get_host_tz(&state.pool, &request.event_type_id).await;
    let guest_tz = match request.timezone.as_deref() {
        Some(tz) => parse_timezone(tz)?,
        None => host_tz,
    };
    let local = start.with_timezone(&guest_tz).naive_local();
    if local.second() != 0 || start.timestamp_subsec_nanos() != 0 {
        return Err(ApiError::bad(
            ErrorCode::InvalidStart,
            "start must fall on a whole minute.",
        ));
    }
    if request.attendees.len() > API_MAX_ATTENDEES {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            ErrorCode::TooManyAttendees,
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
        return Err(ApiError::bad(
            ErrorCode::InvalidRequest,
            "Invalid attendees.",
        ));
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
        // The API captures no leads.
        lead_id: None,
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
    // A manager's booking asks no booking questions (#210).
    let booking = booking_json(row, false, Vec::new());
    Ok((StatusCode::CREATED, to_json(&BookingAnswer { booking })))
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
            ErrorCode::DailyLimitReached,
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
        return Err(ApiError::bad(ErrorCode::InvalidPhone, message));
    }
    if let Err(message) = parse_additional_guests(
        guest.additional_guests,
        max_additional_guests,
        guest.email,
        "en",
    ) {
        return Err(ApiError::bad(ErrorCode::InvalidAttendees, message));
    }
    Ok(())
}

/// How a booking is cancelled.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct CancelBody {
    /// Told to the guest and the attendees, at most 5000 characters.
    #[schema(max_length = 5000)]
    reason: Option<String>,
}

/// How a reschedule is asked for.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct RescheduleBody {
    /// Email the guest the waiting request again, with the same link,
    /// instead of making one; at most once an hour.
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
            ErrorCode::BookingNotActive,
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
        Ok(Some(booking)) if matches!(booking.status.as_str(), "confirmed" | "pending") => {
            ApiError::new(
                StatusCode::FORBIDDEN,
                ErrorCode::Forbidden,
                format!("Only the owner of the event type can {action} this booking."),
            )
        }
        Ok(Some(_)) => not_active(),
        Ok(None) => ApiError::not_found(),
        Err(e) => e.into(),
    }
}

/// Cancel a booking
///
/// What the dashboard's cancel button does: a confirmed booking is cancelled,
/// a pending request declined; the calendar copies and resource reservations
/// are deleted, the guest, the attendees and the host are emailed (with the
/// reason) and the guest texted. Only the owner of the event type can cancel.
/// Scope: `bookings:write`.
#[utoipa::path(
    post,
    path = "/bookings/{id}/cancel",
    tag = "bookings",
    operation_id = "cancelBooking",
    params(("id" = String, Path, description = "The booking's id."),),
    request_body(content = Option<CancelBody>, content_type = "application/json", description = "Optional: an empty body is `{}`."),
    responses(
        (status = 200, description = "Cancelled or declined.", body = WriteAnswer),
        (status = 400, description = "`invalid_request`: a reason over 5000 characters. `invalid_json`."),
        (status = 403, description = "`forbidden`: you host the booking but do not own its event type (with `bookings:read` only; else 404)."),
        (status = 404, description = "`not_found`: no such booking, or one you cannot change."),
        (status = 409, description = "`booking_not_active`: no longer confirmed or pending."),
        (status = 413, description = "`payload_too_large`."),
        (status = 415, description = "`unsupported_media_type`: a body that is not `application/json`."),
    ),
    security(("oauth2" = ["bookings:write"]), ("personalAccessToken" = ["bookings:write"])),
)]
pub(super) async fn cancel_booking_api(
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
            ErrorCode::InvalidRequest,
            en("validate-notes-length"),
        ));
    }
    match host_cancel_booking(&state, &caller.user, &id, request.reason).await {
        HostCancel::Done(status) => {
            tracing::info!(booking_id = %id, status, credential = %caller.credential, "booking ended through the API");
            let booking = hosted_booking(&state.pool, &caller.user.id, &id).await?;
            Ok(ok(
                StatusCode::OK,
                caller.booking_answer(booking.map(|b| to_json(&b)).unwrap_or(Value::Null)),
            ))
        }
        HostCancel::Unchanged => Err(ApiError::new(
            StatusCode::CONFLICT,
            ErrorCode::BookingNotActive,
            "This booking changed state meanwhile.",
        )),
        HostCancel::NotFound => Err(write_refusal(&state.pool, &caller, &id, "cancel").await),
    }
}

/// Ask the guest for a new time
///
/// What the dashboard's reschedule button does: the guest gets a new link and
/// the "pick a new time" email, and the booking keeps its time until they
/// choose one. Moving a booking to a time you choose is not in v1. Only the
/// owner of the event type can ask. Scope: `bookings:write`.
#[utoipa::path(
    post,
    path = "/bookings/{id}/request-reschedule",
    tag = "bookings",
    operation_id = "requestReschedule",
    params(("id" = String, Path, description = "The booking's id."),),
    request_body(content = Option<RescheduleBody>, content_type = "application/json", description = "Optional: an empty body is `{}`."),
    responses(
        (status = 200, description = "Requested, or the request emailed again.", body = WriteAnswer),
        (status = 400, description = "`invalid_json`. `invalid_request`: an unreadable body."),
        (status = 403, description = "`forbidden`: you host the booking but do not own its event type (with `bookings:read` only; else 404)."),
        (status = 404, description = "`not_found`: no such booking, or one you cannot change."),
        (status = 409, description = "`booking_not_active`: no longer confirmed or pending. `reschedule_already_requested`: the guest has not answered an earlier request. `resend_too_soon`: resent less than an hour ago. `no_reschedule_pending`: `resend` with no request waiting. `no_bookable_dates`: the event type has no bookable day left."),
        (status = 413, description = "`payload_too_large`."),
        (status = 415, description = "`unsupported_media_type`: a body that is not `application/json`."),
    ),
    security(("oauth2" = ["bookings:write"]), ("personalAccessToken" = ["bookings:write"])),
)]
pub(super) async fn request_reschedule_api(
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
                caller.booking_answer(booking.map(|b| to_json(&b)).unwrap_or(Value::Null)),
            ))
        }
        HostReschedule::AlreadyRequested => Err(ApiError::new(
            StatusCode::CONFLICT,
            ErrorCode::RescheduleAlreadyRequested,
            en("bookings-error-reschedule-pending"),
        )),
        HostReschedule::ResendTooSoon => Err(ApiError::new(
            StatusCode::CONFLICT,
            ErrorCode::ResendTooSoon,
            en("bookings-error-reschedule-resend-too-soon"),
        )),
        HostReschedule::NotRequested => Err(ApiError::new(
            StatusCode::CONFLICT,
            ErrorCode::NoReschedulePending,
            "No reschedule request is waiting for the guest.",
        )),
        HostReschedule::NoBookableDates => Err(ApiError::new(
            StatusCode::CONFLICT,
            ErrorCode::NoBookableDates,
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
            ErrorCode::InvalidIdempotencyKey,
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
            ErrorCode::IdempotencyInProgress,
            "A request with this Idempotency-Key is in progress.",
        ));
    };
    if stored_hash != request_hash {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            ErrorCode::IdempotencyKeyReused,
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
            ErrorCode::IdempotencyInProgress,
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
        Err(error) => (error.status, error.body()),
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
