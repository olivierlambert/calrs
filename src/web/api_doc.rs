//! OpenAPI document of the REST API v1 (#169) and its interactive reference.
//!
//! The document is generated from `api.rs`: every handler carries a
//! `#[utoipa::path]` and every request and answer type derives `ToSchema`,
//! so the schemas are the types the handlers serialize. `document()` adds
//! what depends on the instance (the server URL, the OAuth endpoints) and the
//! answers every authenticated operation shares (`CommonAnswers`). It is
//! served publicly at `/api/v1/openapi.json`: it describes the API and holds
//! no data. `/api/docs` renders it for signed-in users with the vendored
//! Swagger UI (`assets/swagger-ui/`), which sends no cookie and makes no
//! request to another origin.
use super::*;
use utoipa::openapi::{
    self,
    header::Header,
    schema::{ObjectBuilder, Type},
    security::{AuthorizationCode, Flow, Http, HttpAuthScheme, OAuth2, Scopes, SecurityScheme},
    server::Server,
    Content, Ref, RefOr, Response as OpenApiResponse,
};
use utoipa::{Modify, OpenApi};

/// Version of the vendored Swagger UI, also the cache buster of its URLs.
pub(super) const SWAGGER_UI_VERSION: &str = "5.34.1";

const DESCRIPTION: &str = r#"JSON API of this calrs instance: your event types, their bookable times, the bookings you host, and booking, cancelling or asking a guest for a new time on their behalf. A booking made here goes through the booking page's own code: the same checks, emails, calendar write-back, SMS and webhooks.

## Authentication

Every request carries `Authorization: Bearer <token>`. Cookies never authenticate the API, and a request from a browser page of another origin is refused (there is no CORS). Two kinds of token:

- **Personal access token** (`personalAccessToken`): `calrs_pat_` followed by 43 characters, created in Profile & Settings, API tokens, with the scopes you choose. Paste one under Authorize to try the operations here.
- **OAuth 2.1** (`oauth2`): authorization code with PKCE (S256) against this instance's own authorization server, for applications acting for many users. Clients are public and register at `/oauth/register` (RFC 7591); the authorization request names the API with `resource=<server URL>` (RFC 8707). Metadata: `/.well-known/oauth-authorization-server` and `/.well-known/oauth-protected-resource/api/v1`.

Each operation names the scope it needs. By default only administrators may use the API; an administrator can open it to every user.

## Conventions

- Request bodies are `application/json` (415 otherwise) of at most 16 KiB (413). Unknown fields and unknown query parameters are refused (400).
- Times are RFC 3339; answers give them in UTC.
- Every answer carries `Cache-Control: no-store`.
- Lists are paged: pass `next_cursor` back as `cursor` until it is null. A page can be short, even empty, and still have a next cursor.
- Errors are `{"error": {"code": "...", "message": "..."}}`. `code` is stable (the `ErrorCode` schema lists them all); `message` is English text for people and may change. An unknown path answers 404 `not_found`, an unsupported method 405 `method_not_allowed`.
- Rate limits: 300 requests per minute per token; slot reads and every `POST` also 60 per 5 minutes per user, shared by all of the user's tokens; 50 bookings per user per 24 hours. A 429 carries `Retry-After`.
- Within v1, fields may be added to answers and endpoints may appear; nothing is removed or changes meaning."#;

#[derive(OpenApi)]
#[openapi(
    // `version` is the API's, not calrs's: the document is public, and the
    // exact release would only help someone matching it to known bugs.
    info(title = "calrs REST API", version = "1", description = DESCRIPTION),
    external_docs(url = "https://cal.rs/docs/api.html", description = "The REST API guide"),
    paths(
        api::list_event_types,
        api::get_event_type,
        api::event_type_slots,
        api::list_bookings,
        api::create_booking,
        api::get_booking,
        api::cancel_booking_api,
        api::request_reschedule_api,
        openapi_json,
    ),
    components(schemas(api::ErrorBody, api::ErrorCode, api::BookingRef)),
    tags(
        (name = "event-types", description = "The event types you manage, and the internal ones the booking directory offers you."),
        (name = "slots", description = "Bookable times, computed by the booking page's own engine."),
        (name = "bookings", description = "The bookings you host, and booking, cancelling or rescheduling on a guest's behalf."),
        (name = "meta", description = "This document."),
    ),
    modifiers(&CommonAnswers),
)]
struct ApiDoc;

