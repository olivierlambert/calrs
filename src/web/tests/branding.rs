//! Theme CSS derivation and the static assets served from the binary.

use super::*;

#[test]
fn hex_to_rgb_parses_3_and_6_digit() {
    assert_eq!(hex_to_rgb("ff0000"), Some((255, 0, 0)));
    assert_eq!(hex_to_rgb("2563eb"), Some((37, 99, 235)));
    assert_eq!(hex_to_rgb("f30"), Some((255, 51, 0)));
    assert_eq!(hex_to_rgb("fff"), Some((255, 255, 255)));
    assert_eq!(hex_to_rgb(""), None);
    assert_eq!(hex_to_rgb("12345"), None);
    assert_eq!(hex_to_rgb("gggggg"), None);
}

#[test]
fn hsl_round_trips_through_hex() {
    for hex in [
        "#2563eb", "#f4f4f5", "#18181b", "#ffffff", "#000000", "#be1621",
    ] {
        let (h, s, l) = hex_to_hsl(hex).expect("valid hex");
        assert_eq!(hsl_to_hex(h, s, l), hex, "round trip for {}", hex);
    }
    assert_eq!(hex_to_hsl("nothex"), None);
}

/// Splits the generated CSS into its `:root` and `html.dark` halves.
fn theme_blocks(css: &str) -> (String, String) {
    let (light, dark) = css.split_once("html.dark").expect("both blocks present");
    (light.to_string(), dark.to_string())
}

#[test]
fn custom_theme_derives_a_dark_palette_for_a_light_one() {
    // Regression test for #177: dark mode used to repeat the light colors.
    let css = custom_theme_css("#be1621", "#a01219", "#f5f4f0", "#ffffff", "#1a1b38");
    let (light, dark) = theme_blocks(&css);
    assert!(light.contains("--bg:#f5f4f0"), "{}", light);
    assert!(light.contains("--surface:#ffffff"), "{}", light);
    assert!(
        !dark.contains("--bg:#f5f4f0"),
        "dark bg must differ: {}",
        dark
    );
    assert!(!dark.contains("--surface:#ffffff"), "{}", dark);

    // Every dark neutral is actually dark, and the text is actually light.
    for var in ["--bg", "--surface", "--border"] {
        let hex = css_var(&dark, var);
        let (_, _, l) = hex_to_hsl(&hex).expect("derived hex");
        assert!(l < 0.3, "{} should be dark, got {} (l={})", var, hex, l);
    }
    for var in ["--text", "--text-secondary", "--text-muted"] {
        let hex = css_var(&dark, var);
        let (_, _, l) = hex_to_hsl(&hex).expect("derived hex");
        assert!(l > 0.5, "{} should be light, got {} (l={})", var, hex, l);
    }

    // The brand accent is untouched in both modes.
    assert!(light.contains("--accent:#be1621"), "{}", light);
    assert!(dark.contains("--accent:#be1621"), "{}", dark);
}

#[test]
fn custom_theme_keeps_an_already_dark_palette() {
    let css = custom_theme_css("#bd93f9", "#caa6fc", "#282a36", "#44475a", "#f8f8f2");
    let (_, dark) = theme_blocks(&css);
    assert_eq!(css_var(&dark, "--bg"), "#282a36");
    assert_eq!(css_var(&dark, "--surface"), "#44475a");
    assert_eq!(css_var(&dark, "--text"), "#f8f8f2");
    // Supporting neutrals are still derived so borders match the surface.
    let (_, _, border_l) = hex_to_hsl(&css_var(&dark, "--border")).expect("derived hex");
    let (_, _, surface_l) = hex_to_hsl("#44475a").expect("valid hex");
    assert!(border_l > surface_l, "border should sit above the surface");
}

#[test]
fn custom_theme_derives_for_a_mid_tone_background() {
    // A background at mid lightness is still a light palette when the text
    // is darker than it; an absolute lightness threshold would miss this.
    let css = custom_theme_css("#2563eb", "#1d4ed8", "#7a7a80", "#8e8e94", "#111111");
    let (_, dark) = theme_blocks(&css);
    let (_, _, bg_l) = hex_to_hsl(&css_var(&dark, "--bg")).expect("derived hex");
    assert!(bg_l < 0.3, "mid-tone palette should still get a dark bg");
}

