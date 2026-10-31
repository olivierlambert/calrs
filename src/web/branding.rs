//! Theme CSS, embed pages, and the static assets baked into the binary.

use super::*;

/// Query parameters that the embed-code generator threads into the booking
/// flow. When `embed=1`, the booking pages drop their chrome (logo, footer,
/// theme toggle) and emit a postMessage on load so the parent iframe can
/// auto-size. The other fields are passed straight through to the templates
/// as visual hints. None of them are persisted server-side — the embed code
/// embeds them as URL params on every iframe load.
#[derive(Deserialize, Default, Clone, Debug)]
pub(super) struct EmbedParams {
    #[serde(default)]
    pub(super) embed: Option<String>,
    #[serde(default)]
    pub(super) layout: Option<String>,
    #[serde(default)]
    pub(super) theme: Option<String>,
    #[serde(default)]
    pub(super) brand: Option<String>,
}

impl EmbedParams {
    pub(super) fn is_embedded(&self) -> bool {
        matches!(self.embed.as_deref(), Some("1") | Some("true"))
    }

    /// Full set of accent CSS variable declarations derived from `brand`,
    /// suitable for inlining inside a `:root, html.dark { … }` rule. Returns
    /// None if `brand` is missing, empty, or not 3/6 hex digits — we never pass
    /// raw user input into CSS to avoid breaking out of the rule.
    ///
    /// Besides `--accent`/`--accent-hover` (the brand hex), we derive the rgba
    /// variants `--accent-subtle`/`--accent-border`/`--accent-muted` from the
    /// same color so slot pills, borders and muted states pick up the brand
    /// too — otherwise those would keep the org accent and render half-themed.
    pub(super) fn brand_css(&self) -> Option<String> {
        let raw = self.brand.as_deref()?.trim_start_matches('#');
        let (r, g, b) = hex_to_rgb(raw)?;
        Some(format!(
            "--accent: #{hex}; --accent-hover: #{hex}; \
             --accent-subtle: rgba({r},{g},{b},0.12); \
             --accent-border: rgba({r},{g},{b},0.3); \
             --accent-muted: rgba({r},{g},{b},0.5);",
            hex = raw
        ))
    }

    /// "?embed=1&layout=month&theme=dark&brand=ff0000" — used to keep params
    /// alive across page transitions inside the iframe. Returns empty string
    /// when not embedded so callers can unconditionally append it.
    pub(super) fn query_suffix(&self, leading_amp: bool) -> String {
        if !self.is_embedded() {
            return String::new();
        }
        let mut parts: Vec<String> = vec!["embed=1".to_string()];
        if let Some(v) = self.layout.as_deref().filter(|s| !s.is_empty()) {
            parts.push(format!("layout={}", urlencode_simple(v)));
        }
        if let Some(v) = self.theme.as_deref().filter(|s| !s.is_empty()) {
            parts.push(format!("theme={}", urlencode_simple(v)));
        }
        if let Some(v) = self.brand.as_deref().filter(|s| !s.is_empty()) {
            parts.push(format!("brand={}", urlencode_simple(v)));
        }
        let sep = if leading_amp { "&" } else { "?" };
        format!("{}{}", sep, parts.join("&"))
    }
}

/// Parse a 3- or 6-digit hex color (no leading '#') into (r, g, b) decimal
/// components. Returns None for any other length or non-hex input. A 3-digit
/// value is expanded by doubling each nibble (e.g. "f30" → ff3300).
pub(super) fn hex_to_rgb(raw: &str) -> Option<(u8, u8, u8)> {
    let full = match raw.len() {
        3 if raw.chars().all(|c| c.is_ascii_hexdigit()) => {
            raw.chars().flat_map(|c| [c, c]).collect::<String>()
        }
        6 if raw.chars().all(|c| c.is_ascii_hexdigit()) => raw.to_string(),
        _ => return None,
    };
    let r = u8::from_str_radix(&full[0..2], 16).ok()?;
    let g = u8::from_str_radix(&full[2..4], 16).ok()?;
    let b = u8::from_str_radix(&full[4..6], 16).ok()?;
    Some((r, g, b))
}

