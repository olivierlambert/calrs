//! Localised SMS bodies.
//!
//! Bodies live above the provider trait: what a guest reads must not depend on
//! which gateway happens to be configured. They are translated with the same
//! Fluent catalogue as the emails, using the language stored on the booking, so
//! a French guest gets a French SMS and a French email.
//!
//! Length matters here in a way it never does for email. A GSM-7 message fits
//! 160 characters per billed segment, but a single character outside GSM-7
//! (Polish `ł`, a curly quote, an emoji) switches the whole message to UCS-2 and
//! drops that to 70. Keep the `sms-*` catalogue entries terse, and keep the
//! host-controlled part (the event title) bounded.

use fluent_bundle::{FluentArgs, FluentValue};

use super::{SmsContext, SmsEvent};

/// Event titles are host-controlled and unbounded; past this we shorten so the
/// date and time, which are the point of the message, always survive.
const MAX_TITLE_CHARS: usize = 60;

/// Hard ceiling on a body, as a backstop against a pathological catalogue
/// entry. Two GSM-7 segments' worth.
const MAX_BODY_CHARS: usize = 306;

fn ta<const N: usize>(lang: &str, key: &str, args: [(&str, &str); N]) -> String {
    let mut fa = FluentArgs::new();
    for (k, v) in args.iter() {
        fa.set(*k, FluentValue::from(*v));
    }
    crate::i18n::translate(lang, key, Some(&fa))
}

/// Build the body for a booking event in the guest's language.
pub fn compose(event: SmsEvent, ctx: &SmsContext<'_>) -> String {
    let lang = ctx.lang.unwrap_or("en");
    let key = match event {
        SmsEvent::Confirmed => "sms-confirmed",
        SmsEvent::Cancelled => "sms-cancelled",
        SmsEvent::Rescheduled => "sms-rescheduled",
        SmsEvent::Reminder => "sms-reminder",
        SmsEvent::Expired => "sms-expired",
    };

    let title = shorten(ctx.event_title, MAX_TITLE_CHARS);
    let body = ta(
        lang,
        key,
        [
            ("event", title.as_str()),
            ("date", ctx.date),
            ("time", ctx.start_time),
            ("tz", ctx.timezone),
        ],
    );

    shorten(body.trim(), MAX_BODY_CHARS)
}

/// Truncate on a character boundary, appending an ellipsis when anything was
/// cut. Counts `char`s, not bytes, so accented titles are not chopped mid-code
/// point.
fn shorten(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let kept: String = value.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

/// Whether every character is in the GSM 03.38 basic set, which decides
/// whether a message is billed at 160 or 70 characters per segment.
///
/// The extension table (`{}`, `[]`, `€`, …) is intentionally left out: those
/// characters cost two GSM-7 slots each, so treating them as non-GSM only ever
/// over-estimates the segment count.
fn is_gsm7(body: &str) -> bool {
    const GSM7_EXTRA: &str = "@£$¥èéùìòÇØøÅåΔ_ΦΓΛΩΠΨΣΘΞÆæßÉ!\"#¤%&'()*+,-./:;<=>?¡ÄÖÑÜ§¿äöñüà\n\r ";
    body.chars()
        .all(|c| c.is_ascii_alphanumeric() || GSM7_EXTRA.contains(c))
}

/// Estimated billed segments for a body. Used for logging and to keep the
/// catalogue honest in tests; the gateway's own count is authoritative.
pub fn estimate_segments(body: &str) -> u32 {
    // GSM-7 bills characters (the basic set is all inside the BMP). UCS-2
    // bills UTF-16 code units, so a character outside the BMP (an emoji in a
    // host's event title) takes two.
    let (len, single, multi) = if is_gsm7(body) {
        (body.chars().count(), 160, 153)
    } else {
        (body.encode_utf16().count(), 70, 67)
    };
    if len <= single {
        1
    } else {
        len.div_ceil(multi) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(title: &'a str, lang: Option<&'a str>) -> SmsContext<'a> {
        SmsContext {
            phone: "+33612345678",
            event_title: title,
            date: "Monday, 17 August 2026",
            start_time: "14:00",
            timezone: "Europe/Paris",
            lang,
        }
    }

    #[test]
    fn composes_every_event_with_the_booking_details() {
        for event in [
            SmsEvent::Confirmed,
            SmsEvent::Cancelled,
            SmsEvent::Rescheduled,
            SmsEvent::Reminder,
            SmsEvent::Expired,
        ] {
            let body = compose(event, &ctx("Product demo", None));
            assert!(
                body.contains("Product demo"),
                "{}: {}",
                event.as_str(),
                body
            );
            assert!(body.contains("14:00"), "{}: {}", event.as_str(), body);
            // A missing catalogue entry makes Fluent echo the key back.
            assert!(!body.starts_with("sms-"), "{}: {}", event.as_str(), body);
        }
    }

    #[test]
    fn every_shipped_body_fits_two_segments() {
        for (lang, _label) in crate::i18n::supported_with_labels() {
            for event in [
                SmsEvent::Confirmed,
                SmsEvent::Cancelled,
                SmsEvent::Rescheduled,
                SmsEvent::Reminder,
                SmsEvent::Expired,
            ] {
                let body = compose(event, &ctx("Product demo", Some(lang)));
                let segments = estimate_segments(&body);
                assert!(
                    segments <= 2,
                    "{}/{} would bill {} segments: {}",
                    lang,
                    event.as_str(),
                    segments,
                    body
                );
            }
        }
    }

    #[test]
    fn long_titles_are_shortened_so_the_time_survives() {
        let long = "A".repeat(200);
        let body = compose(SmsEvent::Confirmed, &ctx(&long, None));
        assert!(body.contains('…'));
        assert!(body.contains("14:00"));
        assert!(body.chars().count() <= MAX_BODY_CHARS);
    }

    #[test]
    fn segment_estimate_switches_to_ucs2_on_non_gsm_characters() {
        let ascii = "a".repeat(160);
        assert_eq!(estimate_segments(&ascii), 1);
        // One Polish character forces UCS-2, so the same length costs more.
        let polish = format!("{}ł", "a".repeat(159));
        assert!(estimate_segments(&polish) > 1);
    }

    #[test]
    fn segment_estimate_counts_utf16_code_units_in_ucs2() {
        // 70 BMP characters fit one UCS-2 segment, 71 do not.
        assert_eq!(estimate_segments(&"ł".repeat(70)), 1);
        assert_eq!(estimate_segments(&"ł".repeat(71)), 2);
        // An emoji is two UTF-16 code units: 69 characters, 70 units.
        assert_eq!(estimate_segments(&format!("{}🎉", "ł".repeat(68))), 1);
        // 70 characters but 71 units: billed as two segments, which counting
        // characters would have missed.
        assert_eq!(estimate_segments(&format!("{}🎉", "ł".repeat(69))), 2);
        // 34 emoji are 68 units, 36 are 72 (two parts of 67).
        assert_eq!(estimate_segments(&"🎉".repeat(34)), 1);
        assert_eq!(estimate_segments(&"🎉".repeat(36)), 2);
        assert_eq!(estimate_segments(&"🎉".repeat(68)), 3);
    }
}
