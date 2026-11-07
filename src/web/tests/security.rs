//! Rate limiting, CSRF, client IP, error sanitization, CSP and embed detection.

use super::*;

/// Search engines must see noindex on dated booking pages as well as
/// redirects and errors; blocking crawling would hide it from them (#327).
#[tokio::test]
async fn search_indexing_disabled_on_pages_redirects_and_errors() {
    let (app, _, _, _) = setup_test_app().await;
    let book_uri = booking_form_uri(None);
    for (uri, status) in [
        ("/u/testuser", 200),
        ("/u/testuser/test-meeting", 200),
        (
            "/u/testuser/test-meeting?month=2026-11&tz=Europe%2FParis",
            200,
        ),
        (book_uri.as_str(), 200),
        ("/test-meeting?month=2026-11", 200),
        ("/u/testuser/test-meeting?embed=1", 200),
        ("/auth/login", 200),
        ("/u/testuser/", 308),
        ("/u/testuser/test-meeting/book", 400),
        ("/no/such/route", 404),
    ] {
        let response = app.clone().oneshot(get(uri)).await.unwrap();
        assert_eq!(response.status(), status, "{uri}");
        assert_eq!(
            response.headers().get("x-robots-tag").unwrap(),
            "noindex, nofollow",
            "{uri}"
        );
        if status == 200 {
            let html = body_string(response).await;
            let head = html.split("</head>").next().unwrap();
            assert!(
                head.contains(r#"<meta name="robots" content="noindex, nofollow">"#),
                "missing robots meta tag in head: {uri}"
            );
        }
    }

    for (method, status) in [("HEAD", 200), ("DELETE", 405)] {
        let request = axum::http::Request::builder()
            .method(method)
            .uri("/u/testuser/test-meeting")
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), status, "{method}");
        assert_eq!(
            response.headers().get("x-robots-tag").unwrap(),
            "noindex, nofollow",
            "{method}"
        );
        if method == "HEAD" {
            assert!(body_string(response).await.is_empty());
        }
    }
}

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

#[tokio::test]
async fn rate_limiter_sweeps_expired_entries() {
    // Pin that periodic eviction actually drops entries from the map so
    // it cannot grow without bound under traffic from many distinct IPs.
    let limiter = RateLimiter::new(5, 0); // 0-second window: immediate expiry
    for i in 0..50 {
        limiter.check_limited(&format!("ip-{}", i)).await;
    }
    // Next call triggers a sweep; all prior entries should be evicted
    // before the new key is inserted.
    limiter.check_limited("trigger-sweep").await;
    let state = limiter.state.lock().await;
    assert_eq!(
        state.attempts.len(),
        1,
        "expired entries should be swept, only the newest survives"
    );
}

// --- CSRF function tests ---

#[test]
fn generate_csrf_token_returns_valid_uuid() {
    let token = generate_csrf_token();
    assert!(!token.is_empty());
    assert_eq!(token.len(), 36); // UUID format: 8-4-4-4-12
}

#[test]
fn csrf_cookie_has_security_flags() {
    // `Secure` so the token never travels over plaintext HTTP.
    // `SameSite=Lax` to stop cross-site GETs from carrying it.
    // Deliberately NO `HttpOnly` — the double-submit pattern needs the
    // client script in base.html to read the cookie via document.cookie.
    let cookie = csrf_cookie_value("some-token");
    assert!(
        cookie.contains("; Secure"),
        "CSRF cookie must have Secure flag: {}",
        cookie
    );
    assert!(
        cookie.contains("SameSite=Lax"),
        "CSRF cookie must have SameSite=Lax: {}",
        cookie
    );
    assert!(
        !cookie.contains("HttpOnly"),
        "CSRF cookie must NOT be HttpOnly (JS needs to read it): {}",
        cookie
    );
}

#[test]
fn csrf_cookie_embed_uses_samesite_none() {
    let cookie = csrf_cookie_value_for("tok", true);
    assert!(
        cookie.contains("SameSite=None"),
        "embed CSRF cookie must use SameSite=None for third-party iframes: {}",
        cookie
    );
    assert!(
        cookie.contains("; Secure"),
        "embed CSRF cookie must still carry Secure: {}",
        cookie
    );
    assert!(
        cookie.contains("; Partitioned"),
        "embed CSRF cookie must be Partitioned (CHIPS) for browsers blocking unpartitioned 3P cookies: {}",
        cookie
    );
}

