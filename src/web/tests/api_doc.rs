// OpenAPI document of the REST API (#169): generated, valid OpenAPI 3.1, in
// step with the router and the handlers' scopes, answers matching their
// schemas; and the interactive reference at /api/docs with its vendored
// Swagger UI.

use super::api::{api_get, api_post, create_body, monday_at, pat, user_id, ALL};
use super::*;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use std::collections::BTreeSet;

const API_SOURCE: &str = include_str!("../api.rs");
const METHODS: [&str; 5] = ["get", "post", "put", "patch", "delete"];

async fn document(app: &Router) -> Value {
    let response = app
        .clone()
        .oneshot(get("/api/v1/openapi.json"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/json"
    );
    assert!(response.headers().get("set-cookie").is_none());
    serde_json::from_str(&body_string(response).await).unwrap()
}

/// `(METHOD, path)` of every operation of the document.
fn documented(doc: &Value) -> BTreeSet<(String, String)> {
    let mut operations = BTreeSet::new();
    for (path, item) in doc["paths"].as_object().unwrap() {
        for method in METHODS {
            if item.get(method).is_some() {
                operations.insert((method.to_string(), path.clone()));
            }
        }
    }
    operations
}

/// The method names an axum `MethodRouter` expression chains:
/// `get(a).post(b)` gives `get` and `post`.
fn route_methods(expr: &syn::Expr, out: &mut Vec<String>) {
    match expr {
        syn::Expr::Call(call) => {
            if let syn::Expr::Path(path) = &*call.func {
                let name = path.path.segments.last().unwrap().ident.to_string();
                if METHODS.contains(&name.as_str()) {
                    out.push(name);
                }
            }
        }
        syn::Expr::MethodCall(call) => {
            let name = call.method.to_string();
            if METHODS.contains(&name.as_str()) {
                out.push(name);
            }
            route_methods(&call.receiver, out);
        }
        _ => {}
    }
}

/// `(METHOD, path)` of every route `api::router()` declares, read from the
/// source so a route added there without a `#[utoipa::path]` is caught.
fn routed() -> BTreeSet<(String, String)> {
    struct Routes(BTreeSet<(String, String)>);
    impl<'ast> syn::visit::Visit<'ast> for Routes {
        fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
            if call.method == "route" && call.args.len() == 2 {
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(path),
                    ..
                }) = &call.args[0]
                {
                    let mut methods = Vec::new();
                    route_methods(&call.args[1], &mut methods);
                    assert!(!methods.is_empty(), "no method for {}", path.value());
                    for method in methods {
                        self.0.insert((method, path.value()));
                    }
                }
            }
            syn::visit::visit_expr_method_call(self, call);
        }
    }
    let file = syn::parse_file(API_SOURCE).unwrap();
    let router = file
        .items
        .iter()
        .find_map(|item| match item {
            syn::Item::Fn(f) if f.sig.ident == "router" => Some(f),
            _ => None,
        })
        .expect("api::router()");
    let mut routes = Routes(BTreeSet::new());
    syn::visit::Visit::visit_item_fn(&mut routes, router);
    routes.0
}

