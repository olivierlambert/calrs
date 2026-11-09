use sqlx::SqlitePool;

pub const DEFAULT_WIDGET_URL: &str = "https://cdn.jsdelivr.net/npm/cap-widget";

pub struct CaptchaConfig {
    pub instance_url: String,
    pub site_key: String,
    /// Kept in `AppState` for the life of the process, so wiped when dropped.
    pub secret: zeroize::Zeroizing<String>,
    pub widget_url: String,
    /// Where the widget fetches its WASM solver, exposed to it as
    /// `window.CAP_CUSTOM_WASM_URL`. `None` keeps the widget's default (the
    /// jsDelivr CDN). Always an absolute http(s) URL when set.
    pub wasm_url: Option<String>,
}

pub struct CaptchaVars {
    pub enabled: bool,
    pub api_endpoint: String,
    pub widget_url: String,
    /// Empty when no custom WASM URL is configured.
    pub wasm_url: String,
}

impl CaptchaVars {
    pub fn from_config(config: &Option<CaptchaConfig>) -> Self {
        Self {
            enabled: config.is_some(),
            api_endpoint: config
                .as_ref()
                .map(|c| c.api_endpoint())
                .unwrap_or_default(),
            widget_url: config
                .as_ref()
                .map(|c| c.widget_url.clone())
                .unwrap_or_else(|| DEFAULT_WIDGET_URL.to_string()),
            wasm_url: config
                .as_ref()
                .and_then(|c| c.wasm_url.clone())
                .unwrap_or_default(),
        }
    }
}

impl CaptchaConfig {
    /// API endpoint URL passed to the <cap-widget> data-cap-api-endpoint attribute.
    pub fn api_endpoint(&self) -> String {
        format!(
            "{}/{}/",
            self.instance_url.trim_end_matches('/'),
            self.site_key
        )
    }

    /// Extract scheme+host from widget_url for use in Content-Security-Policy script-src.
    /// e.g. "https://cdn.jsdelivr.net/npm/cap-widget" → "https://cdn.jsdelivr.net"
    pub fn widget_script_origin(&self) -> String {
        extract_origin(&self.widget_url)
    }

    /// Extract scheme+host from instance_url for use in Content-Security-Policy connect-src.
    /// e.g. "https://captcha.example.com" → "https://captcha.example.com"
    pub fn instance_origin(&self) -> String {
        extract_origin(&self.instance_url)
    }

    /// Origin of the custom WASM URL, for Content-Security-Policy connect-src
    /// (the widget fetches the binary with `fetch()`). `None` when unset.
    pub fn wasm_origin(&self) -> Option<String> {
        self.wasm_url
            .as_deref()
            .map(extract_origin)
            .filter(|o| !o.is_empty())
    }
}

/// Whether `url` is an absolute http(s) URL with a host, the only shape the
/// captcha URL fields accept.
pub fn is_http_url(url: &str) -> bool {
    !extract_origin(url.trim()).is_empty()
}

fn extract_origin(url: &str) -> String {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return String::new();
    };
    if !matches!(parsed.scheme(), "http" | "https") {
        return String::new();
    }
    let Some(host) = parsed.host_str().filter(|h| !h.is_empty()) else {
        return String::new();
    };
    match parsed.port() {
        Some(port) => format!("{}://{}:{}", parsed.scheme(), host, port),
        None => format!("{}://{}", parsed.scheme(), host),
    }
}

/// What the admin captcha form shows: the stored values as they are, whether
/// or not they add up to a working configuration. Reading these from the
/// active [`CaptchaConfig`] instead would show empty fields while the
/// configuration is incomplete (say, before the secret is entered), and the
/// next save would then wipe the values already stored.
pub struct CaptchaFormValues {
    pub instance_url: String,
    pub site_key: String,
    /// The stored widget URL, or [`DEFAULT_WIDGET_URL`] when none is stored.
    pub widget_url: String,
    pub wasm_url: String,
}