#[test]
fn csrf_cookie_non_embed_is_samesite_lax_unpartitioned() {
    let cookie = csrf_cookie_value_for("tok", false);
    assert!(cookie.contains("SameSite=Lax"), "{}", cookie);
    assert!(
        !cookie.contains("Partitioned"),
        "non-embed cookie must not be Partitioned: {}",
        cookie
    );
}

#[test]
fn is_embed_request_detects_flag() {
    assert!(is_embed_request(Some("embed=1")));
    assert!(is_embed_request(Some("embed=true")));
    assert!(is_embed_request(Some("foo=bar&embed=1&baz=qux")));
    assert!(!is_embed_request(Some("embed=0")));
    assert!(!is_embed_request(Some("notembed=1")));
    assert!(!is_embed_request(None));
    assert!(!is_embed_request(Some("")));
}

#[test]
fn is_embeddable_path_allows_booking_surfaces_only() {
    // Public booking surfaces — embeddable.
    assert!(is_embeddable_path("/u/alice"));
    assert!(is_embeddable_path("/u/alice/intro"));
    assert!(is_embeddable_path("/u/alice/intro/book"));
    assert!(is_embeddable_path("/team/sales"));
    assert!(is_embeddable_path("/team/sales/intro"));
    assert!(is_embeddable_path("/g/sales"));
    // Legacy single-user booking pages.
    assert!(is_embeddable_path("/intro"));
    assert!(is_embeddable_path("/intro/book"));

    // Sensitive / non-booking surfaces — never embeddable.
    assert!(!is_embeddable_path("/auth/login"));
    assert!(!is_embeddable_path("/auth/register"));
    assert!(!is_embeddable_path("/dashboard"));
    assert!(!is_embeddable_path("/dashboard/event-types"));
    assert!(!is_embeddable_path("/dashboard/admin"));
    assert!(!is_embeddable_path("/"));
    assert!(!is_embeddable_path("/embed.js"));
    assert!(!is_embeddable_path("/accent.css"));
    assert!(!is_embeddable_path("/favicon"));
    assert!(!is_embeddable_path("/favicon.ico"));
    assert!(!is_embeddable_path("/avatar/123"));
    assert!(!is_embeddable_path("/booking/approve/tok"));
}

#[test]
fn embedded_booking_request_requires_both_flag_and_path() {
    // The blocking issue: embed=1 on a sensitive path must NOT enable embed.
    assert!(!is_embedded_booking_request("/auth/login", Some("embed=1")));
    assert!(!is_embedded_booking_request("/dashboard", Some("embed=1")));
    // Booking surface with the flag — enabled.
    assert!(is_embedded_booking_request(
        "/u/alice/intro",
        Some("embed=1")
    ));
    // Booking surface without the flag — not embed mode.
    assert!(!is_embedded_booking_request("/u/alice/intro", None));
}

#[test]
fn csrf_token_from_headers_extracts_token() {
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::COOKIE,
        "__Host-calrs_csrf=test-token-123".parse().unwrap(),
    );
    let token = csrf_token_from_headers(&headers);
    assert_eq!(token, Some("test-token-123".to_string()));
}

#[test]
fn csrf_token_from_headers_returns_none_when_missing() {
    let headers = HeaderMap::new();
    let token = csrf_token_from_headers(&headers);
    assert_eq!(token, None);
}

#[test]
fn csrf_token_from_headers_ignores_other_cookies() {
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::COOKIE,
        "__Host-calrs_session=abc; other=xyz".parse().unwrap(),
    );
    let token = csrf_token_from_headers(&headers);
    assert_eq!(token, None);
}

#[test]
fn verify_csrf_token_passes_when_matching() {
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::COOKIE,
        "__Host-calrs_csrf=my-token".parse().unwrap(),
    );
    let form_token = Some("my-token".to_string());
    assert!(verify_csrf_token(&headers, &form_token).is_ok());
}

#[test]
fn verify_csrf_token_fails_when_mismatch() {
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::COOKIE,
        "__Host-calrs_csrf=cookie-token".parse().unwrap(),
    );
    let form_token = Some("different-token".to_string());
    assert!(verify_csrf_token(&headers, &form_token).is_err());
}