#[test]
fn custom_theme_tolerates_surrounding_whitespace() {
    // The admin form stores the pasted value as-is, and validate_hex trims
    // before measuring, so a padded color reaches the formatter intact.
    let padded = custom_theme_css(" #be1621 ", "#a01219", "#f5f4f0", "#ffffff", "#1a1b38");
    let clean = custom_theme_css("#be1621", "#a01219", "#f5f4f0", "#ffffff", "#1a1b38");
    assert_eq!(padded, clean);
    assert!(
        padded.contains("--accent-subtle:rgba(190,22,33,0.08)"),
        "accent must not parse as black: {}",
        padded
    );
}

#[tokio::test]
async fn build_theme_css_derives_dark_from_stored_custom_colors() {
    let pool = setup_test_db().await;
    sqlx::query(
        "UPDATE auth_config SET theme = 'custom', custom_accent = '#be1621', custom_accent_hover = '#a01219', \
             custom_bg = '#f5f4f0', custom_surface = '#ffffff', custom_text = '#1a1b38' WHERE id = 'singleton'",
    )
    .execute(&pool)
    .await
    .unwrap();

    let css = build_theme_css(&pool).await;
    let (light, dark) = theme_blocks(&css);
    assert_eq!(css_var(&light, "--bg"), "#f5f4f0");
    assert_ne!(css_var(&dark, "--bg"), "#f5f4f0");
    let (_, _, dark_bg_l) = hex_to_hsl(&css_var(&dark, "--bg")).expect("derived hex");
    assert!(dark_bg_l < 0.3, "stored custom theme must yield a dark bg");
}

#[tokio::test]
async fn build_theme_css_derives_dark_when_custom_colors_are_unset() {
    // theme='custom' with the columns still NULL falls back to the default
    // light palette, which must also produce a usable dark block.
    let pool = setup_test_db().await;
    sqlx::query("UPDATE auth_config SET theme = 'custom' WHERE id = 'singleton'")
        .execute(&pool)
        .await
        .unwrap();

    let css = build_theme_css(&pool).await;
    assert!(
        !css.is_empty(),
        "unset custom colors must not blank the theme"
    );
    let (_, dark) = theme_blocks(&css);
    let (_, _, dark_bg_l) = hex_to_hsl(&css_var(&dark, "--bg")).expect("derived hex");
    assert!(dark_bg_l < 0.3, "{}", dark);
}

#[test]
fn custom_theme_rejects_invalid_hex() {
    assert_eq!(
        custom_theme_css("red", "#1d4ed8", "#fff", "#ffffff", "#000000"),
        ""
    );
}

/// Reads `--name:<value>;` out of a generated theme block.
fn css_var(block: &str, name: &str) -> String {
    let start = block
        .find(&format!("{}:", name))
        .unwrap_or_else(|| panic!("{} missing from {}", name, block))
        + name.len()
        + 1;
    let rest = &block[start..];
    let end = rest.find([';', '}']).unwrap_or(rest.len());
    rest[..end].to_string()
}

#[test]
fn brand_css_derives_rgba_variants() {
    let p = EmbedParams {
        brand: Some("ff0000".to_string()),
        ..Default::default()
    };
    let css = p.brand_css().expect("valid hex yields css");
    assert!(css.contains("--accent: #ff0000"), "{}", css);
    assert!(
        css.contains("--accent-subtle: rgba(255,0,0,0.12)"),
        "{}",
        css
    );
    assert!(
        css.contains("--accent-border: rgba(255,0,0,0.3)"),
        "{}",
        css
    );
    assert!(css.contains("--accent-muted: rgba(255,0,0,0.5)"), "{}", css);

    // Invalid input yields no CSS (never inject raw user input).
    let bad = EmbedParams {
        brand: Some("red; } body{display:none".to_string()),
        ..Default::default()
    };
    assert!(bad.brand_css().is_none());
}

