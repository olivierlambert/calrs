//! Admin panel: users, auth, captcha, SMTP and SMS settings, impersonation.

use super::*;

// --- is_safe_company_link ---

#[test]
fn company_link_accepts_http_and_https() {
    assert!(is_safe_company_link("http://example.com"));
    assert!(is_safe_company_link("https://example.com"));
    assert!(is_safe_company_link("HTTPS://EXAMPLE.COM"));
    assert!(is_safe_company_link("  https://example.com/path  "));
}

#[test]
fn company_link_rejects_javascript_uri() {
    // The whole point of this validator: stop stored XSS via the
    // company-link anchor that renders on every public page.
    assert!(!is_safe_company_link("javascript:alert(1)"));
    assert!(!is_safe_company_link("JAVASCRIPT:alert(1)"));
    assert!(!is_safe_company_link("  javascript:alert(1)"));
}

#[test]
fn company_link_rejects_other_dangerous_schemes() {
    assert!(!is_safe_company_link("data:text/html,<script>"));
    assert!(!is_safe_company_link("vbscript:msgbox(1)"));
    assert!(!is_safe_company_link("file:///etc/passwd"));
}

#[test]
fn company_link_rejects_unschemed_input() {
    // A bare hostname shouldn't slip through. The rendered anchor
    // would be relative to the booking page, which is wrong and
    // looks like a path traversal vector.
    assert!(!is_safe_company_link("example.com"));
    assert!(!is_safe_company_link("/foo/bar"));
    assert!(!is_safe_company_link("//example.com"));
    assert!(!is_safe_company_link(""));
}

#[tokio::test]
async fn admin_page_returns_200_for_admin() {
    let (app, _, session, _) = setup_test_app().await;
    let response = app
        .oneshot(get_authed("/dashboard/admin", &session))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_string(response).await;
    assert!(body.contains("Admin"), "Admin page should render");

    // The SMS card is rendered from the provider registry, so every
    // shipped gateway must show up without touching the template.
    assert!(body.contains("SMS settings"), "SMS card should render");
    for spec in crate::sms::PROVIDER_SPECS {
        assert!(
            body.contains(spec.label),
            "SMS gateway {} should be offered",
            spec.label
        );
    }
}

/// The TLS mode `<select>` marks exactly one option `selected`. Two would
/// be worse than none: a browser honours the last one, so an operator on
/// STARTTLS would be shown "None, unencrypted" and silently downgrade the
/// transport by saving the form untouched.
#[tokio::test]
async fn admin_smtp_tls_mode_select_marks_one_option() {
    // Reads config that CALRS_* env blocks override (see CLAUDE.md).
    let _env = crate::test_support::ENV_LOCK.lock().await;
    for (stored, expected_label) in [
        ("starttls", "STARTTLS (port 587)"),
        ("tls", "Implicit TLS (port 465)"),
        ("none", "None, unencrypted (local MTA only)"),
    ] {
        let (app, pool, session, _) = setup_test_app().await;
        sqlx::query(
            "INSERT INTO smtp_config (id, host, port, username, password_enc, from_email, tls_mode, enabled) \
                 VALUES (?, 'smtp.test', 587, '', '', 'noreply@test.com', ?, 1)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(stored)
        .execute(&pool)
        .await
        .unwrap();

        let response = app
            .oneshot(get_authed("/dashboard/admin", &session))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_string(response).await;

        let selected: Vec<&str> = [
            "STARTTLS (port 587)",
            "Implicit TLS (port 465)",
            "None, unencrypted (local MTA only)",
        ]
        .into_iter()
        .filter(|label| {
            body.split("<option")
                .any(|opt| opt.contains(label) && opt.contains("selected"))
        })
        .collect();
        assert_eq!(
            selected,
            vec![expected_label],
            "tls_mode {stored} should select exactly {expected_label}"
        );
    }
}

// --- Admin actions ---

#[tokio::test]
async fn admin_toggle_user_role() {
    let (app, pool, session, _) = setup_test_app().await;

    // Create a non-admin user
    let user2_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'user2@test.com', 'User Two', 'user', 'local', 'user2', 1)")
        .bind(&user2_id)
        .execute(&pool)
        .await
        .unwrap();

    let csrf = "test-csrf-toggle-role";
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/admin/users/{}/toggle-role", user2_id),
            &session,
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());

    let role: String = sqlx::query_scalar("SELECT role FROM users WHERE id = ?")
        .bind(&user2_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(role, "admin", "User should be promoted to admin");
}