#[test]
fn verify_csrf_token_fails_when_form_token_none() {
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::COOKIE,
        "__Host-calrs_csrf=my-token".parse().unwrap(),
    );
    let form_token: Option<String> = None;
    assert!(verify_csrf_token(&headers, &form_token).is_err());
}

#[test]
fn verify_csrf_token_fails_when_cookie_missing() {
    let headers = HeaderMap::new();
    let form_token = Some("my-token".to_string());
    assert!(verify_csrf_token(&headers, &form_token).is_err());
}

#[test]
fn verify_csrf_token_fails_when_cookie_empty() {
    // An empty cookie value would byte-compare-equal to an empty form
    // field; the explicit empty-cookie guard prevents that bypass.
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::COOKIE,
        "__Host-calrs_csrf=".parse().unwrap(),
    );
    let form_token = Some(String::new());
    assert!(verify_csrf_token(&headers, &form_token).is_err());
}

#[test]
fn verify_csrf_token_fails_when_lengths_differ() {
    // ConstantTimeEq returns false on length mismatch; this just pins
    // that mismatched-length tokens are still rejected.
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::COOKIE,
        "__Host-calrs_csrf=short".parse().unwrap(),
    );
    let form_token = Some("a-much-longer-token".to_string());
    assert!(verify_csrf_token(&headers, &form_token).is_err());
}

#[test]
fn verify_csrf_token_fails_when_same_length_differs_at_last_byte() {
    // Exercises the actual ct_eq byte-fold path: both tokens are the
    // same length, so the comparison runs over every byte rather than
    // short-circuiting on the length mismatch.
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::COOKIE,
        "__Host-calrs_csrf=aaaaaaaaaaaaaaaa".parse().unwrap(),
    );
    let form_token = Some("aaaaaaaaaaaaaaab".to_string());
    assert!(verify_csrf_token(&headers, &form_token).is_err());
}

// --- client_ip_for_rate_limit ---

#[test]
fn client_ip_unknown_when_no_headers() {
    let headers = HeaderMap::new();
    assert_eq!(client_ip_for_rate_limit(&headers), "unknown");
}

#[test]
fn client_ip_ignores_x_real_ip() {
    // We deliberately do not honour X-Real-IP: in default Caddy/Nginx
    // setups the proxy does not overwrite a client-supplied value, so
    // trusting it would let the client forge their identity directly.
    let mut headers = HeaderMap::new();
    headers.insert("x-real-ip", "1.2.3.4".parse().unwrap());
    assert_eq!(client_ip_for_rate_limit(&headers), "unknown");
}

#[test]
fn client_ip_takes_rightmost_xff() {
    // The leftmost value is whatever the original client put in the
    // header; the rightmost is the trusted proxy's view of its peer.
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-forwarded-for",
        "attacker-spoofed, 198.51.100.7".parse().unwrap(),
    );
    assert_eq!(client_ip_for_rate_limit(&headers), "198.51.100.7");
}

#[test]
fn client_ip_xff_single_value() {
    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", "203.0.113.10".parse().unwrap());
    assert_eq!(client_ip_for_rate_limit(&headers), "203.0.113.10");
}

#[test]
fn client_ip_xff_trims_whitespace() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-forwarded-for",
        "1.2.3.4 ,    198.51.100.7   ".parse().unwrap(),
    );
    assert_eq!(client_ip_for_rate_limit(&headers), "198.51.100.7");
}

// --- internal_error_* sanitization helpers ---