/// Minimal URL-encoder for the small whitelist of values EmbedParams carries
/// (slug-like layout names, theme tokens, hex colors). Keeps the helper
/// dependency-free; if we ever accept richer values we can swap in `urlencoding`.
fn urlencode_simple(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// Renders the embed code generator for a personal event type. The page is
/// all client-side once rendered: changing form fields rebuilds the snippet
/// and reloads the preview iframe — nothing is persisted server-side.
pub(super) async fn embed_page(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path(slug): Path<String>,
) -> Response {
    let user = &auth_user.user;

    let et: Option<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT et.id, et.slug, et.title, et.default_calendar_view, et.visibility \
         FROM event_types et \
         JOIN accounts a ON a.id = et.account_id \
         WHERE a.user_id = ? AND et.slug = ? AND et.team_id IS NULL",
    )
    .bind(&user.id)
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (_et_id, et_slug, et_title, default_calendar_view, visibility) = match et {
        Some(e) => e,
        None => return Redirect::to("/dashboard/event-types").into_response(),
    };

    // Username for the cal link path: /u/{username}/{slug}. `users.username` is
    // nullable (migration 003) and all /u/{username} lookups match on it, so a
    // user without a username can't produce a working embed link. Rather than
    // emit a snippet pointing at a 404, render the page with a notice and no
    // snippet (has_username = false).
    let username = user.username.clone().filter(|u| !u.is_empty());
    let has_username = username.is_some();
    let cal_path = match &username {
        Some(u) => format!("/u/{}/{}", u, et_slug),
        None => String::new(),
    };

    let base_url = embed_base_url();
    let accent = org_accent_hex(&state.pool).await;

    let tmpl = match state.templates.get_template("embed.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("template render", &e),
    };

    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);

    Html(
        tmpl.render(context! {
            sidebar => sidebar_context(&auth_user, "event-types"),
            lang => auth_user.lang,
            event_type_title => et_title,
            event_type_slug => et_slug,
            cal_path => cal_path,
            has_username => has_username,
            base_url => base_url,
            default_layout => default_calendar_view,
            accent_color => accent,
            visibility => visibility,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_default(),
    )
    .into_response()
}

/// Absolute base URL for embed snippets: the configured base URL (env or DB)
/// without a trailing slash, or empty (the page then falls back to the
/// dashboard's own origin).
fn embed_base_url() -> String {
    crate::settings::base_url()
        .unwrap_or_default()
        .trim_end_matches('/')
        .to_string()
}

/// Resolve the org accent hex for prefilling the embed brand color. Mirrors
/// build_theme_css precedence: the preset's accent for a preset theme,
/// `custom_accent` only when theme='custom', default blue otherwise.
async fn org_accent_hex(pool: &SqlitePool) -> String {
    let theme_row: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT theme, custom_accent FROM auth_config WHERE id = 'singleton'")
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
    match theme_row {
        Some((ref theme, ref custom)) if theme == "custom" => custom
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "#2563eb".to_string()),
        Some((ref theme, _)) => preset_accent(theme).to_string(),
        None => "#2563eb".to_string(),
    }
}

/// Embed code generator for a team event type. Mirrors `embed_page` but the
/// booking link is the team path `/team/{team_slug}/{event_slug}`, which is
/// always well-formed (teams always have a slug), so there is no missing-link
/// case. Access is restricted to team admins and global admins.
pub(super) async fn group_embed_page(
    State(state): State<Arc<AppState>>,
    auth_user: crate::auth::AuthUser,
    Path((team_id, slug)): Path<(String, String)>,
) -> Response {
    let user = &auth_user.user;
    let is_admin = user.role == "admin";

    if !is_admin && !is_team_admin(&state.pool, &user.id, &team_id).await {
        return Html(crate::i18n::translate(
            auth_user.lang,
            "form-error-not-team-admin",
            None,
        ))
        .into_response();
    }

    let et: Option<(String, String, String, String, String, String)> = sqlx::query_as(
        "SELECT et.slug, et.title, et.default_calendar_view, et.visibility, t.slug, t.visibility \
         FROM event_types et \
         JOIN teams t ON t.id = et.team_id \
         WHERE et.team_id = ? AND et.slug = ?",
    )
    .bind(&team_id)
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let (et_slug, et_title, default_calendar_view, visibility, team_slug, team_visibility) =
        match et {
            Some(e) => e,
            None => return Redirect::to("/dashboard/event-types").into_response(),
        };

    let cal_path = format!("/team/{}/{}", team_slug, et_slug);
    let base_url = embed_base_url();
    let accent = org_accent_hex(&state.pool).await;

    let tmpl = match state.templates.get_template("embed.html") {
        Ok(t) => t,
        Err(e) => return internal_error_response("template render", &e),
    };

    let (impersonating, impersonating_name, _) = impersonation_ctx(&auth_user);

    // Team event types always sit on a real team slug, so the embed link is
    // never dead — has_username is always true here.
    Html(
        tmpl.render(context! {
            sidebar => sidebar_context(&auth_user, "event-types"),
            lang => auth_user.lang,
            event_type_title => et_title,
            event_type_slug => et_slug,
            cal_path => cal_path,
            has_username => true,
            base_url => base_url,
            default_layout => default_calendar_view,
            accent_color => accent,
            visibility => visibility,
            team_visibility => team_visibility,
            impersonating => impersonating,
            impersonating_name => impersonating_name,
        })
        .unwrap_or_default(),
    )
    .into_response()
}

