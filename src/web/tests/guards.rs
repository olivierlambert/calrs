//! Structural guards over templates and sources (escaping, lang, untranslated text).

use super::*;

// ====== XSS regression guards (#43) ======
//
// Three dashboard templates used to embed user-controlled strings inside
// inline onclick JS string literals using `\'{{ var }}\'` as a naïve
// escape. MiniJinja auto-escapes `'` to `&#x27;` but leaves backslashes
// untouched, so a payload like `\\'));alert(1);//` breaks out of the JS
// string and injects script. Fix was to move the value into a
// `data-confirm` attribute and read it via `this.dataset.confirm` so it's
// never re-parsed as JS.
//
// These tests render each template with a crafted payload and assert
// that (a) the onclick is the safe static form and (b) the payload only
// lands in data-confirm (HTML-escaped). They fire if anyone re-introduces
// a `{{ … }}` interpolation inside an onclick.

/// Pull the bytes of the `onclick="…"` attribute for the first `<button>`
/// whose attributes contain `class_marker`. Only considers button-element
/// attribute strings (between `<button` and the first `>` after it), so
/// CSS/JS elsewhere in the document that happens to mention the marker
/// doesn't throw off the scan. Returns None if no such button exists.
fn extract_onclick_for_button(html: &str, class_marker: &str) -> Option<String> {
    // Skip the prefix before the first `<button` (document boilerplate).
    let mut rest = html;
    while let Some(idx) = rest.find("<button") {
        let after = &rest[idx + "<button".len()..];
        let (attrs, tail) = after.split_once('>')?;
        if attrs.contains(class_marker) {
            if let Some(attr_rest) = attrs.split_once("onclick=\"") {
                if let Some((value, _)) = attr_rest.1.split_once('"') {
                    return Some(value.to_string());
                }
            }
        }
        rest = tail;
    }
    None
}

/// Sentences that interpolate markup are rendered through `t(...) | safe`,
/// which turns off autoescaping for the whole string. Every value spliced
/// into one must therefore be escaped explicitly. A guest picks their own
/// name, so forgetting it once is stored XSS on the host's dashboard.
#[test]
fn safe_interpolated_sentences_escape_their_values() {
    let mut env = minijinja::Environment::new();
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Lenient);
    env.set_loader(minijinja::path_loader("templates"));
    crate::i18n::register(&mut env);

    let booking_row = context! {
        id => "b1",
        event_title => XSS_PAYLOAD_HTML,
        guest_name => XSS_PAYLOAD_HTML,
        guest_email => "guest@example.com",
        start_at => "2026-03-16 10:00",
        when => context! {},
    };
    let cases: Vec<(&str, minijinja::Value)> = vec![
        (
            "dashboard_bookings.html",
            context! {
                lang => "en", sidebar => context! {},
                bookings => vec![booking_row.clone()],
                pending_bookings => vec![booking_row.clone()],
                claimable_bookings => vec![booking_row.clone()],
            },
        ),
        (
            "dashboard_overview.html",
            context! {
                lang => "en", sidebar => context! {},
                pending_bookings => vec![booking_row.clone()],
                mast => context! {}, stats => context! {}, week => context! {},
            },
        ),
        (
            "overrides.html",
            context! { lang => "en", sidebar => context! {}, event_type_title => XSS_PAYLOAD_HTML },
        ),
        (
            "invite_form.html",
            context! { lang => "en", sidebar => context! {}, event_type_title => XSS_PAYLOAD_HTML },
        ),
        (
            "troubleshoot.html",
            context! { lang => "en", sidebar => context! {}, et_title => XSS_PAYLOAD_HTML },
        ),
        (
            "source_write_setup.html",
            context! { lang => "en", source_name => XSS_PAYLOAD_HTML },
        ),
        (
            "source_calendars.html",
            context! {
                lang => "en", sidebar => context! {}, source_name => XSS_PAYLOAD_HTML,
                calendars => vec![context! { id => "c1", name => XSS_PAYLOAD_HTML, sync_enabled => true, event_count => 0 }],
            },
        ),
        (
            "auth/register.html",
            context! { lang => "en", allowed_domains => XSS_PAYLOAD_HTML },
        ),
    ];

    for (name, ctx) in cases {
        let rendered = env
            .get_template(name)
            .unwrap_or_else(|e| panic!("{name} loads: {e}"))
            .render(ctx)
            .unwrap_or_else(|e| panic!("{name} renders: {e}"));
        assert!(
            !rendered.contains(XSS_PAYLOAD_HTML),
            "{name} emitted an unescaped interpolated value"
        );
        assert!(
            rendered.contains("&lt;script&gt;"),
            "{name} did not render the payload at all — the test context is wrong"
        );
    }
}