#[test]
fn internal_error_response_returns_500_with_generic_body() {
    // Whatever the underlying error says, the response body must not
    // contain it. The detail goes to logs only.
    let resp = internal_error_response("test ctx", &"sensitive: /var/lib/calrs/calrs.db error");
    assert_eq!(resp.status(), axum::http::StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn internal_error_html_body_is_generic() {
    let html = internal_error_html("test", &"users.password_hash leaks here");
    let Html(body) = html;
    assert!(
        !body.contains("password_hash"),
        "body leaked detail: {}",
        body
    );
    assert!(!body.contains("users."), "body leaked detail: {}", body);
    assert!(body.contains("internal error"));
}

#[test]
fn internal_error_body_is_generic() {
    let body = internal_error_body("test", &"some/private/path: nope");
    assert!(!body.contains("/private/path"), "body leaked: {body}");
}

#[test]
fn oidc_error_response_returns_500_without_detail() {
    // openidconnect errors can carry IdP URLs and token content; the
    // response body must not pass them through.
    let resp = oidc_error_response(
        "test",
        &"token endpoint https://idp/token returned: <secret-token-bytes>",
    );
    assert_eq!(resp.status(), axum::http::StatusCode::INTERNAL_SERVER_ERROR);
}

/// Capture `tracing` output into a shared buffer so log-emission can be
/// asserted from a test. Used to pin the contract that the sanitization
/// helpers actually log the underlying detail; a future refactor that
/// drops the log call would cause this test to fail.
///
/// Uses a hand-rolled minimal `Subscriber` instead of
/// `tracing_subscriber::fmt`. The fmt layer's per-event writer plumbing
/// proved flaky under `cargo tarpaulin` (events occasionally landed in a
/// dropped buffer), and we don't need the formatting machinery here:
/// recording field names + their `Debug` strings is enough to assert that
/// the event carried the expected markers.
fn capture_tracing<F: FnOnce()>(f: F) -> String {
    use std::sync::{Arc, Mutex};
    use tracing::field::{Field, Visit};
    use tracing::{Event, Metadata, Subscriber};

    #[derive(Clone)]
    struct Buf(Arc<Mutex<String>>);

    struct Recorder {
        buf: Arc<Mutex<String>>,
    }
    impl Subscriber for Recorder {
        fn enabled(&self, _: &Metadata<'_>) -> bool {
            true
        }
        fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            tracing::span::Id::from_u64(1)
        }
        fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
        fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
        fn event(&self, event: &Event<'_>) {
            struct V<'a>(&'a Mutex<String>);
            impl<'a> Visit for V<'a> {
                fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
                    use std::fmt::Write;
                    let mut s = self.0.lock().unwrap();
                    let _ = write!(s, "{}={:?} ", field.name(), value);
                }
                fn record_str(&mut self, field: &Field, value: &str) {
                    use std::fmt::Write;
                    let mut s = self.0.lock().unwrap();
                    let _ = write!(s, "{}={} ", field.name(), value);
                }
            }
            event.record(&mut V(&self.buf));
            self.buf.lock().unwrap().push('\n');
        }
        fn enter(&self, _: &tracing::span::Id) {}
        fn exit(&self, _: &tracing::span::Id) {}
    }

    let buf = Buf(Arc::new(Mutex::new(String::new())));
    let subscriber = Recorder { buf: buf.0.clone() };
    tracing::subscriber::with_default(subscriber, || {
        // tracing caches per-callsite Interest the first time the
        // callsite fires. If a sibling test (e.g. one that just
        // exercises the response body without capturing logs) ran
        // first under NoSubscriber, the cached interest is `never` and
        // our scoped Recorder never sees the event. Force a rebuild
        // against the now-active subscriber before invoking f.
        tracing::callsite::rebuild_interest_cache();
        f();
    });
    let s = buf.0.lock().unwrap().clone();
    s
}

/// Whether the current process is running under cargo-tarpaulin. The
/// tracing-capture tests below race with tarpaulin's instrumentation in
/// a way I haven't fully pinned down: the captured buffer ends up empty
/// even though the same test passes deterministically under plain
/// `cargo test`. Skip those tests under coverage rather than ship a
/// red CI job, but keep them live everywhere else so the contract is
/// still enforced.
///
/// Detection uses `cfg!(tarpaulin)` since cargo-tarpaulin compiles with
/// `--cfg=tarpaulin`. An earlier attempt keyed off `CARGO_TARPAULIN_VERSION`
/// but tarpaulin never sets that env var, so the guard never fired.
fn under_tarpaulin() -> bool {
    cfg!(tarpaulin)
}

#[test]
fn internal_error_logs_capture_underlying_detail() {
    if under_tarpaulin() {
        return;
    }
    // Pin: the helper must emit a tracing::error! that carries both the
    // context label and the underlying error string. The body sent to
    // the user is generic; the detail goes here.
    let log = capture_tracing(|| {
        let _ = internal_error_response("test-context-marker", &"sensitive-detail-marker");
    });
    assert!(
        log.contains("test-context-marker"),
        "log missing context: {log}"
    );
    assert!(
        log.contains("sensitive-detail-marker"),
        "log missing error detail: {log}"
    );
}