/// Returns CSS that overrides all theme variables for the given preset theme.
fn preset_theme_css(theme: &str) -> &'static str {
    match theme {
        "nord" => concat!(
            ":root{--bg:#eceff4;--surface:#fff;--surface-hover:#e5e9f0;--text:#2e3440;--text-secondary:#4c566a;--text-muted:#7b88a1;",
            "--border:#d8dee9;--border-hover:#b3bdd1;--accent:#5e81ac;--accent-hover:#4c6f97;--accent-subtle:#e8eef5;",
            "--accent-border:#b3cde0;--accent-muted:#81a1c1;--success:#a3be8c;--error-bg:#f5e6e8;--error-text:#bf616a}",
            " html.dark{--bg:#2e3440;--surface:#3b4252;--surface-hover:#434c5e;--text:#eceff4;--text-secondary:#d8dee9;--text-muted:#7b88a1;",
            "--border:#434c5e;--border-hover:#4c566a;--accent:#81a1c1;--accent-hover:#88c0d0;--accent-subtle:rgba(129,161,193,0.12);",
            "--accent-border:rgba(129,161,193,0.3);--accent-muted:#5e81ac;--success:#a3be8c;--error-bg:rgba(191,97,106,0.12);--error-text:#bf616a}"
        ),
        "dracula" => concat!(
            ":root{--bg:#f0edf5;--surface:#fff;--surface-hover:#e8e4ef;--text:#282a36;--text-secondary:#44475a;--text-muted:#7c7f94;",
            "--border:#d6d0e0;--border-hover:#b3adc4;--accent:#bd93f9;--accent-hover:#a76ff0;--accent-subtle:#f3eefe;",
            "--accent-border:#d4bffc;--accent-muted:#caa6fc;--success:#50fa7b;--error-bg:#fce4ec;--error-text:#ff5555}",
            " html.dark{--bg:#282a36;--surface:#44475a;--surface-hover:#4d5068;--text:#f8f8f2;--text-secondary:#d0cfe4;--text-muted:#7c7f94;",
            "--border:#4d5068;--border-hover:#6272a4;--accent:#bd93f9;--accent-hover:#caa6fc;--accent-subtle:rgba(189,147,249,0.12);",
            "--accent-border:rgba(189,147,249,0.3);--accent-muted:#9b6dff;--success:#50fa7b;--error-bg:rgba(255,85,85,0.12);--error-text:#ff5555}"
        ),
        "gruvbox" => concat!(
            ":root{--bg:#f9f5d7;--surface:#fbf1c7;--surface-hover:#f2e5bc;--text:#3c3836;--text-secondary:#504945;--text-muted:#928374;",
            "--border:#d5c4a1;--border-hover:#bdae93;--accent:#d65d0e;--accent-hover:#af3a03;--accent-subtle:#fef0e2;",
            "--accent-border:#f0b886;--accent-muted:#e78a4e;--success:#98971a;--error-bg:#fde8e6;--error-text:#cc241d}",
            " html.dark{--bg:#282828;--surface:#3c3836;--surface-hover:#504945;--text:#ebdbb2;--text-secondary:#d5c4a1;--text-muted:#928374;",
            "--border:#504945;--border-hover:#665c54;--accent:#fe8019;--accent-hover:#fabd2f;--accent-subtle:rgba(254,128,25,0.1);",
            "--accent-border:rgba(254,128,25,0.25);--accent-muted:#d65d0e;--success:#b8bb26;--error-bg:rgba(251,73,52,0.1);--error-text:#fb4934}"
        ),
        "solarized" => concat!(
            ":root{--bg:#fdf6e3;--surface:#eee8d5;--surface-hover:#e8e1cb;--text:#657b83;--text-secondary:#586e75;--text-muted:#93a1a1;",
            "--border:#d6cdb5;--border-hover:#b8b09a;--accent:#268bd2;--accent-hover:#1a6fad;--accent-subtle:#edf5fb;",
            "--accent-border:#a3cee8;--accent-muted:#6aafe2;--success:#859900;--error-bg:#fdf0ed;--error-text:#dc322f}",
            " html.dark{--bg:#002b36;--surface:#073642;--surface-hover:#0a4050;--text:#839496;--text-secondary:#93a1a1;--text-muted:#586e75;",
            "--border:#0a4050;--border-hover:#1a5060;--accent:#268bd2;--accent-hover:#6aafe2;--accent-subtle:rgba(38,139,210,0.1);",
            "--accent-border:rgba(38,139,210,0.25);--accent-muted:#1a6fad;--success:#859900;--error-bg:rgba(220,50,47,0.1);--error-text:#dc322f}"
        ),
        "tokyo-night" => concat!(
            ":root{--bg:#f0f0f5;--surface:#fff;--surface-hover:#e8e8ef;--text:#343b58;--text-secondary:#4c5478;--text-muted:#9099b0;",
            "--border:#d5d6e2;--border-hover:#b0b2c4;--accent:#7a5af5;--accent-hover:#6340db;--accent-subtle:#f0ecfe;",
            "--accent-border:#c4b5fd;--accent-muted:#a78bfa;--success:#41a87a;--error-bg:#fce8ec;--error-text:#e04071}",
            " html.dark{--bg:#1a1b26;--surface:#24283b;--surface-hover:#2f3349;--text:#a9b1d6;--text-secondary:#c0caf5;--text-muted:#565f89;",
            "--border:#2f3349;--border-hover:#414868;--accent:#7aa2f7;--accent-hover:#89b4fa;--accent-subtle:rgba(122,162,247,0.1);",
            "--accent-border:rgba(122,162,247,0.25);--accent-muted:#3d59a1;--success:#9ece6a;--error-bg:rgba(247,118,142,0.1);--error-text:#f7768e}"
        ),
        "vates" => concat!(
            ":root{--bg:#f5f4f0;--surface:#fff;--surface-hover:#faf9f5;--text:#1a1b38;--text-secondary:#3d3e58;--text-muted:#7a7b94;",
            "--border:#e2e1dc;--border-hover:#b5b4ae;--accent:#be1621;--accent-hover:#a01219;--accent-subtle:#fdf1f1;",
            "--accent-border:#f0b8bb;--accent-muted:#d4555e;--success:#2ca878;--error-bg:#fdf1f1;--error-text:#be1621}",
            " html.dark{--bg:#1a1b38;--surface:#262748;--surface-hover:#32335a;--text:#f0efe8;--text-secondary:#c8c7c0;--text-muted:#7a7b94;",
            "--border:#32335a;--border-hover:#4a4b6e;--accent:#e0424c;--accent-hover:#ef7f18;--accent-subtle:rgba(190,22,33,0.12);",
            "--accent-border:rgba(190,22,33,0.3);--accent-muted:#8a1018;--success:#2ca878;--error-bg:rgba(190,22,33,0.12);--error-text:#e0424c}"
        ),
        // "default" (blue) — no overrides needed, base.html defines it
        _ => "",
    }
}