#[tokio::test]
async fn the_document_is_openapi_3_1_for_this_instance() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, _, _, _) = setup_test_app().await;
    let doc = document(&app).await;

    assert!(doc["openapi"].as_str().unwrap().starts_with("3.1."));
    assert_eq!(doc["info"]["title"], "calrs REST API");
    // The API's version, never the release a visitor could fingerprint.
    assert_eq!(doc["info"]["version"], "1");
    assert!(!doc.to_string().contains(env!("CARGO_PKG_VERSION")));
    assert!(doc["info"]["description"].as_str().unwrap().len() > 200);
    assert_eq!(
        doc["servers"],
        json!([{"url": "https://cal.example/api/v1"}])
    );
    assert!(!doc["paths"].as_object().unwrap().is_empty());

    let schemes = &doc["components"]["securitySchemes"];
    let code = &schemes["oauth2"]["flows"]["authorizationCode"];
    assert_eq!(schemes["oauth2"]["type"], "oauth2");
    assert_eq!(
        code["authorizationUrl"],
        "https://cal.example/oauth/authorize"
    );
    assert_eq!(code["tokenUrl"], "https://cal.example/oauth/token");
    assert_eq!(code["refreshUrl"], "https://cal.example/oauth/token");
    let scopes: Vec<&str> = code["scopes"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let mut expected = crate::oauth::API_SCOPES.to_vec();
    expected.sort_unstable();
    assert_eq!(scopes, expected);
    let description = schemes["oauth2"]["description"].as_str().unwrap();
    assert!(description.contains("S256"));
    assert!(description.contains("resource=https://cal.example/api/v1"));
    assert_eq!(schemes["personalAccessToken"]["type"], "http");
    assert_eq!(schemes["personalAccessToken"]["scheme"], "bearer");
    assert!(schemes["personalAccessToken"]["bearerFormat"]
        .as_str()
        .unwrap()
        .starts_with("calrs_pat_"));

    // Every reference resolves, every operation has a unique id, every
    // authenticated one names exactly one API scope for both schemes, and
    // every error answer carries the envelope and `Cache-Control`.
    fn refs(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                if let Some(Value::String(r)) = map.get("$ref") {
                    out.push(r.clone());
                }
                map.values().for_each(|v| refs(v, out));
            }
            Value::Array(items) => items.iter().for_each(|v| refs(v, out)),
            _ => {}
        }
    }
    let mut all = Vec::new();
    refs(&doc, &mut all);
    for r in &all {
        let name = r.strip_prefix("#/components/schemas/").unwrap();
        assert!(
            doc["components"]["schemas"].get(name).is_some(),
            "{r} resolves"
        );
    }
    let mut ids = BTreeSet::new();
    for (method, path) in documented(&doc) {
        let op = &doc["paths"][&path][&method];
        assert!(ids.insert(op["operationId"].as_str().unwrap().to_string()));
        assert!(!op["tags"].as_array().unwrap().is_empty());
        let security = op["security"].as_array().unwrap();
        if path == "/openapi.json" {
            assert_eq!(security, &vec![json!({})], "the document is public");
            continue;
        }
        assert_eq!(security.len(), 2, "{method} {path}");
        let scope = security[0]["oauth2"][0].as_str().unwrap();
        assert!(crate::oauth::API_SCOPES.contains(&scope));
        assert_eq!(security[0], json!({"oauth2": [scope]}));
        assert_eq!(security[1], json!({"personalAccessToken": [scope]}));
        assert!(op["description"].as_str().unwrap().contains(scope));
        for status in ["401", "403", "429", "500"] {
            assert!(op["responses"].get(status).is_some(), "{path} {status}");
        }
        for (status, answer) in op["responses"].as_object().unwrap() {
            assert!(answer["headers"].get("Cache-Control").is_some());
            if status.as_str() >= "400" {
                assert_eq!(
                    answer["content"]["application/json"]["schema"]["$ref"],
                    "#/components/schemas/ErrorBody",
                    "{method} {path} {status}"
                );
            }
        }
        assert!(op["responses"]["429"]["headers"]
            .get("Retry-After")
            .is_some());
        assert!(op["responses"]["401"]["headers"]
            .get("WWW-Authenticate")
            .is_some());
    }

    // `conforms` reads objects as closed, but OpenAPI 3.1 objects are open:
    // a `oneOf` whose branch requires a subset of another's would match a
    // value of the larger branch twice, which strict validators refuse.
    fn one_ofs<'a>(value: &'a Value, out: &mut Vec<&'a Vec<Value>>) {
        match value {
            Value::Object(map) => {
                if let Some(Value::Array(branches)) = map.get("oneOf") {
                    out.push(branches);
                }
                map.values().for_each(|v| one_ofs(v, out));
            }
            Value::Array(items) => items.iter().for_each(|v| one_ofs(v, out)),
            _ => {}
        }
    }
    let required = |branch: &Value| -> BTreeSet<String> {
        let branch = match branch["$ref"].as_str() {
            Some(r) => &doc["components"]["schemas"][r.trim_start_matches("#/components/schemas/")],
            None => branch,
        };
        branch["required"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| r.as_str().map(String::from))
            .collect()
    };
    let mut found = Vec::new();
    one_ofs(&doc, &mut found);
    for branches in found {
        for (i, a) in branches.iter().enumerate() {
            for (j, b) in branches.iter().enumerate() {
                if i != j && a.get("$ref").is_some() && b.get("$ref").is_some() {
                    assert!(
                        !required(a).is_subset(&required(b)),
                        "oneOf {a} and {b}: a value of the second matches both"
                    );
                }
            }
        }
    }
    let written = &doc["components"]["schemas"]["WriteAnswer"]["properties"]["booking"];
    assert_eq!(
        written["anyOf"].as_array().map(Vec::len),
        Some(2),
        "{written}"
    );

    // Request bodies refuse unknown fields, as the handlers do.
    for name in ["CreateBooking", "CancelBody", "RescheduleBody"] {
        assert_eq!(
            doc["components"]["schemas"][name]["additionalProperties"], false,
            "{name}"
        );
    }
    let create = &doc["paths"]["/bookings"]["post"];
    assert!(create["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["name"] == "Idempotency-Key" && p["in"] == "header"));
    assert_eq!(create["requestBody"]["required"], true);
    for path in ["/bookings/{id}/cancel", "/bookings/{id}/request-reschedule"] {
        assert_ne!(
            doc["paths"][path]["post"]["requestBody"]["required"], true,
            "{path}: an empty body is accepted"
        );
    }
}

#[tokio::test]
async fn every_api_route_is_documented_and_every_operation_routed() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, _, _, _) = setup_test_app().await;
    let doc = document(&app).await;
    let documented = documented(&doc);
    assert_eq!(routed(), documented, "api::router() and the OpenAPI paths");

    // And as served: a documented operation reaches its handler (no token:
    // 401, not the 404 or 405 of the fallbacks), a method left out of the
    // document is not routed.
    let id = uuid::Uuid::new_v4().to_string();
    let paths: BTreeSet<&String> = documented.iter().map(|(_, p)| p).collect();
    for path in paths {
        let uri = format!("/api/v1{}", path.replace("{id}", &id));
        for method in METHODS {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method.to_uppercase().as_str())
                        .uri(&uri)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let status = response.status();
            if !documented.contains(&(method.to_string(), path.clone())) {
                assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED, "{method} {uri}");
            } else if path == "/openapi.json" {
                assert_eq!(status, StatusCode::OK);
            } else {
                assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}");
            }
        }
    }
}