// --- Vendored intl-tel-input bundle ---

#[tokio::test]
async fn intl_tel_input_assets_are_served_with_their_content_types() {
    let (app, _, _, _) = setup_test_app().await;
    let expected = [
        ("intlTelInput.min.js", "application/javascript"),
        ("intlTelInput.min.css", "text/css"),
        ("utils.js", "application/javascript"),
        ("flags.webp", "image/webp"),
        ("flags@2x.webp", "image/webp"),
        ("globe.webp", "image/webp"),
        ("globe@2x.webp", "image/webp"),
    ];
    for (file, content_type) in expected {
        let response = app
            .clone()
            .oneshot(get(&format!("/static/intl-tel-input/{file}")))
            .await
            .unwrap();
        assert_eq!(response.status(), 200, "{file} should be served");
        let served = response
            .headers()
            .get("Content-Type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        assert!(
            served.starts_with(content_type),
            "{file} should be served as {content_type}, got {served}"
        );
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        assert!(!bytes.is_empty(), "{file} should not be empty");
    }
}

#[tokio::test]
async fn intl_tel_input_route_serves_only_the_vendored_files() {
    // The handler matches names exactly rather than reading from a
    // directory, so nothing outside the bundle is addressable.
    let (app, _, _, _) = setup_test_app().await;
    for probe in [
        "nope.js",
        "utils.js.map",
        "UTILS.JS",
        "..%2F..%2FCargo.toml",
        "%2Fetc%2Fpasswd",
    ] {
        let response = app
            .clone()
            .oneshot(get(&format!("/static/intl-tel-input/{probe}")))
            .await
            .unwrap();
        assert_ne!(
            response.status(),
            200,
            "{probe} must not resolve to a served file"
        );
    }
}

#[test]
fn vendored_utils_js_keeps_its_apache_notice() {
    // utils.js is Google's libphonenumber under Apache-2.0, not the MIT
    // licence covering the rest of the bundle. The notice has to survive
    // any version bump, so assert on it rather than trusting the README.
    let utils = include_str!("../../../assets/intl-tel-input/utils.js");
    let head = &utils[..utils.len().min(400)];
    assert!(
        head.contains("Apache-2.0"),
        "utils.js must keep its Apache-2.0 SPDX header"
    );
    assert!(
        head.contains("Copyright The Closure Library Authors"),
        "utils.js must keep its copyright notice"
    );
    // Lazy loading uses a dynamic import(), which needs an ES module.
    assert!(
        utils.trim_end().ends_with("export default utils;"),
        "utils.js must stay an ES module for loadUtils to work"
    );
}

// --- Static routes ---

#[tokio::test]
async fn accent_css_returns_200() {
    let (app, _, _, _) = setup_test_app().await;
    let response = app.oneshot(get("/accent.css")).await.unwrap();
    assert_eq!(response.status(), 200);
}

/// A router whose data directory a test can put files into.
async fn router_with_data_dir() -> (Router, TestDataDir) {
    let pool = setup_test_db().await;
    let data_dir = TestDataDir::new();
    let router = create_router(pool, data_dir.to_path_buf(), [0u8; 32]).await;
    (router, data_dir)
}

#[tokio::test]
async fn favicon_defaults_to_the_crab() {
    let (app, _) = router_with_data_dir().await;
    for uri in ["/favicon", "/favicon.ico"] {
        let response = app.clone().oneshot(get(uri)).await.unwrap();
        assert_eq!(response.status().as_u16(), 200, "{uri}");
        assert_eq!(response.headers()["content-type"], "image/svg+xml", "{uri}");
        assert!(body_string(response).await.contains("🦀"), "{uri}");
    }
}

#[tokio::test]
async fn uploaded_favicon_is_served_with_its_sniffed_type() {
    let (app, data_dir) = router_with_data_dir().await;
    let png = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x01];
    std::fs::write(data_dir.join(FAVICON_FILE), png).unwrap();

    let response = app.oneshot(get("/favicon")).await.unwrap();
    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(response.headers()["content-type"], "image/png");
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&bytes[..], &png[..]);
}