/// Light-mode accent hex for a preset theme, mirroring the `--accent` values in
/// `preset_theme_css`. Used to prefill the embed brand color so it matches the
/// org theme even when a preset (not a custom theme) is active.
fn preset_accent(theme: &str) -> &'static str {
    match theme {
        "nord" => "#5e81ac",
        "dracula" => "#bd93f9",
        "gruvbox" => "#d65d0e",
        "solarized" => "#268bd2",
        "tokyo-night" => "#7a5af5",
        "vates" => "#be1621",
        // "default" (blue) and anything unknown
        _ => "#2563eb",
    }
}

/// Convert a `#rrggbb` color to HSL: hue in degrees [0,360), saturation and
/// lightness in [0,1]. Returns None for anything `hex_to_rgb` rejects.
pub(super) fn hex_to_hsl(hex: &str) -> Option<(f32, f32, f32)> {
    let (r, g, b) = hex_to_rgb(hex.trim().trim_start_matches('#'))?;
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d <= f32::EPSILON {
        return Some((0.0, 0.0, l));
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        60.0 * (((g - b) / d).rem_euclid(6.0))
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    Some((h.rem_euclid(360.0), s, l))
}

/// Convert HSL back to a `#rrggbb` string. Saturation and lightness are clamped
/// to [0,1], so callers can offset a lightness past the ends of the scale.
pub(super) fn hsl_to_hex(h: f32, s: f32, l: f32) -> String {
    let (s, l) = (s.clamp(0.0, 1.0), l.clamp(0.0, 1.0));
    let h = h.rem_euclid(360.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let byte = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b))
}