#[test]
fn oidc_error_logs_capture_underlying_detail() {
    if under_tarpaulin() {
        return;
    }
    let log = capture_tracing(|| {
        let _ = oidc_error_response("oidc-context-marker", &"token-endpoint-detail-marker");
    });
    assert!(
        log.contains("oidc-context-marker"),
        "log missing context: {log}"
    );
    assert!(
        log.contains("token-endpoint-detail-marker"),
        "log missing error detail: {log}"
    );
}

#[test]
fn oidc_error_with_source_logs_the_cause_chain() {
    // oauth2 errors display a fixed sentence and keep the reason in
    // `source()` (#264); the log must carry the whole chain.
    #[derive(Debug)]
    struct Outer(std::io::Error);
    impl std::fmt::Display for Outer {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("outer-display-marker")
        }
    }
    impl std::error::Error for Outer {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.0)
        }
    }
    let err = Outer(std::io::Error::other("inner-cause-marker"));
    assert_eq!(error_source_chain(&err), "inner-cause-marker");
    if under_tarpaulin() {
        return;
    }
    let mut status = None;
    let log = capture_tracing(|| {
        status = Some(oidc_error_response_with_source("oidc-ctx", &err).status());
    });
    assert_eq!(status, Some(axum::http::StatusCode::INTERNAL_SERVER_ERROR));
    assert!(log.contains("outer-display-marker"), "log: {log}");
    assert!(log.contains("inner-cause-marker"), "log: {log}");
}

// --- POST handler tests ---

#[tokio::test]
async fn post_without_csrf_returns_403() {
    let (app, _, session, _) = setup_test_app().await;
    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/dashboard/event-types/test-meeting/toggle")
        .header("cookie", format!("__Host-calrs_session={}", session))
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from("_csrf=wrong-token"))
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        403,
        "Missing/mismatched CSRF should return 403"
    );
}

// --- build_csp tests ---

fn make_captcha_config(instance_url: &str, widget_url: &str) -> captcha::CaptchaConfig {
    captcha::CaptchaConfig {
        instance_url: instance_url.to_string(),
        site_key: "testkey".to_string(),
        secret: "secret".to_string(),
        widget_url: widget_url.to_string(),
        wasm_url: None,
    }
}

#[test]
fn build_csp_with_custom_wasm_url_adds_its_origin_to_connect_src() {
    let mut cfg = make_captcha_config("https://cap.example.com", captcha::DEFAULT_WIDGET_URL);
    cfg.wasm_url = Some("https://assets.example.org/cap/cap_wasm_bg.wasm".to_string());
    let with_wasm = build_csp(&Some(cfg));
    let connect = with_wasm
        .split("; ")
        .find(|d| d.starts_with("connect-src"))
        .expect("connect-src directive");
    assert_eq!(
        connect,
        "connect-src 'self' https://cap.example.com https://cdn.jsdelivr.net https://assets.example.org"
    );
    let script = with_wasm
        .split("; ")
        .find(|d| d.starts_with("script-src"))
        .expect("script-src directive");
    assert!(
        !script.contains("assets.example.org"),
        "the WASM origin must not be allowed to serve scripts: {script}"
    );

    // Everything else is the policy without a WASM URL plus that origin.
    let without = build_csp(&Some(make_captcha_config(
        "https://cap.example.com",
        captcha::DEFAULT_WIDGET_URL,
    )));
    assert_eq!(
        with_wasm.replace(" https://assets.example.org", ""),
        without
    );
}

#[test]
fn build_csp_does_not_repeat_a_wasm_origin_already_allowed() {
    let mut cfg = make_captcha_config("https://cap.example.com", captcha::DEFAULT_WIDGET_URL);
    cfg.wasm_url = Some("https://cap.example.com/assets/cap_wasm_bg.wasm".to_string());
    let csp = build_csp(&Some(cfg));
    assert_eq!(csp.matches("https://cap.example.com").count(), 1, "{csp}");
}

#[test]
fn build_csp_without_captcha_has_no_extra_directives() {
    let csp = build_csp(&None);
    assert!(!csp.contains("wasm-unsafe-eval"));
    assert!(!csp.contains("worker-src"));
    assert!(!csp.contains("cdn.jsdelivr.net"));
    assert!(csp.contains("script-src 'self' 'unsafe-inline'"));
    assert!(csp.contains("connect-src 'self'"));
}