/// The document for this instance: the server URL and the OAuth endpoints
/// follow the base URL, which an administrator can change at runtime.
/// `base` is read once by the caller, so the document and its cache key
/// always agree.
pub(crate) fn document(base: &str) -> openapi::OpenApi {
    let mut doc = ApiDoc::openapi();
    doc.servers = Some(vec![Server::new(format!("{base}/api/v1"))]);
    let components = doc.components.get_or_insert_with(Default::default);
    components.add_security_scheme("oauth2", oauth2_scheme(base));
    components.add_security_scheme("personalAccessToken", personal_token_scheme());
    doc
}

fn oauth2_scheme(base: &str) -> SecurityScheme {
    let descriptions = [
        "List and read your event types.",
        "Read the bookable times of the event types you manage.",
        "Read the bookings you host, guest details included.",
        "Book, cancel and request a reschedule for a guest.",
    ];
    let scopes = Scopes::from_iter(crate::oauth::API_SCOPES.into_iter().zip(descriptions));
    let mut code = AuthorizationCode::new(
        format!("{base}/oauth/authorize"),
        format!("{base}/oauth/token"),
        scopes,
    );
    code.refresh_url = Some(format!("{base}/oauth/token"));
    SecurityScheme::OAuth2(OAuth2::with_description(
        [Flow::AuthorizationCode(code)],
        format!(
            "OAuth 2.1 authorization code with PKCE (`code_challenge_method=S256`, required) against \
             this instance. Clients are public (no secret): register one at `{base}/oauth/register` \
             (RFC 7591, `token_endpoint_auth_method=none`). The authorization and token requests \
             carry `resource={base}/api/v1` (RFC 8707): a grant is bound to it, so a token for \
             calrs's MCP server does not open this API, nor the other way round. Omitting `scope` \
             asks for all four. Access tokens last one hour; refresh tokens rotate; a grant lasts \
             30 days and is revoked at `{base}/oauth/revoke` or in Profile & Settings, Connected apps."
        ),
    ))
}

fn personal_token_scheme() -> SecurityScheme {
    let mut http = Http::new(HttpAuthScheme::Bearer);
    http.bearer_format = Some("calrs_pat_<43 characters>".to_string());
    http.description = Some(
        "A personal access token, created in Profile & Settings, API tokens, and shown once. It \
         carries the scopes chosen when it was made, acts as its user (checked again on every \
         request), and stops working when it expires, is revoked, or its user is suspended or \
         may no longer use the API."
            .to_string(),
    );
    SecurityScheme::Http(http)
}

/// The answers every authenticated operation can give, added to each so the
/// handlers' attributes only list their own: 401, the generic 403 codes, 429
/// and 500, the error envelope on every error, and the headers.
struct CommonAnswers;

const UNAUTHORIZED: &str = "`unauthorized`: no Bearer token, or one that is invalid, expired or \
    revoked, or whose user is disabled or may no longer use the API. `WWW-Authenticate` names the \
    resource metadata and the scopes, and adds `error=\"invalid_token\"` when a token was presented.";
const FORBIDDEN: &str = "`insufficient_scope`: the token lacks this operation's scope \
    (`WWW-Authenticate` names it). `origin_not_allowed`: the request came from a browser page of \
    another origin.";
const RATE_LIMITED: &str = "`rate_limited`: over 300 requests per minute for this token, or for \
    slot reads and writes over 60 per 5 minutes for this user.";
const INTERNAL: &str = "`internal_error`: something failed on the server; try again later.";

fn string_header(description: &str) -> RefOr<Header> {
    let mut header = Header::new(ObjectBuilder::new().schema_type(Type::String));
    header.description = Some(description.to_string());
    RefOr::T(header)
}

fn error_content() -> RefOr<Content> {
    RefOr::T(Content::new(Some(Ref::from_schema_name("ErrorBody"))))
}