/// Lightness targets for a derived dark palette, matching the scale the
/// built-in dark theme uses in `base.html` (bg #09090b, surface #18181b,
/// border #27272a, border-hover #52525b, text #fafafa, muted #a1a1aa).
const DARK_BG_L: f32 = 0.04;
const DARK_SURFACE_L: f32 = 0.095;
const DARK_TEXT_L: f32 = 0.97;
/// Offsets applied to the resolved dark surface/text to get the supporting
/// neutrals, so they stay in the same color family as the rest of the palette.
const DARK_BORDER_STEP: f32 = 0.055;
const DARK_BORDER_HOVER_STEP: f32 = 0.23;
const DARK_TEXT_SECONDARY_STEP: f32 = 0.32;
const DARK_TEXT_MUTED_STEP: f32 = 0.34;
/// A tinted near-black reads as brand-colored; a fully saturated one reads as
/// broken. Text carries even less tint so it stays comfortable to read.
const DARK_NEUTRAL_MAX_SAT: f32 = 0.40;
const DARK_TEXT_MAX_SAT: f32 = 0.15;
/// Floor for secondary/muted text, so they never sink into the background when
/// the configured palette is unusual.
const DARK_TEXT_MIN_L: f32 = 0.40;

/// Build custom theme CSS from user-provided hex colors.
///
/// The admin panel only collects one set of colors, which are taken to be the
/// light palette. A dark counterpart is derived from them: hue and (clamped)
/// saturation are kept so the brand tint survives, while lightness is re-mapped
/// onto the built-in dark scale. Emitting the configured colors unchanged under
/// `html.dark` used to leave dark mode fully light (issue #177). A palette that
/// is already dark is passed through as configured.
pub(super) fn custom_theme_css(
    accent: &str,
    accent_hover: &str,
    bg: &str,
    surface: &str,
    text: &str,
) -> String {
    // Validate that all colors look like hex codes
    let validate_hex = |c: &str| -> bool {
        let c = c.trim();
        c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|ch| ch.is_ascii_hexdigit())
    };
    if ![accent, accent_hover, bg, surface, text]
        .iter()
        .all(|c| validate_hex(c))
    {
        return String::new();
    }
    // validate_hex trims before measuring, and the admin form stores whatever
    // was pasted, so trim here too: byte-slicing an untrimmed accent below
    // would read the wrong nibbles and silently emit rgba(0,0,0,..).
    let (accent, accent_hover) = (accent.trim(), accent_hover.trim());
    let (bg, surface, text) = (bg.trim(), surface.trim(), text.trim());

    // Parse accent for subtle/border/muted derivations
    let r = u8::from_str_radix(&accent[1..3], 16).unwrap_or(0);
    let g = u8::from_str_radix(&accent[3..5], 16).unwrap_or(0);
    let b = u8::from_str_radix(&accent[5..7], 16).unwrap_or(0);

    // Anchor the dark neutrals on the background hue: a surface of #fff carries
    // no hue of its own, so deriving it from the surface would drop the tint.
    let (bg_h, bg_s, bg_l) = hex_to_hsl(bg).unwrap_or((0.0, 0.0, 1.0));
    let (text_h, text_s, text_l) = hex_to_hsl(text).unwrap_or((0.0, 0.0, 0.0));
    let neutral_s = bg_s.min(DARK_NEUTRAL_MAX_SAT);
    // Text darker than its background means the palette was designed for light
    // mode, whatever its absolute lightness. Testing the pair rather than the
    // background alone also covers mid-tone backgrounds, which an absolute
    // threshold would leave with dark mode still looking like light mode.
    let (dark_bg, dark_surface, dark_text) = if text_l < bg_l {
        (
            hsl_to_hex(bg_h, neutral_s, DARK_BG_L),
            hsl_to_hex(bg_h, neutral_s, DARK_SURFACE_L),
            hsl_to_hex(text_h, text_s.min(DARK_TEXT_MAX_SAT), DARK_TEXT_L),
        )
    } else {
        (bg.to_string(), surface.to_string(), text.to_string())
    };

    // Supporting neutrals track the resolved surface and text, whether those
    // were derived above or configured as a dark palette to begin with.
    let (surf_h, surf_s, surf_l) = hex_to_hsl(&dark_surface).unwrap_or((0.0, 0.0, DARK_SURFACE_L));
    let (txt_h, txt_s, txt_l) = hex_to_hsl(&dark_text).unwrap_or((0.0, 0.0, DARK_TEXT_L));
    let dark_border = hsl_to_hex(surf_h, surf_s, surf_l + DARK_BORDER_STEP);
    let dark_border_hover = hsl_to_hex(surf_h, surf_s, surf_l + DARK_BORDER_HOVER_STEP);
    let dark_text_secondary = hsl_to_hex(
        txt_h,
        txt_s,
        (txt_l - DARK_TEXT_SECONDARY_STEP).max(DARK_TEXT_MIN_L),
    );
    let dark_text_muted = hsl_to_hex(
        txt_h,
        txt_s,
        (txt_l - DARK_TEXT_MUTED_STEP).max(DARK_TEXT_MIN_L),
    );

    format!(
        ":root{{--bg:{bg};--surface:{surface};--text:{text};--accent:{accent};--accent-hover:{accent_hover};\
         --accent-subtle:rgba({r},{g},{b},0.08);--accent-border:rgba({r},{g},{b},0.25);--accent-muted:rgba({r},{g},{b},0.5)}}\
         html.dark{{--bg:{dark_bg};--surface:{dark_surface};--surface-hover:{dark_border};--text:{dark_text};\
         --text-secondary:{dark_text_secondary};--text-muted:{dark_text_muted};--border:{dark_border};--border-hover:{dark_border_hover};\
         --accent:{accent};--accent-hover:{accent_hover};\
         --accent-subtle:rgba({r},{g},{b},0.12);--accent-border:rgba({r},{g},{b},0.3);--accent-muted:rgba({r},{g},{b},0.5)}}",
    )
}