#[test]
fn build_csp_with_captcha_includes_wasm_and_worker_src() {
    let cfg = Some(make_captcha_config(
        "https://cap.example.com",
        captcha::DEFAULT_WIDGET_URL,
    ));
    let csp = build_csp(&cfg);
    assert!(csp.contains("'wasm-unsafe-eval'"));
    assert!(csp.contains("worker-src blob:"));
}

#[test]
fn build_csp_with_captcha_includes_script_origin() {
    let cfg = Some(make_captcha_config(
        "https://cap.example.com",
        captcha::DEFAULT_WIDGET_URL,
    ));
    let csp = build_csp(&cfg);
    assert!(csp.contains("https://cdn.jsdelivr.net"), "{}", csp);
}

#[test]
fn booking_form_paths_get_relaxed_csp() {
    assert!(is_booking_form_path("/u/alice/intro/book"));
    assert!(is_booking_form_path("/team/sales/demo/book"));
    assert!(is_booking_form_path("/intro/book"));
}

#[test]
fn non_booking_paths_keep_baseline_csp() {
    assert!(!is_booking_form_path("/"));
    assert!(!is_booking_form_path("/dashboard"));
    assert!(!is_booking_form_path("/dashboard/admin"));
    assert!(!is_booking_form_path("/u/alice/intro"));
    assert!(!is_booking_form_path("/booking/cancel/sometoken"));
    assert!(!is_booking_form_path("/u/alice/bookkeeping"));
}

#[test]
fn build_csp_with_captcha_includes_connect_origins() {
    let cfg = Some(make_captcha_config(
        "https://cap.example.com",
        captcha::DEFAULT_WIDGET_URL,
    ));
    let csp = build_csp(&cfg);
    assert!(csp.contains("connect-src 'self'"), "{}", csp);
    assert!(csp.contains("https://cap.example.com"), "{}", csp);
    assert!(csp.contains("https://cdn.jsdelivr.net"), "{}", csp);
}

// Embed mode must preserve captcha allowances (so captcha-enabled
// deployments can still serve a working booking form inside iframes).
// `csp_middleware` does the swap via string replace on the strict CSP,
// so we verify the contract: the produced CSP keeps frame-ancestors
// exactly once and contains the expected substring for swapping.
#[test]
fn build_csp_contains_frame_ancestors_self_for_swap() {
    assert_eq!(
        build_csp(&None).matches("frame-ancestors 'self'").count(),
        1
    );
    let cfg = Some(make_captcha_config(
        "https://cap.example.com",
        captcha::DEFAULT_WIDGET_URL,
    ));
    assert_eq!(build_csp(&cfg).matches("frame-ancestors 'self'").count(), 1);
}

#[test]
fn build_csp_embed_swap_preserves_captcha_allowances() {
    let cfg = Some(make_captcha_config(
        "https://cap.example.com",
        captcha::DEFAULT_WIDGET_URL,
    ));
    let strict = build_csp(&cfg);
    let embed_csp = strict.replace("frame-ancestors 'self'", "frame-ancestors *");
    assert!(embed_csp.contains("frame-ancestors *"));
    assert!(!embed_csp.contains("frame-ancestors 'self'"));
    // Captcha allowances must survive the swap so the widget loads in an
    // embedded booking form.
    assert!(embed_csp.contains("'wasm-unsafe-eval'"));
    assert!(embed_csp.contains("worker-src blob:"));
    assert!(embed_csp.contains("https://cap.example.com"));
    assert!(embed_csp.contains("https://cdn.jsdelivr.net"));
}

/// The Google OAuth callback echoes the provider's `error` parameter. It is
/// read from the query string before the state check, so a link carrying
/// markup must come back escaped, not rendered.
#[tokio::test]
async fn google_callback_escapes_the_error_parameter() {
    let (app, _pool, session, _) = setup_test_app().await;
    let uri = "/dashboard/sources/google/callback?error=%3Cimg%20src%3Dx%20onerror%3Dalert(1)%3E";
    let response = app.oneshot(get_authed(uri, &session)).await.unwrap();
    let body = body_string(response).await;
    assert!(!body.contains("<img"), "{body}");
    assert!(
        body.contains("&lt;img src=x onerror=alert(1)&gt;"),
        "{body}"
    );
}