#[tokio::test]
async fn each_operation_documents_the_scope_its_handler_checks() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, _, _, _) = setup_test_app().await;
    let doc = document(&app).await;

    // The scope literal of the `authenticate(.., .., "scope")` call in a
    // handler's body.
    struct Scope(Option<String>);
    impl<'ast> syn::visit::Visit<'ast> for Scope {
        fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
            if let syn::Expr::Path(path) = &*call.func {
                if path.path.is_ident("authenticate") {
                    if let Some(syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(scope),
                        ..
                    })) = call.args.iter().nth(2)
                    {
                        self.0 = Some(scope.value());
                    }
                }
            }
            syn::visit::visit_expr_call(self, call);
        }
    }
    let file = syn::parse_file(API_SOURCE).unwrap();
    let mut checked = 0;
    for item in &file.items {
        let syn::Item::Fn(f) = item else { continue };
        let Some(attr) = f.attrs.iter().find(|a| {
            a.path().segments.len() == 2
                && a.path().segments[0].ident == "utoipa"
                && a.path().segments[1].ident == "path"
        }) else {
            continue;
        };
        let syn::Meta::List(list) = &attr.meta else {
            panic!("#[utoipa::path(...)]")
        };
        let tokens = list.tokens.to_string();
        let method = tokens.split([' ', ',']).next().unwrap().to_string();
        let path = tokens
            .split("path = \"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .unwrap()
            .to_string();
        let mut scope = Scope(None);
        syn::visit::Visit::visit_item_fn(&mut scope, f);
        let scope = scope
            .0
            .unwrap_or_else(|| panic!("{} authenticates", f.sig.ident));
        let security = &doc["paths"][&path][&method]["security"];
        assert_eq!(
            security,
            &json!([{"oauth2": [scope]}, {"personalAccessToken": [scope]}]),
            "{} ({method} {path})",
            f.sig.ident
        );
        checked += 1;
    }
    assert_eq!(checked, documented(&doc).len() - 1, "all but /openapi.json");
}

