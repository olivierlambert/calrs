//! The guest email footer (#223): a short text, in a markdown subset, added to
//! every email a guest (or an additional attendee) receives.
//!
//! The text is one value for all languages. An event type's own footer
//! replaces the instance default; with neither set nothing is added. Only
//! bold, italic and links survive rendering, anything else the author typed is
//! shown as text, and links are limited to a few safe schemes.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use super::*;

/// Longest footer accepted, counted in characters of the text as typed.
pub const GUEST_FOOTER_MAX_CHARS: usize = 500;

/// Why a posted footer was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum FooterError {
    TooLong,
}

impl FooterError {
    /// The Fluent key of the message to show the author.
    pub fn message_key(&self) -> &'static str {
        match self {
            FooterError::TooLong => "validate-guest-footer-too-long",
        }
    }
}

/// The value to store for a posted footer: line endings normalised, trimmed,
/// `None` when blank. `Err` when it exceeds [`GUEST_FOOTER_MAX_CHARS`].
///
/// Browsers submit a newline as CRLF, so the count is taken after
/// normalising: it matches what the author sees in the form's counter.
pub fn normalize_guest_footer(raw: &str) -> Result<Option<String>, FooterError> {
    let text = raw.replace("\r\n", "\n").replace('\r', "\n");
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    if text.chars().count() > GUEST_FOOTER_MAX_CHARS {
        return Err(FooterError::TooLong);
    }
    Ok(Some(text.to_string()))
}