/// Build the full theme CSS string from DB settings.
pub(super) async fn build_theme_css(pool: &SqlitePool) -> String {
    let row: Option<(String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT theme, custom_accent, custom_accent_hover, custom_bg, custom_surface, custom_text FROM auth_config WHERE id = 'singleton'")
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
    match row {
        Some((ref theme, ref ca, ref cah, ref cb, ref cs, ref ct)) if theme == "custom" => {
            let accent = ca.as_deref().unwrap_or("#2563eb");
            let accent_hover = cah.as_deref().unwrap_or("#1d4ed8");
            let bg = cb.as_deref().unwrap_or("#f4f4f5");
            let surface = cs.as_deref().unwrap_or("#ffffff");
            let text = ct.as_deref().unwrap_or("#18181b");
            custom_theme_css(accent, accent_hover, bg, surface, text)
        }
        Some((ref theme, ..)) => preset_theme_css(theme).to_string(),
        None => String::new(),
    }
}

/// Get the current theme name from DB.
pub(super) async fn get_theme_name(pool: &SqlitePool) -> String {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT theme FROM auth_config WHERE id = 'singleton'")
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
    row.map(|r| r.0).unwrap_or_else(|| "default".to_string())
}

/// Get custom theme colors from DB (for populating the form).
pub(super) async fn get_custom_colors(
    pool: &SqlitePool,
) -> (String, String, String, String, String) {
    let row: Option<(Option<String>, Option<String>, Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT custom_accent, custom_accent_hover, custom_bg, custom_surface, custom_text FROM auth_config WHERE id = 'singleton'")
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
    match row {
        Some((a, ah, bg, s, t)) => (
            a.unwrap_or_else(|| "#2563eb".to_string()),
            ah.unwrap_or_else(|| "#1d4ed8".to_string()),
            bg.unwrap_or_else(|| "#f4f4f5".to_string()),
            s.unwrap_or_else(|| "#ffffff".to_string()),
            t.unwrap_or_else(|| "#18181b".to_string()),
        ),
        None => (
            "#2563eb".to_string(),
            "#1d4ed8".to_string(),
            "#f4f4f5".to_string(),
            "#ffffff".to_string(),
            "#18181b".to_string(),
        ),
    }
}

// --- Logo management ---

pub(super) async fn serve_logo(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let Ok(bytes) = tokio::fs::read(state.data_dir.join("logo.png")).await else {
        return (axum::http::StatusCode::NOT_FOUND, "").into_response();
    };
    let mut response = axum::response::Response::builder()
        .status(200)
        .header("Cache-Control", "public, max-age=3600");
    response = match detect_image_ext(&bytes) {
        Some("jpg") => response.header("Content-Type", "image/jpeg"),
        Some(ext) => response.header("Content-Type", format!("image/{ext}")),
        // Uploads have refused SVG since #97, but an instance set up before
        // then can still hold one. Keep serving it, sandboxed so that opening
        // `/logo` directly cannot run a script inside it.
        None if looks_like_svg(&bytes) => response.header("Content-Type", "image/svg+xml").header(
            "Content-Security-Policy",
            "default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:; sandbox",
        ),
        None => response.header("Content-Type", "image/png"),
    };
    response
        .body(axum::body::Body::from(bytes))
        .unwrap_or_else(|_| axum::response::Response::new(axum::body::Body::empty()))
        .into_response()
}

/// Whether a stored logo is an SVG, as exported files start: possibly after a
/// byte order mark and whitespace, with an XML declaration, a doctype, a
/// comment or the root element.
fn looks_like_svg(bytes: &[u8]) -> bool {
    let start = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let start = &start[start.iter().take_while(|b| b.is_ascii_whitespace()).count()..];
    [&b"<svg"[..], b"<?xml", b"<!DOCTYPE svg", b"<!--"]
        .iter()
        .any(|prefix| start.starts_with(prefix))
}

// --- Favicon management ---

/// File the admin-uploaded favicon is stored under, next to `logo.png`. The
/// name is fixed whatever the format; `serve_favicon` sniffs the bytes.
pub(super) const FAVICON_FILE: &str = "favicon.png";

/// Largest logo the admin panel accepts, as its help text says.
pub(super) const LOGO_MAX_BYTES: usize = 2 * 1024 * 1024;

/// Largest favicon the admin panel accepts, as its help text says.
pub(super) const FAVICON_MAX_BYTES: usize = 512 * 1024;

/// Largest user or team avatar accepted, as the refusal message says.
pub(super) const AVATAR_MAX_BYTES: usize = 2 * 1024 * 1024;

/// The upload refusal reasons a settings page reads back from its
/// `upload_error` query parameter. Only these fixed values are shown, never
/// free text, so a crafted link cannot put words on the page.
pub(super) fn avatar_upload_error(query: &std::collections::HashMap<String, String>) -> &str {
    query
        .get("upload_error")
        .map(String::as_str)
        .filter(|reason| matches!(*reason, "avatar" | "avatar-save"))
        .unwrap_or_default()
}

/// The built-in favicon, served when no custom one is uploaded. It is the crab
/// `base.html` used to inline as a data URI.
const DEFAULT_FAVICON_SVG: &str = "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'><text y='.9em' font-size='90'>🦀</text></svg>";

pub(super) async fn serve_favicon(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let custom = tokio::fs::read(state.data_dir.join(FAVICON_FILE))
        .await
        .ok()
        .and_then(|bytes| {
            let content_type = match detect_image_ext(&bytes)? {
                "png" => "image/png",
                "jpg" => "image/jpeg",
                "gif" => "image/gif",
                "webp" => "image/webp",
                _ => return None,
            };
            Some((content_type, bytes))
        });
    let (content_type, body) =
        custom.unwrap_or(("image/svg+xml", DEFAULT_FAVICON_SVG.as_bytes().to_vec()));
    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", content_type)
        .header("Cache-Control", "public, max-age=3600")
        .body(axum::body::Body::from(body))
        .unwrap_or_else(|_| axum::response::Response::new(axum::body::Body::empty()))
        .into_response()
}

pub(super) async fn serve_accent_css(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let css = state.theme_css.read().await.clone();
    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "text/css; charset=utf-8")
        .header("Cache-Control", "no-cache")
        .body(axum::body::Body::from(css))
        .unwrap_or_else(|_| axum::response::Response::new(axum::body::Body::empty()))
        .into_response()
}

pub(super) async fn serve_brand_logo() -> impl IntoResponse {
    static BRAND_LOGO: &[u8] = include_bytes!("../../assets/calrs.png");
    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "image/png")
        .header("Cache-Control", "public, max-age=86400")
        .body(axum::body::Body::from(BRAND_LOGO))
        .unwrap_or_else(|_| axum::response::Response::new(axum::body::Body::empty()))
        .into_response()
}