/// Whether `value` matches `schema` of `doc`, strictly: an object carries
/// exactly the properties its schema lists (no undocumented field) and every
/// required one.
fn conforms(doc: &Value, value: &Value, schema: &Value, at: &str) -> Result<(), String> {
    if let Some(Value::String(r)) = schema.get("$ref") {
        let name = r.trim_start_matches("#/components/schemas/");
        return conforms(doc, value, &doc["components"]["schemas"][name], at);
    }
    if let Some(Value::Array(variants)) = schema.get("oneOf") {
        let errors: Vec<String> = variants
            .iter()
            .filter_map(|v| conforms(doc, value, v, at).err())
            .collect();
        return if errors.len() == variants.len() - 1 {
            Ok(())
        } else {
            Err(format!("{at}: one variant of oneOf must match: {errors:?}"))
        };
    }
    if let Some(Value::Array(variants)) = schema.get("anyOf") {
        let errors: Vec<String> = variants
            .iter()
            .filter_map(|v| conforms(doc, value, v, at).err())
            .collect();
        return if errors.len() < variants.len() {
            Ok(())
        } else {
            Err(format!("{at}: no variant of anyOf matches: {errors:?}"))
        };
    }
    if let Some(Value::Array(values)) = schema.get("enum") {
        if !values.contains(value) {
            return Err(format!("{at}: {value} not in {values:?}"));
        }
    }
    let types: Vec<&str> = match &schema["type"] {
        Value::String(t) => vec![t.as_str()],
        Value::Array(ts) => ts.iter().filter_map(Value::as_str).collect(),
        _ => vec![],
    };
    let actual = match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    };
    let allowed = types.is_empty()
        || types.contains(&actual)
        || (actual == "integer" && types.contains(&"number"));
    if !allowed {
        return Err(format!("{at}: {actual}, expected {types:?}"));
    }
    match value {
        Value::Object(map) => {
            let properties = schema["properties"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            // A map (`additionalProperties` a schema) takes any key.
            let additional = &schema["additionalProperties"];
            for key in map.keys() {
                if !properties.contains_key(key) && !additional.is_object() {
                    return Err(format!("{at}.{key}: not in the schema"));
                }
            }
            for required in schema["required"].as_array().into_iter().flatten() {
                if !map.contains_key(required.as_str().unwrap()) {
                    return Err(format!("{at}.{required}: required, missing"));
                }
            }
            for (key, item) in map {
                let item_schema = properties.get(key).unwrap_or(additional);
                conforms(doc, item, item_schema, &format!("{at}.{key}"))?;
            }
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                conforms(doc, item, &schema["items"], &format!("{at}[{i}]"))?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Check an answer against the schema the document gives for it.
fn check(doc: &Value, path: &str, method: &str, status: StatusCode, body: &Value) {
    let schema = &doc["paths"][path][method]["responses"][status.as_str()]["content"]
        ["application/json"]["schema"];
    assert!(
        !schema.is_null(),
        "{method} {path} documents {status}: {body}"
    );
    if let Err(e) = conforms(doc, body, schema, "answer") {
        panic!("{method} {path} {status}: {e}\n{body}");
    }
}

async fn send(app: &Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = body_string(response).await;
    (status, serde_json::from_str(&body).unwrap_or(Value::Null))
}

#[tokio::test]
async fn answers_match_the_documented_schemas() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, pool, _, et_id) = setup_test_app().await;
    let doc = document(&app).await;
    sqlx::query("UPDATE caldav_sources SET last_synced = datetime('now')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE event_types SET max_additional_guests = 2 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let owner = user_id(&pool, "testuser").await;
    let token = pat(&pool, &owner, ALL).await;
    let write_only = pat(&pool, &owner, "bookings:write").await;
    // Booking questions (#210) of every shape the API shows: a choice with
    // its option ids, a consent with its policy link, a text with no link.
    let questions = json!([
        {"kind": "single_choice", "label": "Where are the tyres", "options": [{"label": "At the shop"}, {"label": "I bring them"}]},
        {"kind": "checkbox", "label": "Privacy", "consent": true, "required": true, "link": "https://example.com/privacy", "help": "Read it first"},
        {"kind": "short_text", "label": "Plate"}
    ]);
    let questions = crate::web::questions::parse_editor(&questions.to_string(), &[]).unwrap();
    crate::web::questions::save(&pool, &et_id, Some(&questions), None)
        .await
        .unwrap();

    let (status, body) = send(&app, api_get("/api/v1/event-types", &token)).await;
    check(&doc, "/event-types", "get", status, &body);
    assert_eq!(status, StatusCode::OK);
    let uri = format!("/api/v1/event-types/{et_id}");
    let (status, body) = send(&app, api_get(&uri, &token)).await;
    assert_eq!(status, StatusCode::OK);
    check(&doc, "/event-types/{id}", "get", status, &body);
    assert_eq!(body["event_type"]["questions"][0]["options"][1]["id"], "o2");
    assert_eq!(body["event_type"]["questions"][1]["help"], "Read it first");
    let day = far_future_monday();
    let uri =
        format!("/api/v1/event-types/{et_id}/slots?from={day}&to={day}&timezone=Europe/Paris");
    let (status, body) = send(&app, api_get(&uri, &token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body["slots"].as_array().unwrap().is_empty());
    check(&doc, "/event-types/{id}/slots", "get", status, &body);

    let mut create = create_body(&et_id, &monday_at("11:00"), "schema@test.com");
    create["attendees"] = json!(["other@test.com"]);
    create["notes"] = json!("About the schema");
    create["answers"] = json!({"where_are_the_tyres": "o2", "privacy": true, "plate": "AB-123"});
    // The request itself matches the documented body, answers included.
    let request_schema =
        &doc["paths"]["/bookings"]["post"]["requestBody"]["content"]["application/json"]["schema"];
    if let Err(e) = conforms(&doc, &create, request_schema, "request") {
        panic!("POST /bookings request: {e}");
    }
    let answers = &doc["components"]["schemas"]["CreateBooking"]["properties"]["answers"];
    assert_eq!(answers["type"], json!(["object", "null"]));
    assert_eq!(
        answers["additionalProperties"]["oneOf"]
            .as_array()
            .map(Vec::len),
        Some(3),
        "{answers}"
    );
    let mut absent = create.clone();
    absent["answers"] = Value::Null;
    assert_eq!(conforms(&doc, &absent, request_schema, "request"), Ok(()));
    let one_of = "one variant of oneOf must match";
    for (wrong, at) in [
        (
            json!({"plate": 5}),
            format!("request.answers.plate: {one_of}"),
        ),
        (
            json!({"plate": null}),
            format!("request.answers.plate: {one_of}"),
        ),
        (
            json!({"topics": [1]}),
            format!("request.answers.topics: {one_of}"),
        ),
        (
            json!("AB-123"),
            "request.answers: string, expected".to_string(),
        ),
    ] {
        let mut body = create.clone();
        body["answers"] = wrong;
        let error = conforms(&doc, &body, request_schema, "request").unwrap_err();
        assert!(error.starts_with(&at), "{error}");
    }
    // The array branch is refused on its item, not on its type.
    let mut body = create.clone();
    body["answers"] = json!({"topics": [1]});
    let error = conforms(&doc, &body, request_schema, "request").unwrap_err();
    assert!(
        error.contains("request.answers.topics[0]: integer"),
        "{error}"
    );
    let (status, body) = send(&app, api_post("/api/v1/bookings", &token, &create)).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    check(&doc, "/bookings", "post", status, &body);
    assert_eq!(body["booking"]["answers"][2]["value"], "AB-123");
    let id = body["booking"]["id"].as_str().unwrap().to_string();
    let (status, body) = send(&app, api_get("/api/v1/bookings", &token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body["bookings"].as_array().unwrap().is_empty());
    check(&doc, "/bookings", "get", status, &body);
    let (status, body) = send(&app, api_get(&format!("/api/v1/bookings/{id}"), &token)).await;
    assert_eq!(status, StatusCode::OK);
    check(&doc, "/bookings/{id}", "get", status, &body);
    assert_eq!(body["booking"]["answers"][0]["option_ids"], json!(["o2"]));
    assert_eq!(
        body["booking"]["answers"][1]["link_url"],
        "https://example.com/privacy"
    );
    assert_eq!(body["booking"]["answers"][2]["link_url"], Value::Null);

    // A write-only token gets the booking's id and status only.
    let uri = format!("/api/v1/bookings/{id}/request-reschedule");
    let (status, body) = send(&app, api_post(&uri, &write_only, &json!({}))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["booking"].as_object().unwrap().len(), 2);
    check(
        &doc,
        "/bookings/{id}/request-reschedule",
        "post",
        status,
        &body,
    );
    let uri = format!("/api/v1/bookings/{id}/cancel");
    let (status, body) = send(&app, api_post(&uri, &token, &json!({"reason": "x"}))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    check(&doc, "/bookings/{id}/cancel", "post", status, &body);

    // Errors, each under the status the document gives it.
    let (status, body) = send(&app, api_post(&uri, &token, &json!({}))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    check(&doc, "/bookings/{id}/cancel", "post", status, &body);
    let (status, body) = send(&app, api_get("/api/v1/bookings?nope=1", &token)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    check(&doc, "/bookings", "get", status, &body);
    let (status, body) = send(&app, get("/api/v1/bookings")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    check(&doc, "/bookings", "get", status, &body);
    let read_only = pat(&pool, &owner, "bookings:read").await;
    let (status, body) = send(&app, api_post("/api/v1/bookings", &read_only, &create)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    check(&doc, "/bookings", "post", status, &body);
}

#[tokio::test]
async fn every_error_code_is_documented_under_a_status() {
    // The `ErrorCode` schema is the enum the handlers construct, so every
    // code is in it; each must also be named in some answer's description,
    // so a reader finds under which status it comes.
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, _, _, _) = setup_test_app().await;
    let doc = document(&app).await;
    let codes = doc["components"]["schemas"]["ErrorCode"]["enum"]
        .as_array()
        .unwrap();
    assert!(codes.len() >= 30);
    let mut described = String::new();
    for (method, path) in documented(&doc) {
        for answer in doc["paths"][&path][&method]["responses"]
            .as_object()
            .unwrap()
            .values()
        {
            described.push_str(answer["description"].as_str().unwrap());
        }
    }
    described.push_str(doc["info"]["description"].as_str().unwrap());
    for code in codes {
        let code = code.as_str().unwrap();
        assert!(
            described.contains(&format!("`{code}`")),
            "{code} is described"
        );
    }
}

// --- The interactive reference ---

#[tokio::test]
async fn api_docs_needs_a_signed_in_user() {
    let (app, _, _, _) = setup_test_app().await;
    let response = app.oneshot(get("/api/docs")).await.unwrap();
    assert!(response.status().is_redirection());
    let location = response
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.starts_with("/auth/login"), "{location}");
}

#[tokio::test]
async fn api_docs_renders_the_vendored_viewer_without_the_session() {
    let (app, pool, session, _) = setup_test_app().await;
    let response = app
        .clone()
        .oneshot(get_authed("/api/docs", &session))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    // The baseline policy: no 'unsafe-eval', scripts from this origin only.
    let csp = response
        .headers()
        .get("content-security-policy")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(!csp.contains("unsafe-eval"), "{csp}");
    assert!(csp.contains("script-src 'self' 'unsafe-inline';"), "{csp}");
    assert!(csp.contains("connect-src 'self';"), "{csp}");
    let body = body_string(response).await;
    let version = super::super::api_doc::SWAGGER_UI_VERSION;
    assert!(body.contains(&format!(
        r#"<script src="/static/swagger-ui/swagger-ui-bundle.js?v={version}"></script>"#
    )));
    assert!(body.contains(&format!(
        r#"href="/static/swagger-ui/swagger-ui.css?v={version}""#
    )));
    assert!(body.contains("fetch('/api/v1/openapi.json', { credentials: 'omit' })"));
    // Authorize offers the personal token only: the page cannot complete
    // the OAuth flow, which the served document still describes.
    assert!(body.contains("delete schemes.oauth2;"));
    assert!(body.contains("request.credentials = 'omit'"));
    assert!(body.contains("validatorUrl: null"));
    assert!(body.contains("persistAuthorization: false"));
    // Nothing loaded from another origin.
    for tag in ["<script src=\"http", "<link rel=\"stylesheet\" href=\"http"] {
        assert!(!body.contains(tag), "{tag}");
    }
    assert!(body.contains(&crate::i18n::translate("en", "api-docs-try", None)));
    // The test user is an admin: the API is open to them.
    assert!(!body.contains(&crate::i18n::translate("en", "api-docs-closed", None)));

    // A user the API is closed to still reads the reference, and is told.
    let (_, user_session) =
        insert_test_user_with_session(&pool, "Bob", "bob@test.com", "bob").await;
    let response = app
        .clone()
        .oneshot(get_authed("/api/docs", &user_session))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_string(response).await;
    assert!(body.contains(&crate::i18n::translate("en", "api-docs-closed", None)));

    // In the reader's language.
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/docs")
                .header("cookie", format!("__Host-calrs_session={session}"))
                .header("accept-language", "fr")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = body_string(response).await;
    assert!(body.contains("Référence de l&#x27;API") || body.contains("Référence de l'API"));
}

#[tokio::test]
async fn swagger_ui_assets_are_served_from_an_allowlist() {
    let (app, _, _, _) = setup_test_app().await;
    for (file, content_type, size) in [
        (
            "swagger-ui-bundle.js",
            "application/javascript",
            1_000_000usize,
        ),
        ("swagger-ui.css", "text/css", 100_000),
    ] {
        let response = app
            .clone()
            .oneshot(get(&format!("/static/swagger-ui/{file}?v=1")))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{file}");
        let served = response.headers().get("content-type").unwrap();
        assert!(served.to_str().unwrap().starts_with(content_type));
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        assert!(bytes.len() > size, "{file}");
    }
    for probe in [
        "LICENSE",
        "README.md",
        "swagger-ui-bundle.js.map",
        "Swagger-UI.css",
        "..%2F..%2FCargo.toml",
        "%2Fetc%2Fpasswd",
    ] {
        let response = app
            .clone()
            .oneshot(get(&format!("/static/swagger-ui/{probe}")))
            .await
            .unwrap();
        assert_ne!(response.status(), StatusCode::OK, "{probe}");
    }
}

#[test]
fn vendored_swagger_ui_keeps_its_licences_and_pinned_contents() {
    use sha2::{Digest, Sha256};
    let licence = include_str!("../../../assets/swagger-ui/LICENSE");
    assert!(licence.contains("Apache License") && licence.contains("Version 2.0"));
    let notice = include_str!("../../../assets/swagger-ui/NOTICE");
    assert!(notice.contains("SmartBear Software"));
    let bundled = include_str!("../../../assets/swagger-ui/swagger-ui-bundle.js.LICENSE.txt");
    assert!(bundled.contains("DOMPurify") && bundled.contains("React"));
    let bundle: &[u8] = include_bytes!("../../../assets/swagger-ui/swagger-ui-bundle.js");
    assert!(bundle.starts_with(
        b"/*! For license information please see swagger-ui-bundle.js.LICENSE.txt */"
    ));
    let readme = include_str!("../../../assets/swagger-ui/README.md");
    let version = super::super::api_doc::SWAGGER_UI_VERSION;
    assert!(readme.contains(&format!("Version: **{version}**")));
    // The files are upstream's, byte for byte: a change is reviewed and the
    // hashes updated here and in the README.
    for (bytes, expected) in [
        (
            bundle,
            "36eeb4d5640b9c036b1d0f854499af1cceebbdb260b790a03a18be5e8488fbf5",
        ),
        (
            &include_bytes!("../../../assets/swagger-ui/swagger-ui.css")[..],
            "c3cfe8248c4b0b89673600fb829924eafbac6de4bc4b2b06f5f5243483074d0a",
        ),
    ] {
        assert_eq!(hex::encode(Sha256::digest(bytes)), expected);
        assert!(readme.contains(expected));
    }
}

/// A booking refused because a busy time could not be read (it fails
/// closed) answers the documented 500 error envelope.
#[tokio::test]
async fn a_fail_closed_booking_answers_the_documented_error() {
    let _lock = crate::test_support::ENV_LOCK.lock().await;
    let _env = super::oauth::environment();
    let (app, pool, _, et_id) = setup_test_app().await;
    let doc = document(&app).await;
    let owner = user_id(&pool, "testuser").await;
    let token = pat(&pool, &owner, ALL).await;
    sqlx::query("ALTER TABLE events RENAME TO events_gone")
        .execute(&pool)
        .await
        .unwrap();
    let create = create_body(&et_id, &monday_at("10:00"), "closed@test.com");
    let (status, body) = send(&app, api_post("/api/v1/bookings", &token, &create)).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert_eq!(body["error"]["code"], "internal_error");
    check(&doc, "/bookings", "post", status, &body);
}