impl Modify for CommonAnswers {
    fn modify(&self, doc: &mut openapi::OpenApi) {
        for item in doc.paths.paths.values_mut() {
            for operation in [&mut item.get, &mut item.post].into_iter().flatten() {
                let secured = operation
                    .security
                    .as_ref()
                    .is_some_and(|s| !s.is_empty() && s.iter().any(|r| r != &Default::default()));
                if !secured {
                    continue;
                }
                let answers = &mut operation.responses.responses;
                for (status, text) in [
                    ("401", UNAUTHORIZED),
                    ("403", FORBIDDEN),
                    ("429", RATE_LIMITED),
                    ("500", INTERNAL),
                ] {
                    match answers.get_mut(status) {
                        Some(RefOr::T(answer)) => {
                            answer.description = format!("{} {text}", answer.description);
                        }
                        Some(RefOr::Ref(_)) => {}
                        None => {
                            answers
                                .insert(status.to_string(), RefOr::T(OpenApiResponse::new(text)));
                        }
                    }
                }
                for (status, answer) in answers.iter_mut() {
                    let RefOr::T(answer) = answer else { continue };
                    answer.headers.insert(
                        "Cache-Control".to_string(),
                        string_header("Always `no-store`."),
                    );
                    if status.starts_with('4') || status.starts_with('5') {
                        answer
                            .content
                            .entry("application/json".to_string())
                            .or_insert_with(error_content);
                    }
                    if status == "401" || status == "403" {
                        answer.headers.insert(
                            "WWW-Authenticate".to_string(),
                            string_header(
                                "On `unauthorized` and `insufficient_scope`: the Bearer challenge \
                                 (RFC 6750), with `resource_metadata` and `scope`.",
                            ),
                        );
                    }
                    if status == "429" {
                        answer.headers.insert(
                            "Retry-After".to_string(),
                            string_header("Seconds to wait before retrying."),
                        );
                    }
                }
            }
        }
    }
}

/// The document as served, cached per base URL.
fn document_text() -> Arc<str> {
    static CACHE: std::sync::Mutex<Option<(String, Arc<str>)>> = std::sync::Mutex::new(None);
    let base = crate::oauth::issuer();
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((cached_base, text)) = cache.as_ref() {
        if *cached_base == base {
            return text.clone();
        }
    }
    let text: Arc<str> = serde_json::to_string(&document(&base))
        .unwrap_or_else(|_| "{}".to_string())
        .into();
    *cache = Some((base, text.clone()));
    text
}

/// This document.
#[utoipa::path(
    get,
    path = "/openapi.json",
    tag = "meta",
    operation_id = "getOpenApiDocument",
    summary = "This OpenAPI document",
    description = "The OpenAPI 3.1 description of this API, generated by calrs itself. Public: it holds no data.",
    security(()),
    responses((status = 200, description = "The document.", body = Object, content_type = "application/json")),
)]
pub(super) async fn openapi_json() -> Response {
    (
        [
            ("Content-Type", "application/json"),
            ("Cache-Control", "no-cache"),
        ],
        document_text().to_string(),
    )
        .into_response()
}

/// `/api/docs`: the interactive reference, for signed-in users.
pub(super) async fn api_docs_page(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
) -> Response {
    let allowed = super::api_tokens::can_create_api_tokens(&state.pool, &auth_user.user).await;
    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);
    let tmpl = match state.templates.get_template("api_docs.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("api docs template", &e),
    };
    match tmpl.render(context! {
        lang => auth_user.lang,
        sidebar => sidebar_context(&auth_user, "settings"),
        allowed => allowed,
        swagger_version => SWAGGER_UI_VERSION,
        impersonating => impersonating,
        impersonating_name => impersonating_name,
    }) {
        Ok(body) => Html(body).into_response(),
        Err(e) => internal_error_response("render api docs", &e),
    }
}

/// The vendored Swagger UI (`assets/swagger-ui/README.md`), baked into the
/// binary. An exact-match allowlist, as for intl-tel-input: no request path
/// can address anything else. The URLs carry `?v=<version>`, so the long
/// cache lifetime never outlives an update.
pub(super) async fn serve_swagger_ui_asset(Path(file): Path<String>) -> Response {
    let (bytes, content_type): (&'static [u8], &str) = match file.as_str() {
        "swagger-ui-bundle.js" => (
            include_bytes!("../../assets/swagger-ui/swagger-ui-bundle.js"),
            "application/javascript; charset=utf-8",
        ),
        "swagger-ui.css" => (
            include_bytes!("../../assets/swagger-ui/swagger-ui.css"),
            "text/css; charset=utf-8",
        ),
        _ => return axum::http::StatusCode::NOT_FOUND.into_response(),
    };
    (
        [
            ("Content-Type", content_type),
            ("Cache-Control", "public, max-age=31536000, immutable"),
        ],
        bytes,
    )
        .into_response()
}