const XSS_PAYLOAD: &str = r#"\\'));alert(1);//"#;

/// The booking error page is the one screen a guest sees when something
/// goes wrong, so it must never fall back to English. Every argument to
/// the render helpers has to arrive already translated (or as a Fluent
/// key), which means no call site may pass a bare English sentence.
/// Fluent keys are kebab-case and contain no spaces; an English sentence
/// does. That is the whole check.
#[test]
fn booking_action_error_call_sites_pass_no_english() {
    const HELPERS: &[&str] = &[
        "render_booking_action_error_in(",
        "render_booking_action_error_html_in(",
        "render_booking_action_error_keys_in(",
    ];

    let src: String = web_source_files()
        .iter()
        .map(|file| {
            let src = std::fs::read_to_string(file).expect("read source");
            src.split("\nmod tests {")
                .next()
                .unwrap_or(&src)
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");

    let mut offenders = Vec::new();
    let mut checked = 0usize;
    for helper in HELPERS {
        let mut from = 0usize;
        while let Some(off) = src[from..].find(helper) {
            let at = from + off;
            from = at + helper.len();
            // Skip the definitions themselves.
            let line_start = src[..at].rfind('\n').map(|i| i + 1).unwrap_or(0);
            if src[line_start..at].contains("fn ") {
                continue;
            }
            // Collect the argument list up to the balancing paren.
            let mut depth = 0i32;
            let mut args = String::new();
            for c in src[at + helper.len() - 1..].chars() {
                args.push(c);
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
            }
            checked += 1;
            for literal in args.split('"').skip(1).step_by(2) {
                if literal.contains(' ') {
                    offenders.push(format!("{helper} … {literal:?}"));
                }
            }
        }
    }

    assert!(
        checked >= 20,
        "expected to find the error-page call sites, found {checked} — the scan broke"
    );
    assert!(
        offenders.is_empty(),
        "booking error page rendered from untranslated English:\n  {}",
        offenders.join("\n  ")
    );
}

/// Every host-facing page must pass `lang` into its render context. A
/// handler that forgets renders English regardless of the viewer's saved
/// preference, and nothing fails: the page still looks fine, just in the
/// wrong language. Three handlers shipped that way on this branch before
/// this test existed.
#[test]
fn every_host_render_site_passes_lang() {
    const HOST_TEMPLATES: &[&str] = &[
        "dashboard_overview.html",
        "dashboard_event_types.html",
        "dashboard_bookings.html",
        "dashboard_sources.html",
        "dashboard_teams.html",
        "dashboard_internal.html",
        "dashboard_directory.html",
        "settings.html",
        "admin.html",
        "event_type_form.html",
        "invite_form.html",
        "source_form.html",
        "source_test.html",
        "source_write_setup.html",
        "source_calendars.html",
        "team_form.html",
        "team_settings.html",
        "troubleshoot.html",
        "overrides.html",
        "auth/login.html",
        "auth/register.html",
    ];

    let mut offenders = Vec::new();
    let mut files = web_source_files();
    files.push(std::path::PathBuf::from("src/auth.rs"));
    for file in &files {
        let src = std::fs::read_to_string(file).expect("read source");
        let file = file.display();
        // Only the shipping code: the test module below renders templates
        // with deliberately minimal contexts.
        let src = src.split("\nmod tests {").next().unwrap_or(&src);
        let lines: Vec<&str> = src.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let Some(name) = line
                .split_once("get_template(\"")
                .and_then(|(_, rest)| rest.split_once('"'))
                .map(|(name, _)| name)
            else {
                continue;
            };
            if !HOST_TEMPLATES.contains(&name) {
                continue;
            }
            // The render call follows; take the context block after it.
            let Some(start) = (i..lines.len().min(i + 400)).find(|&j| {
                lines[j].contains("render(context!")
                    || lines[j].contains("render(minijinja::context!")
            }) else {
                continue;
            };
            let mut depth = 0i32;
            let mut block = String::new();
            for line in lines.iter().skip(start).take(150) {
                block.push_str(line);
                block.push('\n');
                depth += line.matches('{').count() as i32;
                depth -= line.matches('}').count() as i32;
                if depth <= 0 && block.lines().count() > 1 {
                    break;
                }
            }
            let passes_lang = block.contains("lang =>") || block.contains("lang,");
            if !passes_lang {
                offenders.push(format!("{file}:{} renders {name} without lang", start + 1));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "host pages rendered without a language:\n{}",
        offenders.join("\n")
    );
}

/// The `| escape` in a safe-filtered `t()` call is load-bearing, and the
/// render test above only covers the sites that exist today. This one is
/// structural: in any `t(...) | safe` call, every operand concatenated into
/// an argument must be a string literal, an `| escape`d expression, or a
/// nested `t()` (whose output is our own translation, not user data).
/// A new site that splices a bare variable in fails here.
#[test]
fn safe_filtered_translations_never_splice_bare_variables() {
    let mut offenders = Vec::new();
    for path in template_paths() {
        let src = std::fs::read_to_string(&path).expect("read template");
        for (n, line) in src.lines().enumerate() {
            if !(line.contains("t(") && line.contains("| safe")) {
                continue;
            }
            for operand in concat_operands(line) {
                let trimmed = operand.trim();
                let literal = trimmed.starts_with('\'') || trimmed.starts_with('"');
                let escaped = trimmed.contains("| escape") || trimmed.contains("|escape");
                let translated = trimmed.starts_with("t(") || trimmed.starts_with("(t(");
                if !(literal || escaped || translated) {
                    offenders.push(format!(
                        "{}:{}: `{}` is spliced into a | safe string without | escape",
                        path.display(),
                        n + 1,
                        trimmed
                    ));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "unescaped values in safe-filtered translations:\n{}",
        offenders.join("\n")
    );
}

/// Every operand of every argument of a `t(...)` call on this line: the
/// argument list is split on top-level commas, then each argument on `~`.
fn concat_operands(line: &str) -> Vec<String> {
    let mut operands = Vec::new();
    let Some(open) = line.find("t(") else {
        return operands;
    };
    let body = &line[open + 2..];

    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut current = String::new();
    for ch in body.chars() {
        match quote {
            Some(q) => {
                current.push(ch);
                if ch == q {
                    quote = None;
                }
            }
            None if ch == '\'' || ch == '"' => {
                current.push(ch);
                quote = Some(ch);
            }
            None if ch == '(' => {
                depth += 1;
                current.push(ch);
            }
            None if ch == ')' && depth > 0 => {
                depth -= 1;
                current.push(ch);
            }
            // End of the t(...) call.
            None if ch == ')' => break,
            None if (ch == ',' || ch == '~') && depth == 0 => {
                operands.push(std::mem::take(&mut current));
            }
            None => current.push(ch),
        }
    }
    operands.push(current);

    operands
        .into_iter()
        .map(|o| {
            let o = o.trim().trim_start_matches('(').trim();
            // Drop a leading `name=` so an argument is judged on its value,
            // not its keyword.
            match o.split_once('=') {
                Some((name, rest))
                    if !name.is_empty()
                        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') =>
                {
                    rest.trim().to_string()
                }
                _ => o.to_string(),
            }
        })
        .filter(|o| !o.is_empty())
        .collect()
}

/// All `.html` files under `templates/`, recursively.
fn template_paths() -> Vec<std::path::PathBuf> {
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

/// The shipping `.rs` files of the web module, for the source-scanning
/// guards above. Test code (`tests/`, `test_support.rs`, `*_tests.rs`)
/// is left out: it renders templates with deliberately minimal contexts.
fn web_source_files() -> Vec<std::path::PathBuf> {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("read src/web dir") {
            let path = entry.expect("dir entry").path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if path.is_dir() {
                if name != "tests" {
                    walk(&path, out);
                }
            } else if name.ends_with(".rs")
                && !name.ends_with("_tests.rs")
                && name != "test_support.rs"
            {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(std::path::Path::new("src/web"), &mut out);
    out.sort();
    out
}

const XSS_PAYLOAD_HTML: &str = "<script>alert(1)</script>";

#[test]
fn dashboard_event_types_delete_button_no_onclick_interpolation() {
    let mut env = minijinja::Environment::new();
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Lenient);
    env.set_loader(minijinja::path_loader("templates"));
    crate::i18n::register(&mut env);
    let tmpl = env
        .get_template("dashboard_event_types.html")
        .expect("template loads");

    let rendered = tmpl
        .render(context! {
            sidebar => context! {},
            username => "alice",
            has_any => true,
            all_event_types => vec![context! {
                id => "et-1",
                slug => "intro",
                title => XSS_PAYLOAD,
                duration_min => 30,
                enabled => true,
                visibility => "public",
                can_manage => true,
                active_bookings => 0,
                is_team => false,
                edit_url => "/e/edit",
                toggle_url => "/e/toggle",
                overrides_url => "/e/overrides",
                embed_url => "/e/embed",
                delete_url => "/e/delete",
                view_url => "/e/view",
            }],
        })
        .expect("renders");

    // The delete button uses the shared two-step confirmation: no inline
    // JavaScript at all, the question lives in data-confirm and the form is
    // named by id.
    assert!(
        rendered.contains(r#"data-confirm-form="delete-et-et-1""#),
        "delete button names its form"
    );
    assert_eq!(
        extract_onclick_for_button(&rendered, r#"class="danger""#),
        None,
        "delete button must carry no inline JavaScript",
    );
    // The payload should appear only inside the data-confirm attribute,
    // HTML-escaped. Backslashes pass through unchanged, apostrophes
    // become &#x27; — including the two the sentence itself puts around
    // the title, since the whole string now comes back from Fluent and is
    // escaped in one piece. The browser decodes them identically.
    assert!(
        rendered.contains(
            r#"data-confirm="Delete event type &#x27;\\&#x27;));alert(1);&#x2f;&#x2f;&#x27;"#
        ),
        "payload should be inside data-confirm only"
    );
}

#[test]
fn dashboard_sources_remove_button_no_onclick_interpolation() {
    let mut env = minijinja::Environment::new();
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Lenient);
    env.set_loader(minijinja::path_loader("templates"));
    crate::i18n::register(&mut env);
    let tmpl = env
        .get_template("dashboard_sources.html")
        .expect("template loads");

    let rendered = tmpl
        .render(context! {
            sidebar => context! {},
            sources => vec![context! {
                id => "s-1",
                name => XSS_PAYLOAD,
                url => "https://example.com/dav",
                username => "alice",
                enabled => true,
                last_synced => "never",
                calendar_count => 0,
                event_count => 0,
                needs_write_setup => false,
            }],
        })
        .expect("renders");

    // The Remove button uses the shared two-step confirmation: no inline
    // JavaScript, the question lives in data-confirm.
    assert!(
        rendered.contains(r#"data-confirm-form="remove-source-s-1""#),
        "remove button names its form"
    );
    assert_eq!(
        extract_onclick_for_button(&rendered, "source-remove"),
        None,
        "remove button must carry no inline JavaScript",
    );
    assert!(
        rendered.contains(
            r#"data-confirm="Remove source &#x27;\\&#x27;));alert(1);&#x2f;&#x2f;&#x27;"#
        ),
        "payload should be inside data-confirm only"
    );
}

#[test]
fn team_settings_delete_button_no_onclick_interpolation() {
    let mut env = minijinja::Environment::new();
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Lenient);
    env.set_loader(minijinja::path_loader("templates"));
    crate::i18n::register(&mut env);
    let tmpl = env
        .get_template("team_settings.html")
        .expect("template loads");

    let rendered = tmpl
        .render(context! {
            sidebar => context! {},
            team_id => "t-1",
            team_name => XSS_PAYLOAD,
            team_slug => "acme",
            team_description => "",
            team_avatar_path => minijinja::value::Value::from(None::<String>),
            team_visibility => "public",
            invite_token => minijinja::value::Value::from(None::<String>),
            members => Vec::<minijinja::Value>::new(),
            linked_groups => Vec::<minijinja::Value>::new(),
            available_groups => Vec::<minijinja::Value>::new(),
            can_admin => true,
            is_owner => true,
        })
        .expect("renders");

    let onclick = extract_onclick_for_button(&rendered, "border-color: var(--error-text)")
        .expect("delete team button onclick present");
    assert_eq!(
        onclick, "if(confirm(this.dataset.confirm)) this.nextElementSibling.submit();",
        "onclick must be the static safe form — no interpolation allowed",
    );
    assert!(
        rendered
            .contains(r#"data-confirm="Delete team &#x27;\\&#x27;));alert(1);&#x2f;&#x2f;&#x27;"#),
        "payload should be inside data-confirm only"
    );
}

// ====== Dashboard identity guard ("the timetable") ======
//
// The dashboard was redesigned away from the stock look of generated UI:
// coloured stripes on one side of a card, rounded tops on stacked bars,
// pill shapes, glass blur, tracked capitals, em dashes. This guard reads the
// dashboard templates and fails when one of those comes back. The admin
// panel is restyled separately and is not in the list yet.

const IDENTITY_TEMPLATES: &[&str] = &[
    "dashboard_base.html",
    "dashboard_overview.html",
    "admin.html",
    "source_calendars.html",
    "dashboard_bookings.html",
    "dashboard_event_types.html",
    "dashboard_sources.html",
    "dashboard_teams.html",
    "dashboard_internal.html",
    "dashboard_directory.html",
    "partials/directory_entries.html",
    "partials/icons.html",
    "settings.html",
    "event_type_form.html",
    "team_form.html",
    "team_settings.html",
    "source_form.html",
    "source_test.html",
    "source_write_setup.html",
    "invite_form.html",
    "overrides.html",
    "troubleshoot.html",
];

/// The tells found in `css` (a template's text), as human-readable lines.
fn identity_tells(css: &str) -> Vec<String> {
    let mut found = Vec::new();
    let lower = css.to_ascii_lowercase();
    // A side stripe: a left border or inset shadow wider than a hairline.
    for (pattern, what) in [
        ("border-left:", "border-left stripe"),
        ("border-left-width:", "border-left stripe"),
    ] {
        let mut from = 0;
        while let Some(pos) = lower[from..].find(pattern) {
            let at = from + pos + pattern.len();
            let value: String = lower[at..]
                .trim_start()
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            // A transparent border draws nothing (CSS triangles use it).
            let declaration = lower[at..].split(';').next().unwrap_or("");
            if value.parse::<u32>().is_ok_and(|w| w > 1) && !declaration.contains("transparent") {
                found.push(format!(
                    "{what}: {}",
                    &css[from + pos..(at + 12).min(css.len())]
                ));
            }
            from = at;
        }
    }
    for w in 2..=9 {
        if lower.contains(&format!("inset {w}px 0 0")) {
            found.push(format!("inset {w}px side stripe"));
        }
    }
    for (needle, what) in [
        ("999px", "pill radius (999px)"),
        ("backdrop-filter", "backdrop blur"),
        ("text-transform: uppercase", "all-caps label"),
        ("text-transform:uppercase", "all-caps label"),
        ("\u{2014}", "em dash"),
    ] {
        if lower.contains(needle) {
            found.push(what.to_string());
        }
    }
    // Rounded data marks: a CSS rule for a bar, stack segment, ruler block
    // or track that sets a radius.
    let mut rest = css;
    while let Some(open) = rest.find('{') {
        let selector = rest[..open].rsplit(['}', '\n']).next().unwrap_or("");
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        let body = &rest[open + 1..open + close];
        let is_mark = [
            ".bar",
            ".stack",
            ".blk",
            ".tb",
            ".ov-stack",
            ".adm-seg",
            ".ts-blk",
            ".g.",
        ]
        .iter()
        .any(|m| selector.contains(m));
        if is_mark && body.contains("border-radius") && !body.contains("border-radius: 0") {
            found.push(format!("rounded data mark: {}", selector.trim()));
        }
        rest = &rest[open + close + 1..];
    }
    found
}

#[test]
fn dashboard_templates_keep_the_identity() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let mut problems = Vec::new();
    for name in IDENTITY_TEMPLATES {
        let text =
            std::fs::read_to_string(root.join(name)).unwrap_or_else(|e| panic!("read {name}: {e}"));
        for tell in identity_tells(&text) {
            problems.push(format!("{name}: {tell}"));
        }
    }
    assert!(
        problems.is_empty(),
        "dashboard identity tells:\n{}",
        problems.join("\n")
    );
}

#[test]
fn identity_guard_catches_each_tell() {
    for (css, tell) in [
        (
            ".card { border-left: 3px solid var(--accent); }",
            "border-left",
        ),
        (
            ".row { box-shadow: inset 3px 0 0 var(--accent); }",
            "inset 3px",
        ),
        (
            ".ov-stack { border-radius: 4px 4px 0 0; }",
            "rounded data mark",
        ),
        (".pill { border-radius: 999px; }", "pill"),
        (".bar { backdrop-filter: blur(8px); }", "backdrop"),
        ("<title>Overview \u{2014} calrs</title>", "em dash"),
        (".x { text-transform: uppercase; }", "all-caps"),
    ] {
        let found = identity_tells(css);
        assert!(
            found.iter().any(|f| f.contains(tell)),
            "{css:?} should be caught as {tell}, got {found:?}"
        );
    }
    // A hairline rule and a square bar are fine.
    assert!(identity_tells(
        ".x { border-left: 1px solid var(--rule); } .bar .s { min-height: 2px; } .sort::after { border-left: 3.5px solid transparent; }"
    )
    .is_empty());
}
// ====== Guest email footer guard (#223) ======

/// Every non-test construction of the structs that carry a guest email's
/// content must say where the guest footer comes from: set `guest_footer`
/// (a loader result, or `None` for a host-only or calendar-only value) or copy
/// it from another value with `..other.clone()`. `..Default::default()` alone
/// would silently send that guest email without the footer, which is how a
/// new send path would forget it.
///
/// `writeback.rs` and `calendar_push.rs` only build values to write calendar
/// events, never to send an email.
#[test]
fn every_guest_email_details_literal_sets_the_guest_footer() {
    let mut files = web_source_files();
    for entry in std::fs::read_dir("src/commands").expect("read src/commands") {
        let path = entry.expect("dir entry").path();
        if path.extension().is_some_and(|e| e == "rs") {
            files.push(path);
        }
    }

    let mut offenders = Vec::new();
    for path in files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if name == "writeback.rs" || name == "calendar_push.rs" {
            continue;
        }
        let src = std::fs::read_to_string(&path).expect("read source");
        for (line, ty) in details_literals_without_footer(&src) {
            offenders.push(format!("{}:{line} ({ty})", path.display()));
        }
    }
    assert!(
        offenders.is_empty(),
        "these literals neither set `guest_footer` nor copy it from another value: {offenders:#?}"
    );
}

/// The line and type of each `BookingDetails`, `CancellationDetails` or
/// `RescheduleDetails` literal in `src` that neither sets `guest_footer` nor
/// copies another value with `..other`. Matches the type however it is
/// qualified (`email::BookingDetails {`, `crate::email::BookingDetails {`, or
/// bare after a `use`), and skips what is not a value: a declaration
/// (`struct`, `impl`, `for`) or a return type (`->`).
fn details_literals_without_footer(src: &str) -> Vec<(usize, &'static str)> {
    let mut found = Vec::new();
    for ty in ["BookingDetails", "CancellationDetails", "RescheduleDetails"] {
        let needle = format!("{ty} {{");
        let mut from = 0;
        while let Some(pos) = src[from..].find(&needle) {
            let start = from + pos;
            let open = start + needle.len() - 1;
            from = open + 1;
            // A longer name ending in this one (`MyBookingDetails {`).
            if src[..start]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_')
            {
                continue;
            }
            let word = src[..start]
                .trim_end()
                .rsplit(char::is_whitespace)
                .next()
                .unwrap_or("");
            if matches!(word, "struct" | "impl" | "for" | "enum") || word.ends_with("->") {
                continue;
            }
            let mut depth = 0usize;
            let mut end = open;
            for (i, c) in src[open..].char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = open + i;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let body = &src[open..end];
            // A struct base (`..other`) follows the opening brace or a comma;
            // a range (`get(..10)`, `[..n]`) does not.
            let copies_another = body.match_indices("..").any(|(at, _)| {
                body[..at].trim_end().ends_with([',', '{'])
                    && !body[at..].starts_with("..Default::default()")
            });
            if !body.contains("guest_footer:") && !copies_another {
                found.push((src[..open].matches('\n').count() + 1, ty));
            }
            from = end.max(from);
        }
    }
    found
}

/// The guard catches a literal however its type is written, and leaves
/// declarations and return types alone.
#[test]
fn the_guest_footer_guard_matches_qualified_and_unqualified_literals() {
    let src = r#"
pub struct BookingDetails {
    pub uid: String,
}
impl Default for CancellationDetails {
    fn default() -> Self { todo!() }
}
fn build() -> RescheduleDetails {
    RescheduleDetails { uid: x, guest_footer: None, ..Default::default() }
}
fn a() { let d = email::BookingDetails { uid: x, ..Default::default() }; }
fn b() { let d = BookingDetails { uid: x, ..Default::default() }; }
fn c() { let d = crate::email::CancellationDetails {
    uid: x,
    ..Default::default()
}; }
fn e() { let d = BookingDetails { guest_footer: None, ..Default::default() }; }
fn f() { let d = BookingDetails { uid: x, ..other.clone() }; }
fn g() { let d = MyBookingDetails { uid: x }; }
fn h() { let d = BookingDetails { date: s.get(..10).into(), ..Default::default() }; }
"#;
    let found = details_literals_without_footer(src);
    assert_eq!(
        found,
        vec![
            (11, "BookingDetails"),
            (12, "BookingDetails"),
            (20, "BookingDetails"),
            (13, "CancellationDetails")
        ]
    );
}
