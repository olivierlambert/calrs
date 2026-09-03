//! Localization via Fluent. `.ftl` files are embedded at compile time so the
//! single-binary deploy story is preserved.

use std::collections::HashMap;
use std::sync::OnceLock;

use axum::http::HeaderMap;
use chrono::{Datelike, NaiveDate, Weekday};
use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::types::{FluentNumber, FluentNumberOptions};
use fluent_bundle::{FluentArgs, FluentResource, FluentValue};
use minijinja::value::Kwargs;
use minijinja::{Environment, State};
use unic_langid::LanguageIdentifier;

// Single source of truth: (BCP-47 code, native display label, embedded .ftl source).
// Add a new language by appending a row here; the bundle, the Accept-Language
// matcher, and the settings dropdown all read from this same array.
const SUPPORTED_LANGS: &[(&str, &str, &str)] = &[
    ("en", "English", include_str!("../i18n/en/main.ftl")),
    ("fr", "Français", include_str!("../i18n/fr/main.ftl")),
    ("es", "Español", include_str!("../i18n/es/main.ftl")),
    ("pl", "Polski", include_str!("../i18n/pl/main.ftl")),
    ("de", "Deutsch", include_str!("../i18n/de/main.ftl")),
    ("it", "Italiano", include_str!("../i18n/it/main.ftl")),
    ("et", "Eesti", include_str!("../i18n/et/main.ftl")),
    (
        "pt",
        "Português (Brasil)",
        include_str!("../i18n/pt/main.ftl"),
    ),
];

const DEFAULT_LANG: &str = "en";

static BUNDLES: OnceLock<HashMap<&'static str, FluentBundle<FluentResource>>> = OnceLock::new();