#[tokio::test]
async fn admin_toggle_user_enabled() {
    let (app, pool, session, _) = setup_test_app().await;

    let user2_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'user3@test.com', 'User Three', 'user', 'local', 'user3', 1)")
        .bind(&user2_id)
        .execute(&pool)
        .await
        .unwrap();

    let csrf = "test-csrf-toggle-en";
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/admin/users/{}/toggle-enabled", user2_id),
            &session,
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());

    let enabled: i32 = sqlx::query_scalar("SELECT enabled FROM users WHERE id = ?")
        .bind(&user2_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(enabled, 0, "User should be disabled");
}

/// The red-team run: the only admin demoted themselves from the admin panel
/// and the instance was left with none. Same for disabling.
#[tokio::test]
async fn admin_panel_refuses_to_remove_the_last_enabled_admin() {
    let (app, pool, session, _) = setup_test_app().await;
    sqlx::query("UPDATE users SET language = 'fr' WHERE username = 'testuser'")
        .execute(&pool)
        .await
        .unwrap();
    let me: String = sqlx::query_scalar("SELECT id FROM users WHERE username = 'testuser'")
        .fetch_one(&pool)
        .await
        .unwrap();

    for action in ["toggle-role", "toggle-enabled"] {
        let csrf = "test-csrf-last-admin";
        let response = app
            .clone()
            .oneshot(post_form(
                &format!("/dashboard/admin/users/{me}/{action}"),
                &session,
                csrf,
                &format!("_csrf={csrf}"),
            ))
            .await
            .unwrap();
        assert!(response.status().is_redirection());
        let location = response.headers()["location"].to_str().unwrap().to_string();
        let expected = crate::i18n::translate("fr", "admin-error-last-admin", None);
        assert_eq!(
            location,
            format!("/dashboard/admin?error={}", urlencoding::encode(&expected)),
            "{action}: the refusal is shown in the admin's language"
        );
        let (role, enabled): (String, i32) =
            sqlx::query_as("SELECT role, enabled FROM users WHERE id = ?")
                .bind(&me)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!((role.as_str(), enabled), ("admin", 1), "{action}");
    }

    // With a second enabled admin, demoting yourself is allowed.
    insert_role_user(&pool, "other-admin@test.com", "admin").await;
    let csrf = "test-csrf-last-admin-ok";
    app.oneshot(post_form(
        &format!("/dashboard/admin/users/{me}/toggle-role"),
        &session,
        csrf,
        &format!("_csrf={csrf}"),
    ))
    .await
    .unwrap();
    let role: String = sqlx::query_scalar("SELECT role FROM users WHERE id = ?")
        .bind(&me)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(role, "user");
}

#[tokio::test]
async fn admin_update_auth_settings() {
    let (app, pool, session, _) = setup_test_app().await;
    let csrf = "test-csrf-auth";
    let body = format!("_csrf={}&allowed_email_domains=example.com", csrf);
    let response = app
        .oneshot(post_form("/dashboard/admin/auth", &session, csrf, &body))
        .await
        .unwrap();
    assert!(response.status().is_redirection());

    let domains: Option<String> =
        sqlx::query_scalar("SELECT allowed_email_domains FROM auth_config WHERE id = 'singleton'")
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert_eq!(domains.unwrap(), "example.com");
}

#[tokio::test]
async fn admin_captcha_wasm_url_is_validated_and_stored() {
    let (app, pool, session, _) = setup_test_app().await;
    let stored = || async {
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT captcha_wasm_url FROM auth_config WHERE id = 'singleton'",
        )
        .fetch_one(&pool)
        .await
        .unwrap()
    };

    let csrf = "test-csrf-captcha-wasm";
    let body = format!("_csrf={}&captcha_wasm_url=javascript%3Aalert(1)", csrf);
    let response = app
        .clone()
        .oneshot(post_form("/dashboard/admin/captcha", &session, csrf, &body))
        .await
        .unwrap();
    let location = response.headers()["location"].to_str().unwrap().to_string();
    assert!(
        location.starts_with("/dashboard/admin?error="),
        "{location}"
    );
    assert_eq!(stored().await, None, "an invalid URL must not be saved");

    let body = format!(
        "_csrf={}&captcha_wasm_url=https%3A%2F%2Fcap.example.com%2Fassets%2Fcap_wasm_bg.wasm",
        csrf
    );
    let response = app
        .oneshot(post_form("/dashboard/admin/captcha", &session, csrf, &body))
        .await
        .unwrap();
    assert_eq!(response.headers()["location"], "/dashboard/admin");
    assert_eq!(
        stored().await.as_deref(),
        Some("https://cap.example.com/assets/cap_wasm_bg.wasm")
    );
}