/// Serves the embed runtime that consumers paste into their own sites. It's
/// loaded cross-origin, so we set permissive CORS — there's no auth state to
/// leak. The contents are checked into `assets/embed.js` and baked into the
/// binary via `include_str!`.
pub(super) async fn serve_embed_js() -> impl IntoResponse {
    static EMBED_JS: &str = include_str!("../../assets/embed.js");
    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/javascript; charset=utf-8")
        .header("Cache-Control", "public, max-age=3600")
        .header("Access-Control-Allow-Origin", "*")
        .body(axum::body::Body::from(EMBED_JS))
        .unwrap_or_else(|_| axum::response::Response::new(axum::body::Body::empty()))
        .into_response()
}

pub(super) async fn serve_font_inter_latin() -> impl IntoResponse {
    static FONT: &[u8] = include_bytes!("../../assets/inter-latin.woff2");
    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "font/woff2")
        .header("Cache-Control", "public, max-age=31536000, immutable")
        .body(axum::body::Body::from(FONT))
        .unwrap_or_else(|_| axum::response::Response::new(axum::body::Body::empty()))
        .into_response()
}

pub(super) async fn serve_font_inter_latin_ext() -> impl IntoResponse {
    static FONT: &[u8] = include_bytes!("../../assets/inter-latin-ext.woff2");
    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "font/woff2")
        .header("Cache-Control", "public, max-age=31536000, immutable")
        .body(axum::body::Body::from(FONT))
        .unwrap_or_else(|_| axum::response::Response::new(axum::body::Body::empty()))
        .into_response()
}