fn bundles() -> &'static HashMap<&'static str, FluentBundle<FluentResource>> {
    BUNDLES.get_or_init(|| {
        let mut map = HashMap::new();
        for (code, _label, src) in SUPPORTED_LANGS {
            let langid: LanguageIdentifier = code
                .parse()
                .unwrap_or_else(|_| panic!("invalid lang code: {code}"));
            let resource = FluentResource::try_new(src.to_string())
                .unwrap_or_else(|_| panic!("ftl parse error in {code}"));
            let mut bundle = FluentBundle::new_concurrent(vec![langid]);
            // Disable Unicode directional isolates, they break rendering inside HTML.
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
/// Honours quality (`q=`) weights per RFC 7231 §5.3.1: entries are
/// sorted by descending q, ties broken by original order, and the
/// first primary subtag we ship wins.
pub fn detect_from_accept_language(header: Option<&str>) -> &'static str {
    let Some(header) = header else {
        return DEFAULT_LANG;
    };

    let mut entries: Vec<(f32, String)> = header
        .split(',')
        .filter_map(|raw| {
            let mut parts = raw.split(';');
            let tag = parts.next()?.trim();
            if tag.is_empty() {
                return None;
            }
            let primary = tag.split('-').next()?.to_ascii_lowercase();
            if primary.is_empty() {
                return None;
            }
            let q = parts
                .find_map(|p| p.trim().strip_prefix("q="))
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(1.0);
            Some((q, primary))
        })
        .collect();

    // Stable sort by q descending; preserves textual order on ties.
    entries.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    for (_, primary) in &entries {
        for (code, _, _) in SUPPORTED_LANGS {
            if *code == primary.as_str() {
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

/// Whether a given language code matches one of the bundled locales.
pub fn is_supported(code: &str) -> bool {
    SUPPORTED_LANGS.iter().any(|(c, _, _)| *c == code)
}

/// Resolve the language to use for rendering. The user's saved preference
/// (when set and supported) wins over `Accept-Language`. Pass `None` for
/// `user_pref` to skip straight to header detection (e.g. for guests).
pub fn resolve(user_pref: Option<&str>, headers: &HeaderMap) -> &'static str {
    if let Some(pref) = user_pref {
        if let Some((code, _, _)) = SUPPORTED_LANGS.iter().find(|(c, _, _)| *c == pref) {
            return code;
        }
    }
    detect_from_headers(headers)
}

/// All supported languages with their native display labels, for settings dropdowns.
pub fn supported_with_labels() -> impl Iterator<Item = (&'static str, &'static str)> {
    SUPPORTED_LANGS
        .iter()
        .map(|(code, label, _)| (*code, *label))
}

fn weekday_key(d: Weekday) -> &'static str {
    match d {
        Weekday::Mon => "common-weekday-long-mon",
        Weekday::Tue => "common-weekday-long-tue",
        Weekday::Wed => "common-weekday-long-wed",
        Weekday::Thu => "common-weekday-long-thu",
        Weekday::Fri => "common-weekday-long-fri",
        Weekday::Sat => "common-weekday-long-sat",
        Weekday::Sun => "common-weekday-long-sun",
    }
}

/// Render the localized native month name for a 1-indexed month number.
/// Returns English on unsupported locales (via the standard fallback chain).
fn month_name(lang: &str, month: u32) -> String {
    translate(lang, &format!("common-month-{month}"), None)
}

/// "April 2026" / "avril 2026" / "abril 2026" depending on locale.
pub fn format_month_year(date: NaiveDate, lang: &str) -> String {
    let month = month_name(lang, date.month());
    let year = date.year().to_string();
    let mut args = FluentArgs::new();
    args.set("month", FluentValue::from(month.as_str()));
    args.set("year", FluentValue::from(year.as_str()));
    translate(lang, "common-format-month-year", Some(&args))
}

/// "Tuesday, March 12, 2026" / "mardi 12 mars 2026" depending on locale.
/// Year and day are passed as strings to bypass Fluent's locale-aware
/// number formatter (which would otherwise insert grouping separators).
pub fn format_long_date(date: NaiveDate, lang: &str) -> String {
    let weekday = translate(lang, weekday_key(date.weekday()), None);
    let month = month_name(lang, date.month());
    let day = date.day().to_string();
    let year = date.year().to_string();
    let mut args = FluentArgs::new();
    args.set("weekday", FluentValue::from(weekday.as_str()));
    args.set("month", FluentValue::from(month.as_str()));
    args.set("day", FluentValue::from(day.as_str()));
    args.set("year", FluentValue::from(year.as_str()));
    translate(lang, "common-format-long-date", Some(&args))
}

/// Register the `t(key, **kwargs)` function on a minijinja environment.
/// Templates pull the active language from the rendering context's `lang` var.
pub fn register(env: &mut Environment<'static>) {
    env.add_function("t", t_function);
}

/// A template argument on its way into Fluent.
///
/// Integers are kept as numbers so `{ $n -> [one] ... }` plural selectors
/// actually select (a stringified "1" never matches the `one` variant, it
/// silently falls through to `other`). Grouping is switched off so an integer
/// still renders exactly as it did when every argument was a string: "1440",
/// never "1,440".
enum Arg {
    Num(f64),
    Text(String),
}

impl Arg {
    fn from_value(v: &minijinja::Value) -> Self {
        match i64::try_from(v.clone()) {
            Ok(n) => Arg::Num(n as f64),
            Err(_) => Arg::Text(v.to_string()),
        }
    }

    fn to_fluent(&self) -> FluentValue<'_> {
        match self {
            Arg::Num(n) => number(*n),
            Arg::Text(s) => FluentValue::from(s.as_str()),
        }
    }
}

/// A number for a Fluent argument, formatted the way `t()` formats one coming
/// from a template. Rust call sites building their own `FluentArgs` go through
/// here so the two paths cannot drift: `FluentValue::from(n)` would group
/// thousands, and a plural selector needs a real number rather than a string.
pub fn number<'a>(n: f64) -> FluentValue<'a> {
    FluentValue::Number(FluentNumber::new(
        n,
        FluentNumberOptions {
            use_grouping: false,
            ..Default::default()
        },
    ))
}

fn t_function(state: &State, key: &str, kwargs: Kwargs) -> String {
    let lang_owned: String = state
        .lookup("lang")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| DEFAULT_LANG.to_string());

    // Collect kwargs into FluentArgs. We hold the converted strings in a Vec
    // so FluentArgs (which borrows) stays valid for the format_pattern call.
    let pairs: Vec<(String, Arg)> = kwargs
        .args()
        .filter_map(|name| {
            kwargs
                .get::<minijinja::Value>(name)
                .ok()
                .map(|v| (name.to_string(), Arg::from_value(&v)))
        })
        .collect();

    if pairs.is_empty() {
        return translate(&lang_owned, key, None);
    }

    let mut args = FluentArgs::new();
    for (k, v) in &pairs {
        args.set(k.as_str(), v.to_fluent());
    }
    translate(&lang_owned, key, Some(&args))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_header_falls_back_to_default() {
        assert_eq!(detect_from_accept_language(None), "en");
    }

    #[test]
    fn empty_header_falls_back_to_default() {
        assert_eq!(detect_from_accept_language(Some("")), "en");
    }

    #[test]
    fn exact_supported_tag() {
        assert_eq!(detect_from_accept_language(Some("fr")), "fr");
    }

    #[test]
    fn primary_subtag_extracted_from_region() {
        assert_eq!(detect_from_accept_language(Some("en-US")), "en");
        assert_eq!(detect_from_accept_language(Some("fr-CA")), "fr");
    }

    #[test]
    fn first_listed_wins_when_q_unspecified() {
        // Browsers commonly send the preferred language first without explicit q.
        assert_eq!(
            detect_from_accept_language(Some("fr-CA,fr;q=0.9,en;q=0.5")),
            "fr"
        );
    }

    #[test]
    fn higher_q_overrides_textual_order() {
        // The previous (broken) implementation would have picked en here.
        assert_eq!(detect_from_accept_language(Some("en;q=0.5,fr;q=0.9")), "fr");
    }

    #[test]
    fn unsupported_languages_skipped() {
        // ja and zh aren't shipped; first supported tag wins.
        assert_eq!(detect_from_accept_language(Some("ja,zh,fr")), "fr");
    }

    #[test]
    fn all_unsupported_falls_back_to_default() {
        assert_eq!(detect_from_accept_language(Some("ja,zh,ko")), "en");
    }

    #[test]
    fn q_zero_is_still_considered_for_fallback() {
        // q=0 means "do not accept", but our scan currently treats it as a
        // weak preference. This is fine for our fallback semantics since
        // we'd return the default anyway if nothing matched.
        assert_eq!(detect_from_accept_language(Some("fr;q=0")), "fr");
    }

    #[test]
    fn translate_returns_value_for_existing_key() {
        let v = translate("fr", "confirmed-heading-booked", None);
        assert!(!v.is_empty());
        assert_ne!(v, "confirmed-heading-booked");
    }

    #[test]
    fn translate_falls_back_to_english_on_missing_key_in_locale() {
        // Polish file is seeded but if a future key is missing it should
        // fall back to English rather than emit the raw key.
        let en = translate("en", "confirmed-heading-booked", None);
        let pl = translate("pl", "this-key-definitely-does-not-exist", None);
        assert_eq!(pl, "this-key-definitely-does-not-exist"); // unknown key → key
        assert!(!en.is_empty());
    }

    #[test]
    fn month_year_english() {
        let d = NaiveDate::from_ymd_opt(2026, 4, 1).unwrap();
        assert_eq!(format_month_year(d, "en"), "April 2026");
    }

    #[test]
    fn month_year_french() {
        let d = NaiveDate::from_ymd_opt(2026, 4, 1).unwrap();
        // Lowercase, no comma, French ordering.
        assert_eq!(format_month_year(d, "fr"), "avril 2026");
    }

    #[test]
    fn month_year_spanish() {
        let d = NaiveDate::from_ymd_opt(2026, 4, 1).unwrap();
        assert_eq!(format_month_year(d, "es"), "abril 2026");
    }

    #[test]
    fn month_year_polish() {
        let d = NaiveDate::from_ymd_opt(2026, 4, 1).unwrap();
        assert_eq!(format_month_year(d, "pl"), "kwiecień 2026");
    }

    #[test]
    fn month_year_german() {
        let d = NaiveDate::from_ymd_opt(2026, 4, 1).unwrap();
        // German months are capitalized by grammar.
        assert_eq!(format_month_year(d, "de"), "April 2026");
    }

    #[test]
    fn month_year_italian() {
        let d = NaiveDate::from_ymd_opt(2026, 4, 1).unwrap();
        assert_eq!(format_month_year(d, "it"), "aprile 2026");
    }

    #[test]
    fn long_date_german_with_period_after_day() {
        // 2026-04-27 is a Monday.
        let d = NaiveDate::from_ymd_opt(2026, 4, 27).unwrap();
        assert_eq!(format_long_date(d, "de"), "Montag, 27. April 2026");
    }

    #[test]
    fn long_date_italian_no_comma() {
        let d = NaiveDate::from_ymd_opt(2026, 4, 27).unwrap();
        assert_eq!(format_long_date(d, "it"), "lunedì 27 aprile 2026");
    }

    #[test]
    fn month_year_falls_back_to_english_for_unknown_lang() {
        let d = NaiveDate::from_ymd_opt(2026, 4, 1).unwrap();
        // Unknown locale (e.g. "de") should fall through to English.
        assert_eq!(format_month_year(d, "de"), "April 2026");
    }

    #[test]
    fn long_date_english() {
        // 2026-04-27 is a Monday.
        let d = NaiveDate::from_ymd_opt(2026, 4, 27).unwrap();
        assert_eq!(format_long_date(d, "en"), "Monday, April 27, 2026");
    }

    #[test]
    fn long_date_french_word_order() {
        let d = NaiveDate::from_ymd_opt(2026, 4, 27).unwrap();
        // French puts day before month, no comma.
        assert_eq!(format_long_date(d, "fr"), "lundi 27 avril 2026");
    }

    #[test]
    fn year_does_not_get_thousands_separator() {
        // Regression guard: passing the year as i64 would let Fluent's
        // number formatter add grouping ("2,026" / "2 026"). We pass a
        // pre-stringified value to avoid that.
        let d = NaiveDate::from_ymd_opt(2026, 4, 27).unwrap();
        let en = format_long_date(d, "en");
        let fr = format_long_date(d, "fr");
        assert!(en.contains("2026"));
        assert!(fr.contains("2026"));
        assert!(!en.contains("2,026"));
        assert!(!fr.contains("2 026"));
    }

    /// Every `t("key")` referenced from a template must exist in the English
    /// bundle. Without this, a typo or a key dropped during a refactor renders
    /// the raw message id in the UI and nothing fails until someone sees it.
    #[test]
    fn every_template_key_exists_in_english() {
        let mut missing: Vec<String> = Vec::new();
        for path in template_files() {
            let src = std::fs::read_to_string(&path).expect("read template");
            for key in regex_lite_find_keys(&src) {
                if translate("en", &key, None) == key {
                    missing.push(format!("{}: {}", path.display(), key));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "missing English keys:\n{}",
            missing.join("\n")
        );
    }

    /// Handlers translate too (form validation errors, the troubleshoot
    /// timeline). A typo there renders the raw message id into the page just
    /// as silently as it would from a template.
    #[test]
    fn every_rust_key_exists_in_english() {
        let mut missing: Vec<String> = Vec::new();
        for path in rust_files() {
            let src = std::fs::read_to_string(&path).expect("read source");
            for key in find_rust_keys(&src) {
                if translate("en", &key, None) == key {
                    missing.push(format!("{}: {}", path.display(), key));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "missing English keys:\n{}\n\nA kebab-case `Err(\"...\")` is read as a \
             message id, because that is how the booking-form validators hand one \
             back. If yours is an internal error code rather than a sentence a \
             guest reads, name it something that is not kebab-case.",
            missing.join("\n")
        );
    }

    /// A message id: kebab-case ASCII, which no template name, CSS class or
    /// SQL fragment in this codebase looks like.
    fn looks_like_key(key: &str) -> bool {
        key.contains('-')
            && key
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    }

    /// Message ids passed to `translate(lang, "key", ..)` or `tr1(lang, "key", ..)`,
    /// plus the two indirect forms: the key pairs handed to
    /// `render_booking_action_error_keys(state, headers, "title", "body")`, and the
    /// keys the booking-form validators return as `Err("key")`. Both reach Fluent
    /// through a variable, so without this pass a typo renders the raw id.
    fn find_rust_keys(src: &str) -> Vec<String> {
        let mut keys = Vec::new();
        // Helpers that take message ids as arguments instead of resolving them
        // inline. Add one here when you add one to the code.
        const KEY_TAKING_HELPERS: &[&str] = &["render_booking_action_error_keys("];
        for call in KEY_TAKING_HELPERS {
            let mut i = 0;
            while let Some(pos) = src[i..].find(call) {
                let at = i + pos;
                let start = at + call.len();
                i = start;
                // The guard test in web/mod.rs names this helper in a string
                // literal; scanning from there would run past the call site.
                if src[..at].ends_with('"') {
                    continue;
                }
                let mut depth = 1i32;
                let mut args = String::new();
                for c in src[start..].chars() {
                    match c {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    args.push(c);
                }
                keys.extend(
                    args.split('"')
                        .skip(1)
                        .step_by(2)
                        .filter(|k| looks_like_key(k))
                        .map(str::to_string),
                );
            }
        }
        let mut i = 0;
        while let Some(pos) = src[i..].find("Err(\"") {
            let start = i + pos + 5;
            i = start;
            let Some(end) = src[start..].find('"') else {
                continue;
            };
            let key = &src[start..start + end];
            if looks_like_key(key) {
                keys.push(key.to_string());
            }
        }
        for call in ["translate(", "tr1("] {
            let mut i = 0;
            while let Some(pos) = src[i..].find(call) {
                let start = i + pos + call.len();
                i = start;
                // Skip the lang argument, then read the quoted key.
                let Some(comma) = src[start..].find(',') else {
                    continue;
                };
                let rest = src[start + comma + 1..].trim_start();
                let Some(inner) = rest.strip_prefix('"') else {
                    continue;
                };
                let Some(end) = inner.find('"') else {
                    continue;
                };
                let key = &inner[..end];
                if looks_like_key(key) {
                    keys.push(key.to_string());
                }
            }
        }
        keys
    }

    /// All `.rs` files under `src/`, recursively.
    fn rust_files() -> Vec<std::path::PathBuf> {
        fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).expect("read src dir") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs")
                    // This module's own tests translate a deliberately absent
                    // key to exercise the fallback chain.
                    && !path.ends_with("i18n.rs")
                {
                    out.push(path);
                }
            }
        }
        let mut out = Vec::new();
        walk(std::path::Path::new("src"), &mut out);
        out.sort();
        out
    }

    /// Every template must still parse after a localization pass, in every
    /// shipped locale (a locale only changes the strings, but a template that
    /// fails to load is a 500 on a live page).
    #[test]
    fn every_template_loads() {
        let mut env = Environment::new();
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Lenient);
        env.set_loader(minijinja::path_loader("templates"));
        register(&mut env);
        for path in template_files() {
            let name = path
                .strip_prefix("templates/")
                .expect("template path")
                .to_string_lossy()
                .to_string();
            env.get_template(&name)
                .unwrap_or_else(|e| panic!("{name} failed to load: {e}"));
        }
    }

    /// Collect every `t("key")` / `t('key')` occurrence in a template source.
    fn regex_lite_find_keys(src: &str) -> Vec<String> {
        let mut keys = Vec::new();
        let bytes = src.as_bytes();
        let mut i = 0;
        while let Some(pos) = src[i..].find("t(") {
            let start = i + pos;
            // Reject identifiers that merely end in `t`, e.g. `format(`.
            let prev_ok =
                start == 0 || !bytes[start - 1].is_ascii_alphanumeric() && bytes[start - 1] != b'_';
            i = start + 2;
            if !prev_ok {
                continue;
            }
            let rest = &src[i..];
            let quote = match rest.chars().next() {
                Some(c @ ('"' | '\'')) => c,
                _ => continue,
            };
            let after = &rest[1..];
            let Some(end) = after.find(quote) else {
                continue;
            };
            let key = &after[..end];
            if !key.is_empty()
                && key
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            {
                keys.push(key.to_string());
            }
        }
        keys
    }

    /// All `.html` files under `templates/`, recursively.
    fn template_files() -> Vec<std::path::PathBuf> {
        fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).expect("read templates dir") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|e| e == "html") {
                    out.push(path);
                }
            }
        }
        let mut out = Vec::new();
        walk(std::path::Path::new("templates"), &mut out);
        out.sort();
        out
    }

    /// Fluent plural selectors have to see a number, not a stringified one.
    /// A string "1" never matches the `one` variant and silently falls through
    /// to `other`, which is how "1 members" reaches a page.
    #[test]
    fn integer_args_select_plural_variants() {
        let mut env = Environment::new();
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Lenient);
        register(&mut env);
        env.add_template("p", "{{ t('teams-member-count', count=n) }}")
            .expect("template");
        let tmpl = env.get_template("p").expect("template");
        let render = |n: i64| {
            tmpl.render(minijinja::context! { lang => "en", n => n })
                .expect("render")
        };
        assert_eq!(render(1), "1 member");
        assert_eq!(render(2), "2 members");
        assert_eq!(render(0), "0 members");
    }

    /// Numbers must render without locale grouping. Fluent would otherwise
    /// print a booking notice of 1440 minutes as "1,440" in English and
    /// "1 440" in French, changing copy that used to be a plain string.
    #[test]
    fn integer_args_render_without_grouping() {
        let mut env = Environment::new();
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Lenient);
        register(&mut env);
        env.add_template("p", "{{ t('confirmed-cancel-notice-info', minutes=m) }}")
            .expect("template");
        let tmpl = env.get_template("p").expect("template");
        for lang in ["en", "fr"] {
            let out = tmpl
                .render(minijinja::context! { lang => lang, m => 1440 })
                .expect("render");
            assert!(out.contains("1440"), "{lang}: {out}");
            assert!(
                !out.contains("1,440") && !out.contains("1 440"),
                "{lang}: {out}"
            );
        }
    }

    /// Every shipped locale is complete: a key added to English without a
    /// value in some locale renders English inside an otherwise translated
    /// page, which is the kind of gap nobody reports and everybody sees.
    #[test]
    fn every_locale_covers_every_english_key() {
        let english = message_ids("en");
        let mut gaps = Vec::new();
        for (code, _, _) in SUPPORTED_LANGS {
            if *code == DEFAULT_LANG {
                continue;
            }
            let have = message_ids(code);
            let missing: Vec<&str> = english
                .iter()
                .filter(|id| !have.contains(id))
                .copied()
                .collect();
            if !missing.is_empty() {
                gaps.push(format!("{code}: {}", missing.join(", ")));
            }
        }
        assert!(
            gaps.is_empty(),
            "locales missing English keys:\n{}",
            gaps.join("\n")
        );
    }

    /// Plural messages must carry the categories the locale's own grammar
    /// needs. Polish selects one/few/many for integers; a translation that
    /// only copies English's one/other silently reads wrong for 2 and 5.
    #[test]
    fn plural_messages_carry_the_locale_categories() {
        // Only the categories an integer count can select.
        let required: &[(&str, &[&str])] = &[
            ("pl", &["one", "few", "many"]),
            ("de", &["one", "other"]),
            ("fr", &["one", "other"]),
            ("es", &["one", "other"]),
            ("it", &["one", "other"]),
            ("pt", &["one", "other"]),
            ("et", &["one", "other"]),
        ];
        let plural_keys: Vec<&str> = SUPPORTED_LANGS
            .iter()
            .find(|(c, _, _)| *c == "en")
            .map(|(_, _, src)| *src)
            .unwrap_or("")
            .split("\n\n")
            .filter(|block| block.contains(" ->"))
            .filter_map(|block| block.lines().next()?.split(" =").next())
            .collect();
        assert!(
            !plural_keys.is_empty(),
            "no plural messages found in English"
        );

        let mut problems = Vec::new();
        for (lang, cats) in required {
            let src = SUPPORTED_LANGS
                .iter()
                .find(|(c, _, _)| c == lang)
                .map(|(_, _, src)| *src)
                .unwrap_or("");
            for key in &plural_keys {
                let Some(block) = src
                    .split("\n\n")
                    .find(|b| b.starts_with(&format!("{key} =")))
                else {
                    problems.push(format!("{lang}: {key} missing"));
                    continue;
                };
                for cat in *cats {
                    if !block.contains(&format!("[{cat}]")) {
                        problems.push(format!("{lang}: {key} lacks [{cat}]"));
                    }
                }
                if !block.contains("*[") {
                    problems.push(format!("{lang}: {key} has no default variant"));
                }
            }
        }
        assert!(
            problems.is_empty(),
            "plural category gaps:\n{}",
            problems.join("\n")
        );
    }

    /// Message ids declared in a locale's bundle, read from the .ftl source
    /// (FluentBundle exposes no iterator over its messages).
    fn message_ids(lang: &str) -> Vec<&'static str> {
        let src = SUPPORTED_LANGS
            .iter()
            .find(|(code, _, _)| *code == lang)
            .map(|(_, _, src)| *src)
            .unwrap_or("");
        src.lines()
            .filter_map(|line| {
                let id = line.split(" =").next()?;
                let valid = !id.is_empty()
                    && id.starts_with(|c: char| c.is_ascii_lowercase())
                    && id
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
                (valid && line.starts_with(id) && line[id.len()..].starts_with(" =")).then_some(id)
            })
            .collect()
    }
}