pub async fn load_captcha_form_values(pool: &SqlitePool) -> CaptchaFormValues {
    let row: Option<(
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT captcha_instance_url, captcha_site_key, captcha_widget_url, captcha_wasm_url \
             FROM auth_config WHERE id = 'singleton'",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    let (instance_url, site_key, widget_url, wasm_url) = row.unwrap_or_default();
    CaptchaFormValues {
        instance_url: instance_url.unwrap_or_default(),
        site_key: site_key.unwrap_or_default(),
        widget_url: widget_url
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_WIDGET_URL.to_string()),
        wasm_url: wasm_url.unwrap_or_default(),
    }
}

pub async fn load_captcha_config(pool: &SqlitePool, key: &[u8; 32]) -> Option<CaptchaConfig> {
    let row: Option<(
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT captcha_instance_url, captcha_site_key, captcha_secret, captcha_widget_url, \
             captcha_wasm_url FROM auth_config WHERE id = 'singleton'",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let (instance_url, site_key, secret_enc, widget_url, wasm_url) = row?;
    let instance_url = instance_url.filter(|s| !s.trim().is_empty())?;
    let site_key = site_key.filter(|s| !s.trim().is_empty())?;
    let secret_enc = secret_enc.filter(|s| !s.trim().is_empty())?;
    let secret = crate::crypto::decrypt_value(key, &secret_enc).ok()?;
    let widget_url = widget_url
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_WIDGET_URL.to_string());
    // The admin form refuses anything else, so this only drops a value written
    // to the database by hand.
    let wasm_url = wasm_url
        .map(|s| s.trim().to_string())
        .filter(|s| is_http_url(s));

    Some(CaptchaConfig {
        instance_url,
        site_key,
        secret,
        widget_url,
        wasm_url,
    })
}

#[derive(serde::Serialize)]
struct VerifyRequest<'a> {
    secret: &'a str,
    response: &'a str,
}

#[derive(serde::Deserialize)]
struct VerifyResponse {
    success: bool,
}