/// A stored file that is not a recognised image (left over, or written by
/// hand) must not be served as one: the crab is served instead.
#[tokio::test]
async fn unrecognised_favicon_file_falls_back_to_the_crab() {
    let (app, data_dir) = router_with_data_dir().await;
    std::fs::write(data_dir.join(FAVICON_FILE), b"<svg onload=alert(1)>").unwrap();

    let response = app.oneshot(get("/favicon")).await.unwrap();
    assert_eq!(response.headers()["content-type"], "image/svg+xml");
    assert!(body_string(response).await.contains("🦀"));
}

#[tokio::test]
async fn pages_link_the_favicon_route() {
    let (app, _, _, _) = setup_test_app().await;
    let response = app.oneshot(get("/auth/login")).await.unwrap();
    let html = body_string(response).await;
    assert!(
        html.contains(r#"<link rel="icon" href="/favicon">"#),
        "{html}"
    );
}

#[tokio::test]
async fn logo_returns_response() {
    let (app, _, _, _) = setup_test_app().await;
    let response = app.oneshot(get("/logo")).await.unwrap();
    // 200 if logo exists, or 404/redirect if not
    let status = response.status().as_u16();
    assert!(status == 200 || status == 404 || status == 303);
}

// --- Logo and favicon uploads (#304) ---

/// A router with a data directory, and the session of an admin of it.
async fn admin_with_data_dir() -> (Router, TestDataDir, String) {
    let pool = setup_test_db().await;
    let (user_id, session) =
        insert_test_user_with_session(&pool, "Admin", "admin@example.com", "admin").await;
    sqlx::query("UPDATE users SET role = 'admin' WHERE id = ?")
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();
    let data_dir = TestDataDir::new();
    let router = create_router(pool, data_dir.to_path_buf(), [0u8; 32]).await;
    (router, data_dir, session)
}

/// The multipart POST the admin page's upload form sends.
fn upload(
    uri: &str,
    session: &str,
    field: &str,
    content_type: &str,
    bytes: &[u8],
) -> axum::http::Request<Body> {
    let boundary = "calrs-test-boundary";
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"{field}\"; filename=\"upload\"\r\nContent-Type: {content_type}\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    axum::http::Request::builder()
        .method("POST")
        .uri(format!("{uri}?_csrf=tok"))
        .header(
            "cookie",
            format!("__Host-calrs_session={session}; __Host-calrs_csrf=tok"),
        )
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap()
}

/// A PNG signature padded to `len` bytes, which is all the upload checks read.
fn png_of(len: usize) -> Vec<u8> {
    let mut bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.resize(len, 0);
    bytes
}

fn location(response: &axum::http::Response<Body>) -> &str {
    response.headers()["location"].to_str().unwrap()
}

#[tokio::test]
async fn an_svg_logo_is_refused_with_a_message() {
    let (app, data_dir, session) = admin_with_data_dir().await;
    let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\" onload=\"alert(1)\"/>";
    let response = app
        .clone()
        .oneshot(upload(
            "/dashboard/admin/logo",
            &session,
            "logo",
            "image/svg+xml",
            svg,
        ))
        .await
        .unwrap();
    assert_eq!(location(&response), "/dashboard/admin?upload_error=logo");
    assert!(!data_dir.join("logo.png").exists());

    let page = body_string(
        app.oneshot(get_authed("/dashboard/admin?upload_error=logo", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(page.contains(">The logo was not saved."), "message shown");
    assert!(page.contains(&format!("data-max-bytes=\"{LOGO_MAX_BYTES}\"")));
    assert!(!page.contains("SVG"), "the help no longer recommends SVG");
}

#[tokio::test]
async fn a_logo_up_to_the_advertised_2_mb_is_saved() {
    let (app, data_dir, session) = admin_with_data_dir().await;
    let png = png_of(LOGO_MAX_BYTES);
    let response = app
        .clone()
        .oneshot(upload(
            "/dashboard/admin/logo",
            &session,
            "logo",
            "image/png",
            &png,
        ))
        .await
        .unwrap();
    assert_eq!(location(&response), "/dashboard/admin");
    assert_eq!(std::fs::read(data_dir.join("logo.png")).unwrap(), png);

    let response = app
        .oneshot(upload(
            "/dashboard/admin/logo",
            &session,
            "logo",
            "image/png",
            &png_of(LOGO_MAX_BYTES + 1),
        ))
        .await
        .unwrap();
    assert_eq!(location(&response), "/dashboard/admin?upload_error=logo");
}

/// Over the route's body limit, the multipart reader itself fails; that must
/// still say why rather than redirect with nothing.
#[tokio::test]
async fn a_logo_over_the_body_limit_is_refused_with_a_message() {
    let (app, data_dir, session) = admin_with_data_dir().await;
    let png = png_of(LOGO_MAX_BYTES + 256 * 1024);
    let response = app
        .oneshot(upload(
            "/dashboard/admin/logo",
            &session,
            "logo",
            "image/png",
            &png,
        ))
        .await
        .unwrap();
    assert_eq!(location(&response), "/dashboard/admin?upload_error=logo");
    assert!(!data_dir.join("logo.png").exists());
}

/// A body cut off before its closing boundary fails in the reader, not in the
/// size or format checks (the image itself is valid), so only the handling
/// of a read error can refuse it.
#[tokio::test]
async fn a_truncated_upload_is_refused_with_a_message() {
    let (app, data_dir, session) = admin_with_data_dir().await;
    for (uri, field, file) in [
        ("/dashboard/admin/logo", "logo", "logo.png"),
        ("/dashboard/admin/favicon", "favicon", FAVICON_FILE),
    ] {
        let request = upload(uri, &session, field, "image/png", &png_of(1024));
        let (parts, body) = request.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        let cut = bytes.len() - "\r\n--calrs-test-boundary--\r\n".len();
        let request = axum::http::Request::from_parts(parts, Body::from(bytes.slice(..cut)));
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(
            location(&response),
            format!("/dashboard/admin?upload_error={field}")
        );
        assert!(!data_dir.join(file).exists(), "{field}");
    }
}

#[tokio::test]
async fn the_logo_is_served_with_its_real_type() {
    let (app, data_dir) = router_with_data_dir().await;
    for (bytes, content_type) in [
        (b"GIF89a....".to_vec(), "image/gif"),
        (b"RIFF\0\0\0\0WEBPVP8 ".to_vec(), "image/webp"),
        (vec![0xFF, 0xD8, 0xFF, 0xE0], "image/jpeg"),
        (png_of(16), "image/png"),
    ] {
        std::fs::write(data_dir.join("logo.png"), &bytes).unwrap();
        let response = app.clone().oneshot(get("/logo")).await.unwrap();
        assert_eq!(response.headers()["content-type"], content_type);
    }
}

/// An SVG logo stored before uploads refused SVG keeps displaying, but cannot
/// run its script when `/logo` is opened directly.
#[tokio::test]
async fn a_legacy_svg_logo_is_served_sandboxed() {
    let (app, data_dir) = router_with_data_dir().await;
    for svg in [
        &b"<svg onload=\"alert(1)\"/>"[..],
        b"\xEF\xBB\xBF\n  <?xml version=\"1.0\"?><svg/>",
        b"<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"\"><svg/>",
        b"<!-- Generator: Inkscape --><svg/>",
    ] {
        std::fs::write(data_dir.join("logo.png"), svg).unwrap();
        let response = app.clone().oneshot(get("/logo")).await.unwrap();
        assert_eq!(response.headers()["content-type"], "image/svg+xml");
        let csp = response.headers()["content-security-policy"]
            .to_str()
            .unwrap();
        assert!(csp.contains("sandbox"), "{csp}");
        assert!(csp.contains("default-src 'none'"), "{csp}");
    }
}

#[tokio::test]
async fn favicon_uploads_say_why_they_are_refused() {
    let (app, data_dir, session) = admin_with_data_dir().await;
    for bytes in [png_of(FAVICON_MAX_BYTES + 1), b"GIF8 not quite".to_vec()] {
        let response = app
            .clone()
            .oneshot(upload(
                "/dashboard/admin/favicon",
                &session,
                "favicon",
                "image/png",
                &bytes,
            ))
            .await
            .unwrap();
        assert_eq!(location(&response), "/dashboard/admin?upload_error=favicon");
    }
    assert!(!data_dir.join(FAVICON_FILE).exists());

    let page = body_string(
        app.clone()
            .oneshot(get_authed(
                "/dashboard/admin?upload_error=favicon",
                &session,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert!(page.contains(">The favicon was not saved."));

    let png = png_of(FAVICON_MAX_BYTES);
    let response = app
        .oneshot(upload(
            "/dashboard/admin/favicon",
            &session,
            "favicon",
            "image/png",
            &png,
        ))
        .await
        .unwrap();
    assert_eq!(location(&response), "/dashboard/admin");
    assert_eq!(std::fs::read(data_dir.join(FAVICON_FILE)).unwrap(), png);
}

/// The page reads back fixed reasons only, so a crafted link shows nothing.
#[tokio::test]
async fn an_unknown_upload_error_shows_no_message() {
    let (app, _, session) = admin_with_data_dir().await;
    let page = body_string(
        app.oneshot(get_authed(
            "/dashboard/admin?upload_error=%3Cb%3Eforged%3C%2Fb%3E",
            &session,
        ))
        .await
        .unwrap(),
    )
    .await;
    assert!(!page.contains("forged"));
    // The messages also sit in the file inputs' `data-too-big`, for the
    // browser-side check; only a paragraph of their own is shown.
    assert!(!page.contains(">The logo was not saved."));
    assert!(!page.contains(">The favicon was not saved."));
    assert!(!page.contains(">The file could not be written"));
}

// --- User and team avatar uploads (#307) ---

/// A router with a data directory, its pool, the session of a user of it,
/// and a team that user administers. Returns the user's and the team's
/// upload URLs and settings pages, in that order.
async fn avatar_targets() -> (
    Router,
    SqlitePool,
    TestDataDir,
    String,
    [(String, String); 2],
) {
    let pool = setup_test_db().await;
    let (user_id, session) =
        insert_test_user_with_session(&pool, "Ann", "ann@example.com", "user").await;
    let team_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO teams (id, name, slug, visibility, created_by) VALUES (?, 'Avatar Team', 'avatar-team', 'public', ?)")
        .bind(&team_id)
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO team_members (team_id, user_id, role, source) VALUES (?, ?, 'admin', 'direct')")
        .bind(&team_id)
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();
    let data_dir = TestDataDir::new();
    let router = create_router(pool.clone(), data_dir.to_path_buf(), [0u8; 32]).await;
    let targets = [
        (
            "/dashboard/settings/avatar".to_string(),
            "/dashboard/settings".to_string(),
        ),
        (
            format!("/dashboard/teams/{team_id}/avatar"),
            format!("/dashboard/teams/{team_id}/settings"),
        ),
    ];
    (router, pool, data_dir, session, targets)
}

#[tokio::test]
async fn an_avatar_up_to_2_mb_is_saved_and_one_byte_more_is_refused() {
    let (app, pool, data_dir, session, targets) = avatar_targets().await;
    for (uri, page) in &targets {
        let response = app
            .clone()
            .oneshot(upload(
                uri,
                &session,
                "avatar",
                "image/png",
                &png_of(AVATAR_MAX_BYTES),
            ))
            .await
            .unwrap();
        assert_eq!(location(&response), page.as_str(), "{uri}");

        let response = app
            .clone()
            .oneshot(upload(
                uri,
                &session,
                "avatar",
                "image/png",
                &png_of(AVATAR_MAX_BYTES + 1),
            ))
            .await
            .unwrap();
        assert_eq!(
            location(&response),
            format!("{page}?upload_error=avatar"),
            "{uri}"
        );
    }
    let user_avatar: Option<String> =
        sqlx::query_scalar("SELECT avatar_path FROM users WHERE email = 'ann@example.com'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let team_avatar: Option<String> =
        sqlx::query_scalar("SELECT avatar_path FROM teams WHERE slug = 'avatar-team'")
            .fetch_one(&pool)
            .await
            .unwrap();
    for name in [user_avatar.unwrap(), team_avatar.unwrap()] {
        assert_eq!(
            std::fs::read(data_dir.join("avatars").join(&name)).unwrap(),
            png_of(AVATAR_MAX_BYTES),
            "the refused upload must not replace the saved one"
        );
    }
}

#[tokio::test]
async fn an_avatar_that_is_not_an_image_is_refused_with_a_message() {
    let (app, _pool, data_dir, session, targets) = avatar_targets().await;
    let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\" onload=\"alert(1)\"/>";
    for (uri, page) in &targets {
        let response = app
            .clone()
            .oneshot(upload(uri, &session, "avatar", "image/svg+xml", svg))
            .await
            .unwrap();
        assert_eq!(location(&response), format!("{page}?upload_error=avatar"));

        let shown = body_string(
            app.clone()
                .oneshot(get_authed(&format!("{page}?upload_error=avatar"), &session))
                .await
                .unwrap(),
        )
        .await;
        assert!(shown.contains(">The picture was not saved."), "{page}");
        assert!(shown.contains(&format!("data-max-bytes=\"{AVATAR_MAX_BYTES}\"")));

        // Only the fixed reasons are read back, never free text.
        let crafted = body_string(
            app.clone()
                .oneshot(get_authed(&format!("{page}?upload_error=hello"), &session))
                .await
                .unwrap(),
        )
        .await;
        assert!(!crafted.contains(">The picture was not saved."), "{page}");
        assert!(!crafted.contains("hello"), "{page}");
    }
    assert!(!data_dir.join("avatars").exists(), "nothing written");
}

/// Over the route's body limit, and on a body cut off before its closing
/// boundary, the multipart reader itself fails; both must still say why.
#[tokio::test]
async fn an_avatar_the_reader_cannot_read_is_refused_with_a_message() {
    let (app, _pool, data_dir, session, targets) = avatar_targets().await;
    for (uri, page) in &targets {
        let response = app
            .clone()
            .oneshot(upload(
                uri,
                &session,
                "avatar",
                "image/png",
                &png_of(AVATAR_MAX_BYTES + 256 * 1024),
            ))
            .await
            .unwrap();
        assert_eq!(location(&response), format!("{page}?upload_error=avatar"));

        let request = upload(uri, &session, "avatar", "image/png", &png_of(1024));
        let (parts, body) = request.into_parts();
        let bytes = body.collect().await.unwrap().to_bytes();
        let cut = bytes.len() - "\r\n--calrs-test-boundary--\r\n".len();
        let request = axum::http::Request::from_parts(parts, Body::from(bytes.slice(..cut)));
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(location(&response), format!("{page}?upload_error=avatar"));
    }
    assert!(!data_dir.join("avatars").exists(), "nothing written");
}

#[tokio::test]
async fn an_avatar_that_cannot_be_written_is_reported() {
    let (app, pool, data_dir, session, targets) = avatar_targets().await;
    // A file where the avatars directory should be makes every write fail.
    std::fs::write(data_dir.join("avatars"), b"not a directory").unwrap();
    for (uri, page) in &targets {
        let response = app
            .clone()
            .oneshot(upload(uri, &session, "avatar", "image/png", &png_of(1024)))
            .await
            .unwrap();
        assert_eq!(
            location(&response),
            format!("{page}?upload_error=avatar-save")
        );
        let shown = body_string(
            app.clone()
                .oneshot(get_authed(
                    &format!("{page}?upload_error=avatar-save"),
                    &session,
                ))
                .await
                .unwrap(),
        )
        .await;
        assert!(
            shown.contains(">The picture could not be saved on the server."),
            "{page}"
        );
    }
    let recorded: i64 = sqlx::query_scalar(
        "SELECT (SELECT COUNT(*) FROM users WHERE avatar_path IS NOT NULL)
              + (SELECT COUNT(*) FROM teams WHERE avatar_path IS NOT NULL)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(recorded, 0, "a failed write records no avatar");
}
