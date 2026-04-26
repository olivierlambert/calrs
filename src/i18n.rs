//! Localization via Fluent. `.ftl` files are embedded at compile time so the
//! single-binary deploy story is preserved.

use std::collections::HashMap;
use std::sync::OnceLock;

use axum::http::HeaderMap;
use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource, FluentValue};
use minijinja::value::Kwargs;
use minijinja::{Environment, State};
use unic_langid::LanguageIdentifier;

const SUPPORTED_LANGS: &[(&str, &str)] = &[
    ("en", include_str!("../i18n/en/main.ftl")),
    ("fr", include_str!("../i18n/fr/main.ftl")),
    ("es", include_str!("../i18n/es/main.ftl")),
];

const DEFAULT_LANG: &str = "en";

static BUNDLES: OnceLock<HashMap<&'static str, FluentBundle<FluentResource>>> = OnceLock::new();

fn bundles() -> &'static HashMap<&'static str, FluentBundle<FluentResource>> {
    BUNDLES.get_or_init(|| {
        let mut map = HashMap::new();
        for (code, src) in SUPPORTED_LANGS {
            let langid: LanguageIdentifier = code
                .parse()
                .unwrap_or_else(|_| panic!("invalid lang code: {code}"));
            let resource = FluentResource::try_new(src.to_string())
                .unwrap_or_else(|_| panic!("ftl parse error in {code}"));
            let mut bundle = FluentBundle::new_concurrent(vec![langid]);
            // Disable Unicode directional isolates — they break rendering inside HTML.
            bundle.set_use_isolating(false);
            bundle
                .add_resource(resource)
                .unwrap_or_else(|_| panic!("ftl add resource failed for {code}"));
            map.insert(*code, bundle);
        }
        map
    })
}

/// Translate a key for the given language, with optional Fluent args.
/// Falls back to English on missing key/locale, then to the key itself.
pub fn translate(lang: &str, key: &str, args: Option<&FluentArgs>) -> String {
    let bundles = bundles();
    let bundle = bundles
        .get(lang)
        .or_else(|| bundles.get(DEFAULT_LANG))
        .expect("default bundle missing");

    if let Some(msg) = bundle.get_message(key) {
        if let Some(pattern) = msg.value() {
            let mut errors = vec![];
            return bundle
                .format_pattern(pattern, args, &mut errors)
                .into_owned();
        }
    }

    if lang != DEFAULT_LANG {
        return translate(DEFAULT_LANG, key, args);
    }
    key.to_string()
}

/// Pick a supported language from an `Accept-Language` header value.
/// Quality values are ignored; first matching primary subtag wins.
pub fn detect_from_accept_language(header: Option<&str>) -> &'static str {
    let Some(header) = header else {
        return DEFAULT_LANG;
    };
    for entry in header.split(',') {
        let tag = entry.split(';').next().unwrap_or("").trim();
        let primary = tag.split('-').next().unwrap_or("").to_ascii_lowercase();
        for (code, _) in SUPPORTED_LANGS {
            if *code == primary {
                return code;
            }
        }
    }
    DEFAULT_LANG
}

/// Convenience: pull `Accept-Language` from a `HeaderMap`.
pub fn detect_from_headers(headers: &HeaderMap) -> &'static str {
    let header = headers.get("accept-language").and_then(|v| v.to_str().ok());
    detect_from_accept_language(header)
}

/// Register the `t(key, **kwargs)` function on a minijinja environment.
/// Templates pull the active language from the rendering context's `lang` var.
pub fn register(env: &mut Environment<'static>) {
    env.add_function("t", t_function);
}

fn t_function(state: &State, key: &str, kwargs: Kwargs) -> String {
    let lang_owned: String = state
        .lookup("lang")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| DEFAULT_LANG.to_string());

    // Collect kwargs into FluentArgs. We hold the converted strings in a Vec
    // so FluentArgs (which borrows) stays valid for the format_pattern call.
    let pairs: Vec<(String, String)> = kwargs
        .args()
        .filter_map(|name| {
            kwargs
                .get::<minijinja::Value>(name)
                .ok()
                .map(|v| (name.to_string(), v.to_string()))
        })
        .collect();

    // Surface unused-kwarg errors so typos in templates are caught.
    let _ = kwargs.assert_all_used();

    if pairs.is_empty() {
        return translate(&lang_owned, key, None);
    }

    let mut args = FluentArgs::new();
    for (k, v) in &pairs {
        args.set(k.as_str(), FluentValue::from(v.as_str()));
    }
    translate(&lang_owned, key, Some(&args))
}