#[tokio::test]
async fn admin_captcha_form_shows_stored_values_while_incomplete() {
    let (app, pool, session, _) = setup_test_app().await;
    // No secret, so the captcha is not active, but the URLs are stored.
    sqlx::query(
        "UPDATE auth_config SET captcha_instance_url = 'https://cap.example.com', \
             captcha_site_key = 'site-123', captcha_secret = NULL, \
             captcha_widget_url = 'https://cdn.example.com/widget.js', \
             captcha_wasm_url = 'https://cdn.example.com/cap_wasm_bg.wasm' \
             WHERE id = 'singleton'",
    )
    .execute(&pool)
    .await
    .unwrap();

    let response = app
        .oneshot(get_authed("/dashboard/admin", &session))
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = body_string(response).await;
    for (field, value) in [
        ("captcha_instance_url", "https://cap.example.com"),
        ("captcha_site_key", "site-123"),
        ("captcha_widget_url", "https://cdn.example.com/widget.js"),
        (
            "captcha_wasm_url",
            "https://cdn.example.com/cap_wasm_bg.wasm",
        ),
    ] {
        let input = body
            .split("<input")
            .find(|tag| tag.contains(&format!("name=\"{field}\"")))
            .unwrap_or_else(|| panic!("no {field} input"));
        // minijinja escapes '/' in attribute values.
        let escaped = value.replace('/', "&#x2f;");
        assert!(
            input.contains(&format!("value=\"{escaped}\"")),
            "{field} lost its stored value: {input}"
        );
    }
}

#[tokio::test]
async fn admin_captcha_inputs_are_labelled() {
    let (app, _, session, _) = setup_test_app().await;
    let response = app
        .oneshot(get_authed("/dashboard/admin", &session))
        .await
        .unwrap();
    let body = body_string(response).await;
    for field in [
        "captcha_instance_url",
        "captcha_site_key",
        "captcha_secret",
        "captcha_widget_url",
        "captcha_wasm_url",
    ] {
        assert_eq!(
            body.matches(&format!("id=\"{field}\"")).count(),
            1,
            "{field} needs exactly one id"
        );
        assert!(
            body.contains(&format!("<label for=\"{field}\">")),
            "{field} has no label pointing at it"
        );
    }
}

#[tokio::test]
async fn admin_non_admin_gets_403() {
    let (app, pool, _, _) = setup_test_app().await;

    // Create a non-admin user with session
    let user_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'nonadmin@test.com', 'Non Admin', 'user', 'local', 'nonadmin', 1)")
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();
    let session = uuid::Uuid::new_v4().to_string();
    let expires = (Utc::now() + Duration::days(30))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    sqlx::query("INSERT INTO sessions (id, user_id, expires_at) VALUES (?, ?, ?)")
        .bind(&session)
        .bind(&user_id)
        .bind(&expires)
        .execute(&pool)
        .await
        .unwrap();

    let response = app
        .oneshot(get_authed("/dashboard/admin", &session))
        .await
        .unwrap();
    assert_eq!(response.status(), 403, "Non-admin should get 403");
}

// --- Impersonation ---

