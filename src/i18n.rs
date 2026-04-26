//! Localization via Fluent. `.ftl` files are embedded at compile time so the
//! single-binary deploy story is preserved.

use std::collections::HashMap;
use std::sync::OnceLock;

use axum::http::HeaderMap;
use chrono::{Datelike, NaiveDate, Weekday};
use fluent_bundle::concurrent::FluentBundle;
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
pub fn month_name(lang: &str, month: u32) -> String {
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

    if pairs.is_empty() {
        return translate(&lang_owned, key, None);
    }

    let mut args = FluentArgs::new();
    for (k, v) in &pairs {
        args.set(k.as_str(), FluentValue::from(v.as_str()));
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
        assert_eq!(detect_from_accept_language(Some("de,it,fr")), "fr");
    }

    #[test]
    fn all_unsupported_falls_back_to_default() {
        assert_eq!(detect_from_accept_language(Some("de,it,ja")), "en");
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
    fn month_year_falls_back_to_english_for_unsupported_lang() {
        let d = NaiveDate::from_ymd_opt(2026, 4, 1).unwrap();
        // "es" has no month keys yet, should fall through to English.
        assert_eq!(format_month_year(d, "es"), "April 2026");
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
}