/// The footer to add to an event type's guest emails: the event type's own
/// when it has one, else the instance default, else `None`.
pub async fn guest_footer(pool: &SqlitePool, event_type_id: &str) -> Option<String> {
    sqlx::query_scalar::<_, Option<String>>(
        "SELECT COALESCE(NULLIF(TRIM(et.guest_email_footer, ' ' || char(9, 10, 13)), ''), \
                         NULLIF(TRIM(ac.guest_email_footer, ' ' || char(9, 10, 13)), '')) \
         FROM event_types et \
         LEFT JOIN auth_config ac ON ac.id = 'singleton' \
         WHERE et.id = ?",
    )
    .bind(event_type_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .flatten()
}

/// `guest_footer` for the event type of booking `booking_id`.
pub async fn guest_footer_for_booking(pool: &SqlitePool, booking_id: &str) -> Option<String> {
    let et_id = sqlx::query_scalar::<_, String>("SELECT event_type_id FROM bookings WHERE id = ?")
        .bind(booking_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()?;
    guest_footer(pool, &et_id).await
}

/// Whether a link target may be rendered as a link. Everything else
/// (`javascript:`, `data:`, relative paths) is shown as plain text.
fn is_safe_link(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    ["http://", "https://", "mailto:", "tel:"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
}

fn parser(md: &str) -> Parser<'_> {
    Parser::new_ext(md, Options::empty())
}

/// The footer as HTML for the body of an email. Every character the author
/// typed is escaped; only the tags produced here are markup.
pub(super) fn guest_footer_html(md: &str) -> String {
    let mut out = String::new();
    // One entry per open link: whether it was rendered as an `<a>`.
    let mut links: Vec<bool> = Vec::new();
    for event in parser(md) {
        match event {
            Event::Start(Tag::Strong) => out.push_str("<strong>"),
            Event::End(TagEnd::Strong) => out.push_str("</strong>"),
            Event::Start(Tag::Emphasis) => out.push_str("<em>"),
            Event::End(TagEnd::Emphasis) => out.push_str("</em>"),
            Event::Start(Tag::Link { dest_url, .. }) => {
                let safe = is_safe_link(&dest_url);
                if safe {
                    out.push_str(&format!(
                        "<a href=\"{}\" style=\"color:#4b5563;text-decoration:underline;\">",
                        h(dest_url.trim())
                    ));
                }
                links.push(safe);
            }
            Event::End(TagEnd::Link) => {
                if links.pop().unwrap_or(false) {
                    out.push_str("</a>");
                }
            }
            Event::Text(text) | Event::Code(text) => out.push_str(&h(&text)),
            Event::Html(raw) | Event::InlineHtml(raw) => out.push_str(&h(&raw)),
            Event::SoftBreak | Event::HardBreak => out.push_str("<br>"),
            Event::End(TagEnd::Paragraph) => out.push_str("<br><br>"),
            Event::End(
                TagEnd::Heading(_)
                | TagEnd::Item
                | TagEnd::CodeBlock
                | TagEnd::BlockQuote(_)
                | TagEnd::HtmlBlock,
            ) => out.push_str("<br>"),
            _ => {}
        }
    }
    while out.ends_with("<br>") {
        out.truncate(out.len() - "<br>".len());
    }
    out
}

/// The footer for the plain-text part: markers dropped, a link written as
/// `label (url)`.
pub(super) fn guest_footer_plain(md: &str) -> String {
    let mut out = String::new();
    // One entry per open link: the target to append when it closes, and where
    // its label starts.
    let mut links: Vec<Option<(String, usize)>> = Vec::new();
    for event in parser(md) {
        match event {
            Event::Start(Tag::Link { dest_url, .. }) => {
                let url = dest_url.trim().to_string();
                links.push(is_safe_link(&url).then_some((url, out.len())));
            }
            Event::End(TagEnd::Link) => {
                if let Some(Some((url, start))) = links.pop() {
                    let label = &out[start..];
                    let bare = url.strip_prefix("mailto:").unwrap_or(&url);
                    if label != url && label != bare {
                        out.push_str(&format!(" ({url})"));
                    }
                }
            }
            Event::Text(text) | Event::Code(text) => out.push_str(&text),
            Event::Html(raw) | Event::InlineHtml(raw) => out.push_str(&raw),
            Event::SoftBreak | Event::HardBreak => out.push('\n'),
            Event::End(TagEnd::Paragraph) => out.push_str("\n\n"),
            Event::End(
                TagEnd::Heading(_)
                | TagEnd::Item
                | TagEnd::CodeBlock
                | TagEnd::BlockQuote(_)
                | TagEnd::HtmlBlock,
            ) => out.push('\n'),
            _ => {}
        }
    }
    out.trim_end().to_string()
}

/// The HTML block placed after an email's content, `None` for no footer.
pub(super) fn guest_footer_block(md: Option<&str>) -> String {
    match md.map(str::trim).filter(|m| !m.is_empty()) {
        Some(md) => format!(
            "<div style=\"margin:20px 0 0;padding-top:16px;border-top:1px solid #e5e7eb;font-size:13px;line-height:1.5;color:#4b5563;\">{}</div>",
            guest_footer_html(md)
        ),
        None => String::new(),
    }
}

/// `plain` (a plain-text body ending in `signature`) with the footer inserted
/// just before the signature. Without a footer, `plain` is returned as is.
pub(super) fn plain_with_guest_footer(
    plain: String,
    signature: &str,
    footer: Option<&str>,
) -> String {
    let Some(md) = footer.map(str::trim).filter(|m| !m.is_empty()) else {
        return plain;
    };
    let text = guest_footer_plain(md);
    match plain.strip_suffix(signature) {
        // A blank line above, however the body before it ended.
        Some(head) => format!("{}\n\n{text}\n\n{signature}", head.trim_end()),
        None => format!("{plain}\n\n{text}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_trims_blank_and_counts_chars() {
        assert_eq!(normalize_guest_footer("  \r\n "), Ok(None));
        assert_eq!(
            normalize_guest_footer("a\r\nb\r\n"),
            Ok(Some("a\nb".to_string()))
        );
        let ok = "ł".repeat(GUEST_FOOTER_MAX_CHARS);
        assert!(normalize_guest_footer(&ok).unwrap().is_some());
        let long = "ł".repeat(GUEST_FOOTER_MAX_CHARS + 1);
        assert_eq!(normalize_guest_footer(&long), Err(FooterError::TooLong));
    }

    #[test]
    fn crlf_does_not_count_twice() {
        let text = "x\r\n".repeat(GUEST_FOOTER_MAX_CHARS / 2);
        assert!(normalize_guest_footer(&text).is_ok());
    }

    #[test]
    fn html_escapes_typed_markup() {
        let html = guest_footer_html("<script>alert(1)</script> & \"q\"");
        assert!(!html.contains("<script>"), "{html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");
        assert!(html.contains("&amp;"), "{html}");
        assert!(html.contains("&quot;q&quot;"), "{html}");
    }

    #[test]
    fn html_renders_bold_italic_and_links() {
        let html = guest_footer_html("**bold** *it* [site](https://example.com/a?b=1&c=2)");
        assert!(html.contains("<strong>bold</strong>"), "{html}");
        assert!(html.contains("<em>it</em>"), "{html}");
        assert!(
            html.contains("<a href=\"https://example.com/a?b=1&amp;c=2\""),
            "{html}"
        );
        assert!(html.contains(">site</a>"), "{html}");
    }

    #[test]
    fn unsafe_link_schemes_become_text() {
        for md in [
            "[x](javascript:alert(1))",
            "[x](JaVaScRiPt:alert(1))",
            "[x](data:text/html;base64,AAAA)",
            "[x](/relative)",
            "[x](vbscript:y)",
        ] {
            let html = guest_footer_html(md);
            assert!(!html.contains("<a "), "{md} -> {html}");
            assert!(html.contains('x'), "{md} -> {html}");
        }
        for md in ["[x](mailto:a@b.c)", "[x](tel:+3312345)", "[x](http://a.b)"] {
            assert!(guest_footer_html(md).contains("<a href="), "{md}");
        }
    }

    #[test]
    fn html_attribute_cannot_be_broken_out_of() {
        let html = guest_footer_html("[x](https://a.b/\"onmouseover=\"alert(1))");
        assert!(!html.contains("\"onmouseover"), "{html}");
    }

    #[test]
    fn html_keeps_line_breaks_and_paragraphs() {
        assert_eq!(guest_footer_html("one\ntwo"), "one<br>two");
        assert_eq!(guest_footer_html("one\n\ntwo"), "one<br><br>two");
    }

    #[test]
    fn raw_html_and_images_are_not_markup() {
        let html = guest_footer_html("<b>x</b> ![alt](https://a.b/i.png)");
        assert!(!html.contains("<b>"), "{html}");
        assert!(!html.contains("<img"), "{html}");
    }

    #[test]
    fn plain_drops_markers_and_spells_out_links() {
        assert_eq!(
            guest_footer_plain("**Call** *us*: [site](https://example.com)"),
            "Call us: site (https://example.com)"
        );
        assert_eq!(
            guest_footer_plain("[https://example.com](https://example.com)"),
            "https://example.com"
        );
        assert_eq!(
            guest_footer_plain("[a@b.c](mailto:a@b.c)"),
            "a@b.c",
            "a mailto link labelled with its address is not repeated"
        );
        assert_eq!(guest_footer_plain("[x](javascript:alert(1))"), "x");
        assert_eq!(guest_footer_plain("one\ntwo\n\nthree"), "one\ntwo\n\nthree");
    }

    #[test]
    fn plain_footer_goes_before_the_signature() {
        let body = "Hello\n\n-- sig".to_string();
        assert_eq!(
            plain_with_guest_footer(body.clone(), "-- sig", Some("**Hi**")),
            "Hello\n\nHi\n\n-- sig"
        );
        assert_eq!(plain_with_guest_footer(body.clone(), "-- sig", None), body);
        assert_eq!(
            plain_with_guest_footer(body.clone(), "-- sig", Some("  ")),
            body
        );
    }

    #[test]
    fn block_is_empty_without_footer() {
        assert_eq!(guest_footer_block(None), "");
        assert_eq!(guest_footer_block(Some(" \n")), "");
        assert!(guest_footer_block(Some("hi")).contains(">hi</div>"));
    }
}