#[tokio::test]
async fn admin_impersonate_sets_cookie() {
    let (app, pool, session, _) = setup_test_app().await;

    let user2_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO users (id, email, name, role, auth_provider, username, enabled) VALUES (?, 'target@test.com', 'Target User', 'user', 'local', 'target', 1)")
        .bind(&user2_id)
        .execute(&pool)
        .await
        .unwrap();

    let csrf = "test-csrf-imp";
    let response = app
        .oneshot(post_form(
            &format!("/dashboard/admin/impersonate/{}", user2_id),
            &session,
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());

    // Check for impersonation cookie
    let cookies: Vec<&str> = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .collect();
    let has_impersonate = cookies.iter().any(|c| c.contains("calrs_impersonate"));
    assert!(has_impersonate, "Should set impersonation cookie");
}

#[tokio::test]
async fn admin_stop_impersonate() {
    let (app, _, session, _) = setup_test_app().await;
    let csrf = "test-csrf-stop-imp";
    let response = app
        .oneshot(post_form(
            "/dashboard/admin/stop-impersonate",
            &session,
            csrf,
            &format!("_csrf={}", csrf),
        ))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
}

// --- Admin accent/theme ---

#[tokio::test]
async fn admin_update_accent_redirects() {
    let (app, _, session, _) = setup_test_app().await;
    let csrf = "test-csrf-accent";
    let body = format!("_csrf={}&theme=nord", csrf);
    let response = app
        .oneshot(post_form("/dashboard/admin/accent", &session, csrf, &body))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
}

// --- Admin OIDC update ---

#[tokio::test]
async fn admin_update_oidc_settings() {
    let (app, pool, session, _) = setup_test_app().await;
    let csrf = "test-csrf-oidc";
    let body = format!(
        "_csrf={}&oidc_issuer_url=https%3A%2F%2Fauth.example.com&oidc_client_id=calrs&oidc_client_secret=secret123",
        csrf
    );
    let response = app
        .oneshot(post_form("/dashboard/admin/oidc", &session, csrf, &body))
        .await
        .unwrap();
    assert!(response.status().is_redirection());

    let client_id: Option<String> =
        sqlx::query_scalar("SELECT oidc_client_id FROM auth_config WHERE id = 'singleton'")
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert_eq!(client_id.unwrap(), "calrs");
}

// --- Switching gateway must not reuse the previous gateway's credential ---

#[tokio::test]
async fn switching_sms_gateway_requires_a_fresh_credential() {
    let (app, pool, session, _) = setup_test_app().await;
    let csrf = "test-csrf-sms-switch";

    // Configure Twilio.
    let body = "_csrf=test-csrf-sms-switch&provider=twilio&api_key=AC123&api_secret=twilio-token&sender=%2B15551234567&base_url=&default_country_code=%2B33&enabled=on";
    let response = app
        .clone()
        .oneshot(post_form("/dashboard/admin/sms", &session, csrf, body))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    let stored: (String, Option<String>) =
        sqlx::query_as("SELECT provider, api_secret_enc FROM sms_config")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored.0, "twilio");
    let twilio_secret = stored.1.clone().expect("secret stored");

    // Switching to another gateway with the secret field left empty must
    // be refused rather than silently reusing the Twilio auth token.
    let body = "_csrf=test-csrf-sms-switch&provider=sevenio&api_key=&api_secret=&sender=calrs&base_url=&default_country_code=%2B33&enabled=on";
    let response = app
        .clone()
        .oneshot(post_form("/dashboard/admin/sms", &session, csrf, body))
        .await
        .unwrap();
    assert!(response.status().is_redirection());
    let location = response
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(
        location.contains("error="),
        "expected a rejection: {}",
        location
    );

    let stored: (String, Option<String>) =
        sqlx::query_as("SELECT provider, api_secret_enc FROM sms_config")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored.0, "twilio", "the stored gateway must be unchanged");
    assert_eq!(stored.1.as_deref(), Some(twilio_secret.as_str()));

    // With a credential, the switch goes through and replaces the secret.
    let body = "_csrf=test-csrf-sms-switch&provider=sevenio&api_key=&api_secret=seven-key&sender=calrs&base_url=&default_country_code=%2B33&enabled=on";
    let response = app
        .oneshot(post_form("/dashboard/admin/sms", &session, csrf, body))
        .await
        .unwrap();
    assert!(response.status().is_redirection());

    let config = crate::sms::load_config(&pool, &[0u8; 32])
        .await
        .unwrap()
        .expect("configured");
    assert_eq!(config.provider, "sevenio");
    assert_eq!(config.api_secret, "seven-key");
}

// --- SMS gateway configuration round-trips through the database ---

