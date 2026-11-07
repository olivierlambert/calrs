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
/// when it has one, else the instance default, else `None`. A failed lookup
/// is logged and sends the email without a footer: a booking email is never
/// held back for it.
pub async fn guest_footer(pool: &SqlitePool, event_type_id: &str) -> Option<String> {
    match sqlx::query_scalar::<_, Option<String>>(
        "SELECT COALESCE(NULLIF(TRIM(et.guest_email_footer, ' ' || char(9, 10, 13)), ''), \
                         NULLIF(TRIM(ac.guest_email_footer, ' ' || char(9, 10, 13)), '')) \
         FROM event_types et \
         LEFT JOIN auth_config ac ON ac.id = 'singleton' \
         WHERE et.id = ?",
    )
    .bind(event_type_id)
    .fetch_optional(pool)
    .await
    {
        Ok(footer) => footer.flatten(),
        Err(e) => {
            tracing::error!(event_type_id = %event_type_id, error = %e, "guest footer lookup failed, sending without it");
            None
        }
    }
}

/// `guest_footer` for the event type of booking `booking_id`.
pub async fn guest_footer_for_booking(pool: &SqlitePool, booking_id: &str) -> Option<String> {
    let et_id = match sqlx::query_scalar::<_, String>(
        "SELECT event_type_id FROM bookings WHERE id = ?",
    )
    .bind(booking_id)
    .fetch_optional(pool)
    .await
    {
        Ok(et_id) => et_id?,
        Err(e) => {
            tracing::error!(booking_id = %booking_id, error = %e, "guest footer lookup failed, sending without it");
            return None;
        }
    };
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

/// A footer after `escape_block_syntax`: the text the parser reads, and the
/// byte offsets in it of the backslashes that function added, so a construct
/// shown as typed can leave them out again.
struct Escaped {
    text: String,
    added: Vec<usize>,
}

impl Escaped {
    /// The source of `range` as the author typed it: what `escape_block_syntax`
    /// added is dropped, everything else (their own backslashes included) is
    /// kept.
    fn typed(&self, range: std::ops::Range<usize>) -> String {
        let mut out = String::with_capacity(range.len());
        let mut last = range.start;
        for &at in self.added.iter().filter(|at| range.contains(at)) {
            out.push_str(&self.text[last..at]);
            last = at + 1;
        }
        out.push_str(&self.text[last..range.end]);
        out
    }
}

/// What the renderers write for one parsed event of a footer.
enum Piece {
    /// Text, as is.
    Text(String),
    /// An unsupported construct shown as the author typed it.
    Typed(String),
    /// Any other event, for the renderer to handle.
    Event(Event<'static>),
}

/// The events of `md`, with every construct the footer does not support
/// (images, inline code) turned into the text the author typed, markers
/// included: the guest reads `![map](https://a.b/m.png)` or `` `code` ``
/// rather than a lone alt text or a word stripped of its backticks, and no
/// image is ever loaded.
fn pieces(md: &str) -> Vec<Piece> {
    let escaped = escape_block_syntax(md);
    let mut out = Vec::new();
    // Inside an image: its nesting depth. Its events are covered by the
    // typed source already pushed for it.
    let mut in_image = 0usize;
    for (event, range) in parser(&escaped.text).into_offset_iter() {
        match event {
            Event::Start(Tag::Image { .. }) => {
                if in_image == 0 {
                    out.push(Piece::Typed(escaped.typed(range)));
                }
                in_image += 1;
            }
            Event::End(TagEnd::Image) => in_image = in_image.saturating_sub(1),
            _ if in_image > 0 => {}
            Event::Code(_) => out.push(Piece::Typed(escaped.typed(range))),
            Event::Text(text) => out.push(Piece::Text(text.into_string())),
            Event::Html(raw) | Event::InlineHtml(raw) => out.push(Piece::Text(raw.into_string())),
            other => out.push(Piece::Event(other.into_static())),
        }
    }
    out
}

/// `md` with every block-level marker turned into literal text, so the parser
/// only ever sees inline markup (bold, italic, links). Without it a line such
/// as `---`, `# Title`, `- item`, `1. item` or `> quote` would be consumed as
/// a rule, heading, list or quote and its marker silently dropped from the
/// email, where the form promises that anything but bold, italic and links is
/// shown as typed. A backslash before ASCII punctuation renders that
/// character, so escaping a marker that turns out not to be one is harmless.
fn escape_block_syntax(md: &str) -> Escaped {
    let mut out = Escaped {
        text: String::with_capacity(md.len() + 8),
        added: Vec::new(),
    };
    for (i, line) in md.split('\n').enumerate() {
        if i > 0 {
            out.text.push('\n');
        }
        let indent = line.bytes().take_while(|b| *b == b' ').count();
        // Four spaces or more is an indented block, which keeps its text.
        if indent > 3 {
            out.text.push_str(line);
            continue;
        }
        let (lead, rest) = line.split_at(indent);
        out.text.push_str(lead);
        escape_line_start(rest, &mut out);
    }
    out
}

/// Whether `line` is a run of `marker` (at least three), spaces and tabs only:
/// a thematic break when `marker` is `*`, `-` or `_`.
fn is_thematic_break(line: &str, marker: char) -> bool {
    line.chars().filter(|c| *c == marker).count() >= 3
        && line.chars().all(|c| c == marker || c == ' ' || c == '\t')
}

/// Push `rest` (a line after at most three spaces) with a marker at its start
/// escaped when it would open a block.
fn escape_line_start(rest: &str, out: &mut Escaped) {
    let escape = match rest.chars().next() {
        Some('#' | '>' | '=' | '+' | '-') => true,
        Some(c @ ('`' | '~')) => {
            let run = rest.chars().take_while(|x| *x == c).count();
            if run >= 3 {
                // A code fence. Escape the whole run: a lone escaped first
                // character would leave two that pair up as an inline code
                // span with the next fence.
                for _ in 0..run {
                    out.backslash();
                    out.text.push(c);
                }
                out.text.push_str(&rest[run..]);
                return;
            }
            false
        }
        Some(c @ ('*' | '_')) => {
            is_thematic_break(rest, c) || (c == '*' && rest[1..].starts_with([' ', '\t']))
        }
        Some('[') => rest.find(']').is_some_and(|end| {
            // A link reference definition: `[label]: target`.
            end > 1 && rest[end + 1..].starts_with(':')
        }),
        Some('0'..='9') => {
            // An ordered list marker: up to nine digits, then `.` or `)`
            // and a space (or nothing). Escape the `.` or `)`.
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            let after = &rest[digits..];
            if digits <= 9
                && after.starts_with(['.', ')'])
                && (after.len() == 1 || after[1..].starts_with([' ', '\t']))
            {
                out.text.push_str(&rest[..digits]);
                out.backslash();
                out.text.push_str(after);
                return;
            }
            false
        }
        _ => false,
    };
    if escape {
        out.backslash();
    }
    out.text.push_str(rest);
}

impl Escaped {
    /// Add an escaping backslash, remembering where.
    fn backslash(&mut self) {
        self.added.push(self.text.len());
        self.text.push('\\');
    }
}

/// The footer as HTML for the body of an email. Every character the author
/// typed is escaped; only the tags produced here are markup.
pub(super) fn guest_footer_html(md: &str) -> String {
    let mut out = String::new();
    // One entry per open link: whether it was rendered as an `<a>`.
    let mut links: Vec<bool> = Vec::new();
    for piece in pieces(md) {
        let event = match piece {
            Piece::Text(text) => {
                out.push_str(&h(&text));
                continue;
            }
            Piece::Typed(text) => {
                out.push_str(&h(&text).replace('\n', "<br>"));
                continue;
            }
            Piece::Event(event) => event,
        };
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
    for piece in pieces(md) {
        let event = match piece {
            Piece::Text(text) | Piece::Typed(text) => {
                out.push_str(&text);
                continue;
            }
            Piece::Event(event) => event,
        };
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
    fn images_and_inline_code_are_kept_as_typed() {
        for (md, plain) in [
            ("![map](https://a.b/m.png)", "![map](https://a.b/m.png)"),
            (
                "See ![the *map*](https://a.b/m.png \"Map\") here",
                "See ![the *map*](https://a.b/m.png \"Map\") here",
            ),
            (
                "![outer ![inner](i.png)](o.png)",
                "![outer ![inner](i.png)](o.png)",
            ),
            ("Dial `*21#` now", "Dial `*21#` now"),
            ("``a ` b``", "``a ` b``"),
            // A line break inside, and a block marker at the start of the
            // next line: the backslash the renderer added is not shown.
            ("![a\n# b](x.png)", "![a\n# b](x.png)"),
            ("`a\n- b`", "`a\n- b`"),
        ] {
            assert_eq!(guest_footer_plain(md), plain, "plain: {md:?}");
            let html = guest_footer_html(md);
            assert!(!html.contains("<img"), "{md:?} -> {html}");
            assert!(!html.contains("<code"), "{md:?} -> {html}");
            let expected = plain
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('"', "&quot;")
                .replace('\n', "<br>");
            assert_eq!(html, expected, "html: {md:?}");
        }
    }

    #[test]
    fn typed_constructs_stay_escaped_and_links_keep_their_policy() {
        let html = guest_footer_html("![<b>x</b>](https://a.b/i.png\"onerror=\"alert(1)) `<i>`");
        assert!(!html.contains("<b>"), "{html}");
        assert!(!html.contains("<i>"), "{html}");
        assert!(!html.contains("\"onerror"), "{html}");
        // An image inside a safe link: the link stays, the image is text.
        let md = "[![logo](https://a.b/l.png)](https://example.com)";
        let html = guest_footer_html(md);
        assert!(html.contains("<a href=\"https://example.com\""), "{html}");
        assert!(html.contains("![logo](https://a.b/l.png)</a>"), "{html}");
        assert!(!html.contains("<img"), "{html}");
        assert_eq!(
            guest_footer_plain(md),
            "![logo](https://a.b/l.png) (https://example.com)"
        );
        // Inline code inside an unsafe link: shown as typed, no link.
        let html = guest_footer_html("[`x`](javascript:alert(1))");
        assert!(!html.contains("<a "), "{html}");
        assert!(html.contains("`x`"), "{html}");
        // The author's own backslashes are what they typed.
        assert_eq!(guest_footer_plain("`a\\*b`"), "`a\\*b`");
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
    fn unsupported_block_syntax_is_kept_as_text() {
        for md in [
            "---",
            "***",
            "* * *",
            "___",
            "# Title",
            "###### Small",
            "- item",
            "+ item",
            "* item",
            "1. item",
            "2) item",
            "> quote",
            "Title\n---",
            "Title\n===",
            "```\ncode\n```",
            "~~~\ncode\n~~~",
            "[x]: https://a.b",
        ] {
            assert_eq!(guest_footer_plain(md), md.trim_end(), "plain: {md:?}");
            let html = guest_footer_html(md);
            let expected = md
                .trim_end()
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('\n', "<br>");
            assert_eq!(html, expected, "html: {md:?}");
        }
    }

    #[test]
    fn indented_marker_lines_keep_their_text() {
        assert_eq!(guest_footer_plain("  - indented"), "- indented");
        assert_eq!(guest_footer_plain("    - four spaces"), "- four spaces");
    }

    #[test]
    fn tab_indented_and_ten_digit_lines_keep_their_text() {
        // A leading tab is an indented block and a list marker has at most
        // nine digits, so neither is markup and nothing needs escaping.
        for md in ["\t- item", "\t# Title", "  \t- item", "1234567890. item"] {
            let text = guest_footer_plain(md);
            assert_eq!(text, md.trim_start_matches([' ', '\t']), "plain: {md:?}");
            assert!(
                guest_footer_html(md).contains(md.trim_start_matches([' ', '\t'])),
                "html: {md:?}"
            );
        }
        // After a paragraph line a tab-indented line is a continuation.
        assert_eq!(guest_footer_plain("a\n\t- b"), "a\n- b");
    }

    #[test]
    fn inline_markup_at_the_start_of_a_line_still_renders() {
        let html = guest_footer_html("**bold** start\n*it* start\n[site](https://example.com)");
        assert!(html.contains("<strong>bold</strong>"), "{html}");
        assert!(html.contains("<em>it</em>"), "{html}");
        assert!(html.contains("<a href=\"https://example.com\""), "{html}");
        assert_eq!(
            guest_footer_plain("**bold** start\n*it* start\n[site](https://example.com)"),
            "bold start\nit start\nsite (https://example.com)"
        );
        // Numbers and dashes that are not markers are untouched.
        assert_eq!(guest_footer_plain("2026 was fine"), "2026 was fine");
        assert_eq!(guest_footer_plain("-5 degrees"), "-5 degrees");
        assert_eq!(guest_footer_plain("`x` y"), "`x` y");
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