/// Returns `Ok(())` if captcha is not configured (pass-through) or if the token
/// is valid. Returns `Err(())` if captcha is configured but the token is missing
/// or fails server-side verification.
pub async fn verify(config: &Option<CaptchaConfig>, token: Option<&str>) -> Result<(), ()> {
    let cfg = match config {
        Some(c) => c,
        None => return Ok(()),
    };

    let token = match token.filter(|t| !t.trim().is_empty()) {
        Some(t) => t,
        None => {
            tracing::warn!("captcha token missing on booking attempt");
            return Err(());
        }
    };

    let verify_url = format!("{}/siteverify", cfg.api_endpoint().trim_end_matches('/'));

    // No redirects: a 307 or 308 would re-send the secret to whatever host
    // it names.
    let client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "captcha client build failed");
            return Err(());
        }
    };
    let resp = match client
        .post(&verify_url)
        .json(&VerifyRequest {
            secret: &cfg.secret,
            response: token,
        })
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "captcha verification request failed");
            return Err(());
        }
    };

    match resp.json::<VerifyResponse>().await {
        Ok(r) if r.success => Ok(()),
        Ok(_) => {
            tracing::warn!("captcha token rejected by verification server");
            Err(())
        }
        Err(e) => {
            tracing::warn!(error = %e, "captcha verification response parse failed");
            Err(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn verification_does_not_follow_redirects() {
        let (url, hits) = crate::test_support::redirect_trap().await;
        let cfg = Some(CaptchaConfig {
            instance_url: url,
            site_key: "key".to_string(),
            secret: "s3cret".to_string().into(),
            widget_url: DEFAULT_WIDGET_URL.to_string(),
            wasm_url: None,
        });
        assert!(verify(&cfg, Some("token")).await.is_err());
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    // --- extract_origin tests ---

    #[test]
    fn extract_origin_returns_scheme_and_host() {
        assert_eq!(
            extract_origin("https://cdn.jsdelivr.net/npm/cap-widget"),
            "https://cdn.jsdelivr.net"
        );
    }

    #[test]
    fn extract_origin_handles_root_url() {
        assert_eq!(
            extract_origin("https://cap.example.com"),
            "https://cap.example.com"
        );
    }

    #[test]
    fn extract_origin_handles_trailing_slash() {
        assert_eq!(
            extract_origin("https://cap.example.com/"),
            "https://cap.example.com"
        );
    }

    #[test]
    fn extract_origin_rejects_invalid_url() {
        assert_eq!(extract_origin("not-a-url"), "");
        assert_eq!(extract_origin(""), "");
    }

    #[test]
    fn is_http_url_accepts_only_absolute_http_urls() {
        assert!(is_http_url(
            "https://cap.example.com/assets/cap_wasm_bg.wasm"
        ));
        assert!(is_http_url("http://10.0.0.5:3000/cap_wasm_bg.wasm"));
        assert!(!is_http_url(""));
        assert!(!is_http_url("/assets/cap_wasm_bg.wasm"));
        assert!(!is_http_url("javascript:alert(1)"));
        assert!(!is_http_url("file:///etc/passwd"));
    }

    #[test]
    fn wasm_origin_is_none_without_a_wasm_url() {
        let mut cfg = CaptchaConfig {
            instance_url: "https://cap.example.com".to_string(),
            site_key: "key".to_string(),
            secret: "s".to_string().into(),
            widget_url: DEFAULT_WIDGET_URL.to_string(),
            wasm_url: None,
        };
        assert_eq!(cfg.wasm_origin(), None);
        cfg.wasm_url = Some("https://assets.example.com:8443/cap/cap_wasm_bg.wasm".to_string());
        assert_eq!(
            cfg.wasm_origin().as_deref(),
            Some("https://assets.example.com:8443")
        );
        assert_eq!(
            CaptchaVars::from_config(&Some(cfg)).wasm_url,
            "https://assets.example.com:8443/cap/cap_wasm_bg.wasm"
        );
    }

    // --- api_endpoint tests ---

    #[test]
    fn api_endpoint_strips_trailing_slash_on_instance() {
        let cfg = CaptchaConfig {
            instance_url: "https://cap.example.com/".to_string(),
            site_key: "mykey".to_string(),
            secret: "s".to_string().into(),
            widget_url: DEFAULT_WIDGET_URL.to_string(),
            wasm_url: None,
        };
        assert_eq!(cfg.api_endpoint(), "https://cap.example.com/mykey/");
    }

    #[test]
    fn api_endpoint_no_trailing_slash_on_instance() {
        let cfg = CaptchaConfig {
            instance_url: "https://cap.example.com".to_string(),
            site_key: "mykey".to_string(),
            secret: "s".to_string().into(),
            widget_url: DEFAULT_WIDGET_URL.to_string(),
            wasm_url: None,
        };
        assert_eq!(cfg.api_endpoint(), "https://cap.example.com/mykey/");
    }

    // --- verify passthrough tests (no network) ---

    #[tokio::test]
    async fn verify_passes_through_when_no_config() {
        assert!(verify(&None, None).await.is_ok());
        assert!(verify(&None, Some("any-token")).await.is_ok());
    }

    #[tokio::test]
    async fn verify_fails_when_config_set_and_token_missing() {
        let cfg = Some(CaptchaConfig {
            instance_url: "https://cap.example.com".to_string(),
            site_key: "key".to_string(),
            secret: "secret".to_string().into(),
            widget_url: DEFAULT_WIDGET_URL.to_string(),
            wasm_url: None,
        });
        assert!(verify(&cfg, None).await.is_err());
    }

    #[tokio::test]
    async fn verify_fails_when_config_set_and_token_empty() {
        let cfg = Some(CaptchaConfig {
            instance_url: "https://cap.example.com".to_string(),
            site_key: "key".to_string(),
            secret: "secret".to_string().into(),
            widget_url: DEFAULT_WIDGET_URL.to_string(),
            wasm_url: None,
        });
        assert!(verify(&cfg, Some("")).await.is_err());
        assert!(verify(&cfg, Some("   ")).await.is_err());
    }
}