#[tokio::test]
async fn sms_config_round_trips_through_the_database() {
    // Reads config that CALRS_* env blocks override (see CLAUDE.md).
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let pool = setup_test_db().await;
    let key = [7u8; 32];

    // Nothing configured is the default state, and the booking form still
    // needs a country code to render.
    assert!(crate::sms::load_config(&pool, &key)
        .await
        .unwrap()
        .is_none());
    assert!(crate::sms::load_status(&pool).await.unwrap().is_none());
    assert_eq!(
        crate::sms::default_country_code(&pool).await,
        crate::sms::phone::DEFAULT_COUNTRY_CODE
    );

    let secret_enc = crate::crypto::encrypt_password(&key, "super-secret-token").unwrap();
    sqlx::query(
        "INSERT INTO sms_config (id, provider, api_key, api_secret_enc, sender, base_url, default_country_code, enabled)
             VALUES ('cfg', 'gatewayapi', NULL, ?, 'calrs', 'https://gatewayapi.eu', '+45', 1)",
    )
    .bind(&secret_enc)
    .execute(&pool)
    .await
    .unwrap();

    let config = crate::sms::load_config(&pool, &key)
        .await
        .unwrap()
        .expect("configured");
    assert_eq!(config.provider, crate::sms::kinds::GATEWAYAPI);
    assert_eq!(config.api_secret, "super-secret-token");
    assert_eq!(config.sender, "calrs");
    assert_eq!(config.base_url, "https://gatewayapi.eu");
    assert_eq!(crate::sms::default_country_code(&pool).await, "+45");

    let status = crate::sms::load_status(&pool).await.unwrap().expect("row");
    assert_eq!(status.provider_label, "GatewayAPI");
    assert!(status.enabled);
    assert!(!status.from_env);

    // Disabling stops sends but keeps the row visible to the admin panel.
    sqlx::query("UPDATE sms_config SET enabled = 0")
        .execute(&pool)
        .await
        .unwrap();
    assert!(crate::sms::load_config(&pool, &key)
        .await
        .unwrap()
        .is_none());
    assert!(crate::sms::load_status(&pool).await.unwrap().is_some());

    // A row that no longer validates reads as "not configured" rather than
    // failing inside a booking request.
    sqlx::query("UPDATE sms_config SET enabled = 1, sender = ''")
        .execute(&pool)
        .await
        .unwrap();
    assert!(crate::sms::load_config(&pool, &key)
        .await
        .unwrap()
        .is_none());
}

/// The card shows the URI calrs will send, built like the flow builds it:
/// a trailing slash does not double, and an http base gets an explanation
/// instead of a URI Entra would never accept.
#[tokio::test]
async fn admin_shows_the_microsoft_callback_to_register_or_why_not() {
    let _env = crate::test_support::ENV_LOCK.lock().await;
    let _guard = crate::test_support::EnvGuard::new();
    for (base, callback) in [
        (
            "https://cal.example.com/",
            Some("https://cal.example.com/dashboard/sources/microsoft/callback"),
        ),
        ("http://cal.example.com", None),
    ] {
        std::env::set_var("CALRS_BASE_URL", base);
        let (app, _, session, _) = setup_test_app().await;
        let body = body_string(
            app.oneshot(get_authed("/dashboard/admin", &session))
                .await
                .unwrap(),
        )
        .await;
        // The template escapes `/` as `&#x2f;`.
        let escaped = |text: &str| text.replace('/', "&#x2f;");
        match callback {
            Some(uri) => assert!(
                body.contains(&format!("<code>{}</code>", escaped(uri))),
                "{base}"
            ),
            None => assert!(
                body.contains("to see the callback URL to register"),
                "{base}"
            ),
        }
        assert!(!body.contains(&escaped("//dashboard/sources/microsoft/callback")));
    }
}

/// Each Microsoft credential field is named by its visible label, so
/// assistive technology can identify it.
#[tokio::test]
async fn microsoft_credential_labels_name_their_inputs() {
    let (app, _, session, _) = setup_test_app().await;
    let body = body_string(
        app.oneshot(get_authed("/dashboard/admin", &session))
            .await
            .unwrap(),
    )
    .await;
    for field in [
        "microsoft_oauth2_client_id",
        "microsoft_oauth2_client_secret",
        "microsoft_oauth2_tenant",
    ] {
        assert!(
            body.contains(&format!("<label for=\"{field}\">")),
            "{field}"
        );
        assert_eq!(
            body.matches(&format!("id=\"{field}\"")).count(),
            1,
            "{field}"
        );
    }
}