/// Vendored intl-tel-input bundle, baked into the binary and served
/// same-origin so a booking page never reaches a CDN. See
/// `assets/intl-tel-input/README.md` for the two licences that apply.
///
/// One handler with an exact-match allowlist rather than a file server: the
/// bundle is seven known files, and matching them by name means no request
/// path can address anything else. The stylesheet references the images by
/// flat relative URL, which is why they all share this one prefix.
pub(super) async fn serve_intl_tel_input_asset(Path(file): Path<String>) -> impl IntoResponse {
    let (bytes, content_type): (&'static [u8], &str) = match file.as_str() {
        "intlTelInput.min.js" => (
            include_bytes!("../../assets/intl-tel-input/intlTelInput.min.js"),
            "application/javascript; charset=utf-8",
        ),
        "intlTelInput.min.css" => (
            include_bytes!("../../assets/intl-tel-input/intlTelInput.min.css"),
            "text/css; charset=utf-8",
        ),
        "utils.js" => (
            include_bytes!("../../assets/intl-tel-input/utils.js"),
            "application/javascript; charset=utf-8",
        ),
        "flags.webp" => (
            include_bytes!("../../assets/intl-tel-input/flags.webp"),
            "image/webp",
        ),
        "flags@2x.webp" => (
            include_bytes!("../../assets/intl-tel-input/flags@2x.webp"),
            "image/webp",
        ),
        "globe.webp" => (
            include_bytes!("../../assets/intl-tel-input/globe.webp"),
            "image/webp",
        ),
        "globe@2x.webp" => (
            include_bytes!("../../assets/intl-tel-input/globe@2x.webp"),
            "image/webp",
        ),
        _ => return axum::http::StatusCode::NOT_FOUND.into_response(),
    };

    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", content_type)
        .header("Cache-Control", "public, max-age=31536000, immutable")
        .body(axum::body::Body::from(bytes))
        .unwrap_or_else(|_| axum::response::Response::new(axum::body::Body::empty()))
        .into_response()
}
