//! Stateless Streamable HTTP: JSON responses, no sessions or SSE workers.
//! All requests authenticate independently; tools share dashboard services.
use super::*;
use axum::{body::Bytes, extract::DefaultBodyLimit, http::StatusCode, Json};
use serde_json::{json, Value};

const VERSIONS: [&str; 3] = ["2025-11-25", "2025-06-18", "2025-03-26"];

pub(super) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/mcp", post(handle).get(no_stream).delete(no_stream))
        .layer(DefaultBodyLimit::max(64 * 1024))
}

fn challenge(status: StatusCode, scopes: &str) -> Response {
    let error = if status == StatusCode::FORBIDDEN {
        "insufficient_scope"
    } else {
        "invalid_token"
    };
    (status, [("WWW-Authenticate", format!(
        "Bearer error=\"{error}\", resource_metadata=\"{}/.well-known/oauth-protected-resource/mcp\", scope=\"{scopes}\"",
        crate::oauth::issuer())), ("Cache-Control", "no-store".into())]).into_response()
}

async fn authenticate(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(crate::models::User, String), Box<Response>> {
    if let Some(origin) = headers.get("origin") {
        let expected = reqwest::Url::parse(&crate::oauth::issuer())
            .ok()
            .map(|u| u.origin().ascii_serialization());
        if origin.to_str().ok() != expected.as_deref() {
            return Err(Box::new(StatusCode::FORBIDDEN.into_response()));
        }
    }
    if headers
        .get("mcp-protocol-version")
        .is_some_and(|h| !h.to_str().is_ok_and(|v| VERSIONS.contains(&v)))
    {
        return Err(Box::new(StatusCode::BAD_REQUEST.into_response()));
    }
    crate::oauth::bearer_user(&state.pool, headers, &crate::oauth::resource())
        .await
        .map_err(|e| Box::new(e.into_response()))?
        .ok_or_else(|| {
            Box::new(challenge(
                StatusCode::UNAUTHORIZED,
                &crate::oauth::SCOPES.join(" "),
            ))
        })
}

async fn no_stream(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if let Err(response) = authenticate(&state, &headers).await {
        return *response;
    }
    (StatusCode::METHOD_NOT_ALLOWED, [("Allow", "POST")]).into_response()
}

fn rpc_error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn rpc_response(value: Value) -> Response {
    ([("Cache-Control", "no-store")], Json(value)).into_response()
}

async fn handle(State(state): State<Arc<AppState>>, headers: HeaderMap, body: Bytes) -> Response {
    let (user, scopes) = match authenticate(&state, &headers).await {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let content_type = headers
        .get("content-type")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if !content_type
        .split(';')
        .next()
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("application/json"))
    {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    }
    let accept = headers
        .get("accept")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    let types: Vec<&str> = accept
        .split(',')
        .map(|v| v.split(';').next().unwrap_or("").trim())
        .collect();
    if !types.contains(&"application/json") || !types.contains(&"text/event-stream") {
        return StatusCode::NOT_ACCEPTABLE.into_response();
    }
    let message: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(rpc_error(Value::Null, -32700, "Parse error")),
            )
                .into_response()
        }
    };
    let id = message.get("id").cloned();
    if !message.is_object()
        || message["jsonrpc"] != "2.0"
        || id
            .as_ref()
            .is_some_and(|v| !(v.is_string() || v.is_i64() || v.is_u64()))
        || !message["method"].is_string()
        || message.get("result").is_some()
        || message.get("error").is_some()
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(rpc_error(Value::Null, -32600, "Invalid Request")),
        )
            .into_response();
    }
    let method = message["method"].as_str().unwrap_or("");
    let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
    if !params.is_object() {
        return match id {
            Some(id) => rpc_response(rpc_error(id, -32602, "Invalid params")),
            None => StatusCode::BAD_REQUEST.into_response(),
        };
    }
    let Some(id) = id else {
        // Notifications never invoke tools or other request methods.
        return if method.starts_with("notifications/") {
            StatusCode::ACCEPTED.into_response()
        } else {
            StatusCode::BAD_REQUEST.into_response()
        };
    };
    let result = match method {
        "initialize" => {
            let Some(version) = params["protocolVersion"].as_str() else {
                return rpc_response(rpc_error(id, -32602, "protocolVersion is required"));
            };
            if !params["capabilities"].is_object()
                || !params["clientInfo"]["name"].is_string()
                || !params["clientInfo"]["version"].is_string()
            {
                return rpc_response(rpc_error(
                    id,
                    -32602,
                    "capabilities and clientInfo are required",
                ));
            }
            json!({"protocolVersion": if VERSIONS.contains(&version) { version } else { VERSIONS[0] },
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "calrs", "version": env!("CARGO_PKG_VERSION")}})
        }
        "ping" => json!({}),
        "tools/list" => json!({"tools": tools()}),
        "tools/call" => {
            let Some(name) = params["name"].as_str() else {
                return rpc_response(rpc_error(id, -32602, "Tool name is required"));
            };
            let required = match name {
                "list_event_types" => crate::oauth::SCOPES[0],
                "get_calendar_blocks" => crate::oauth::SCOPES[1],
                "create_invite_link" => crate::oauth::SCOPES[2],
                _ => return rpc_response(rpc_error(id, -32602, "Unknown tool")),
            };
            if !scopes.split_whitespace().any(|s| s == required) {
                return challenge(StatusCode::FORBIDDEN, required);
            }
            // Bound write amplification independently of the IP login limiter.
            if name == "create_invite_link"
                && state
                    .booking_limiter
                    .check_limited(&format!("mcp-invite:{}", user.id))
                    .await
            {
                return StatusCode::TOO_MANY_REQUESTS.into_response();
            }
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            match call_tool(&state.pool, &user, name, arguments).await {
                Ok(value) => {
                    json!({"content": [{"type": "text", "text": value.to_string()}], "structuredContent": value, "isError": false})
                }
                Err(error) => {
                    json!({"content": [{"type": "text", "text": error}], "isError": true})
                }
            }
        }
        _ => return rpc_response(rpc_error(id, -32601, "Method not found")),
    };
    rpc_response(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

fn tools() -> Value {
    json!([
        {"name": "list_event_types", "description": "List enabled event types you can distribute: your managed personal/team events and internal events. Use an event_type_id with create_invite_link. Follow next_cursor until null.",
         "inputSchema": {"type":"object", "properties":{"cursor":{"type":"string","description":"next_cursor from the previous page"}}, "additionalProperties":false},
         "annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}},
        {"name": "create_invite_link", "description": "Get a booking link for an enabled event type. Internal/private events get a new single-use invite expiring in 7 days. Public events return their reusable booking URL (null expiry/use limit); private teams use their existing team access token. Sends no email or booking. Internal events are available to all users; others require owner/team-admin permission.",
         "inputSchema":{"type":"object","properties":{"event_type_id":{"type":"string"}},"required":["event_type_id"],"additionalProperties":false},
         "annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false}},
        {"name": "get_calendar_blocks", "description": "Get your own busy intervals from the current calendar cache and bookings (including pending holds). RFC3339 start/end, at most 31 days. Returns merged UTC intervals and source sync health, never event titles or other people's calendars. This does not calculate bookable slots or refresh remote calendars.",
         "inputSchema":{"type":"object","properties":{"start":{"type":"string","format":"date-time"},"end":{"type":"string","format":"date-time"}},"required":["start","end"],"additionalProperties":false},
         "annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}}
    ])
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListArgs {
    cursor: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InviteArgs {
    event_type_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BlockArgs {
    start: String,
    end: String,
}

fn db_error(error: sqlx::Error) -> String {
    tracing::error!(%error, "MCP tool database operation failed");
    "Database operation failed; try again later.".into()
}

async fn call_tool(
    pool: &SqlitePool,
    user: &crate::models::User,
    name: &str,
    arguments: Value,
) -> Result<Value, String> {
    match name {
        "list_event_types" => {
            let args: ListArgs = serde_json::from_value(arguments)
                .map_err(|_| "Expected an object with an optional cursor.")?;
            if args.cursor.as_ref().is_some_and(|s| s.len() > 64) {
                return Err("Invalid cursor.".into());
            }
            // Page over stable IDs before applying the shared permissions
            // helper. Sparse pages still advance next_cursor.
            let rows: Vec<(String, String, String, i32, String, Option<String>)> = sqlx::query_as(
                "SELECT id, slug, title, duration_min, visibility, team_id FROM event_types WHERE enabled = 1 AND id > ? ORDER BY id LIMIT 101")
                .bind(args.cursor.as_deref().unwrap_or("")).fetch_all(pool).await.map_err(db_error)?;
            let next = if rows.len() > 100 {
                Some(rows[99].0.clone())
            } else {
                None
            };
            let mut events = Vec::new();
            for (id, slug, title, duration, visibility, team) in rows.into_iter().take(100) {
                if visibility == "internal" || can_manage_event_type(pool, user, &id).await {
                    events.push(json!({"id":id,"slug":slug,"title":title,"duration_min":duration,"visibility":visibility,"team_id":team}));
                }
            }
            Ok(json!({"event_types":events,"next_cursor":next}))
        }
        "create_invite_link" => {
            let args: InviteArgs =
                serde_json::from_value(arguments).map_err(|_| "Expected event_type_id only.")?;
            if args.event_type_id.is_empty() || args.event_type_id.len() > 64 {
                return Err("Invalid event_type_id.".into());
            }
            let mut invite = create_quick_invite(pool, user, &args.event_type_id, true)
                .await
                .map_err(db_error)?
                .ok_or("Event type not found, disabled, or not permitted.")?;
            // The CLI/dev fallback mirrors OAuth discovery; production uses the
            // same existing public URL setting as dashboard invite links.
            if invite.url.starts_with('/') {
                invite.url = format!("{}{}", crate::oauth::issuer(), invite.url);
            }
            Ok(
                json!({"url":invite.url,"expires_at":invite.expires_at.map(|v| v.replace(' ', "T") + "Z"),"max_uses":invite.max_uses}),
            )
        }
        "get_calendar_blocks" => {
            let args: BlockArgs = serde_json::from_value(arguments)
                .map_err(|_| "Expected RFC3339 start and end only.")?;
            let start = chrono::DateTime::parse_from_rfc3339(&args.start)
                .map_err(|_| "Invalid start; use RFC3339.")?
                .naive_utc();
            let end = chrono::DateTime::parse_from_rfc3339(&args.end)
                .map_err(|_| "Invalid end; use RFC3339.")?
                .naive_utc();
            if end <= start
                || end - start > Duration::days(31)
                || !(1970..=9998).contains(&start.year())
                || !(1970..=9998).contains(&end.year())
            {
                return Err(
                    "The interval must be positive, at most 31 days, and between 1970 and 9998."
                        .into(),
                );
            }
            // Stored calendar times use their source timezone. Widen the SQL
            // window, convert to UTC with the shared helper, then clip exactly.
            let busy = try_fetch_busy_times_for_user_opts(
                pool,
                &user.id,
                start - Duration::days(2),
                end + Duration::days(2),
                chrono_tz::UTC,
                None,
                None,
                None,
                true,
            )
            .await
            .map_err(db_error)?;
            let mut clipped: Vec<_> = busy
                .into_iter()
                .filter_map(|(s, e)| {
                    let s = s.max(start);
                    let e = e.min(end);
                    (s < e).then_some((s, e))
                })
                .collect();
            clipped.sort_unstable();
            let mut merged: Vec<(NaiveDateTime, NaiveDateTime)> = Vec::new();
            for (s, e) in clipped {
                if let Some(last) = merged.last_mut().filter(|last| s <= last.1) {
                    last.1 = last.1.max(e);
                } else {
                    merged.push((s, e));
                }
            }
            let sources: Vec<(String, Option<String>, bool)> = sqlx::query_as(
                "SELECT cs.id, cs.last_synced, cs.last_sync_error IS NOT NULL FROM caldav_sources cs JOIN accounts a ON a.id = cs.account_id WHERE a.user_id = ? AND cs.enabled = 1 ORDER BY cs.id")
                .bind(&user.id).fetch_all(pool).await.map_err(db_error)?;
            Ok(
                json!({"blocks": merged.into_iter().map(|(s,e)| json!({"start":s.and_utc().to_rfc3339(),"end":e.and_utc().to_rfc3339()})).collect::<Vec<_>>(),
                "sources": sources.into_iter().map(|(id,last,error)| json!({"id":id,"last_synced":last,"sync_failed":error})).collect::<Vec<_>>(),"cached":true}),
            )
        }
        _ => Err("Unknown tool.".into()),
    }
}
