//! Shared pieces of the long dashboard lists (booking directory, admin
//! users): case-blind text search pushed into SQL, and page links.

/// Search text is cut to this many characters before it reaches SQL, so a
/// pasted essay cannot turn into thousands of bound patterns.
pub(super) const MAX_SEARCH_CHARS: usize = 200;

/// The search text without control characters (a pasted NUL or escape
/// is dropped, a tab or newline counts as a space), trimmed and cut to
/// `MAX_SEARCH_CHARS`.
pub(super) fn bounded_search(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter_map(|c| match c {
            c if c.is_whitespace() => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect();
    cleaned.trim().chars().take(MAX_SEARCH_CHARS).collect()
}

/// `c` when it maps to exactly one character.
fn single(mut mapped: impl Iterator<Item = char>) -> Option<char> {
    match (mapped.next(), mapped.next()) {
        (Some(one), None) => Some(one),
        _ => None,
    }
}

/// The characters a search character `c` stands for: itself, its lowercase
/// and its uppercase when each is one character, and the uppercase of that
/// lowercase. Built from the character as typed, so one whose lowercase is
/// two characters still matches itself: "İ" (lowercase "i̇") finds "İlker",
/// and "ẞ" finds both "ẞ" and "ß".
fn case_set(c: char) -> Vec<char> {
    let lower = single(c.to_lowercase());
    let candidates = [
        Some(c),
        lower,
        single(c.to_uppercase()),
        lower.and_then(|l| single(l.to_uppercase())),
    ];
    let mut set = Vec::new();
    for candidate in candidates.into_iter().flatten() {
        if !set.contains(&candidate) {
            set.push(candidate);
        }
    }
    set
}

/// A SQLite `GLOB` pattern matching any text that contains `needle`, case
/// blind for every alphabet (SQLite's own `LIKE` folds ASCII only, which
/// would miss "DÉMO" for "Démo"). Each character becomes the set of its
/// case forms (`case_set`): `Démo` gives `*[Dd][éÉ][mM][oO]*`. Everything
/// else is literal, `%` and `_` included, and the pattern is always bound
/// as a parameter, never spliced into SQL.
pub(super) fn ci_glob(needle: &str) -> String {
    let mut pattern = String::from("*");
    for c in needle.chars() {
        let set = case_set(c);
        if set.len() > 1 || matches!(c, '*' | '?' | '[') {
            // A set, which also makes the three GLOB metacharacters literal.
            // Cased letters are never `]`, `-` or `^`, so no escaping inside.
            pattern.push('[');
            pattern.extend(set);
            pattern.push(']');
        } else {
            pattern.push(c);
        }
    }
    pattern.push('*');
    pattern
}

/// Whether `haystack` contains `needle` under the rule `ci_glob` gives
/// SQLite, so that what a page highlights agrees with what SQL selected.
pub(super) fn ci_contains(haystack: &str, needle: &str) -> bool {
    let sets: Vec<Vec<char>> = needle.chars().map(case_set).collect();
    if sets.is_empty() {
        return true;
    }
    let hay: Vec<char> = haystack.chars().collect();
    hay.windows(sets.len())
        .any(|window| window.iter().zip(&sets).all(|(h, set)| set.contains(h)))
}

/// Page links under a list. Only built when the list runs over more than
/// one page, so a list that fits shows no controls at all.
#[derive(Debug, serde::Serialize)]
pub(super) struct Pager {
    pub page: i64,
    pub pages: i64,
    pub total: i64,
    /// 1-based positions of the first and last row shown.
    pub from: i64,
    pub to: i64,
    pub prev_href: Option<String>,
    pub next_href: Option<String>,
}

/// The page to show, clamped to the pages that exist (a stale `?page=9`
/// after a deletion lands on the last page rather than on an empty one).
pub(super) fn clamp_page(requested: i64, total: i64, page_size: i64) -> i64 {
    let pages = page_count(total, page_size);
    requested.clamp(1, pages)
}

pub(super) fn page_count(total: i64, page_size: i64) -> i64 {
    ((total + page_size - 1) / page_size).max(1)
}

/// `None` when everything fits on one page. `href(n)` builds the link to
/// page `n`, keeping the list's search and filters.
pub(super) fn pager(
    page: i64,
    total: i64,
    page_size: i64,
    href: impl Fn(i64) -> String,
) -> Option<Pager> {
    let pages = page_count(total, page_size);
    if pages <= 1 {
        return None;
    }
    let page = page.clamp(1, pages);
    let from = (page - 1) * page_size + 1;
    Some(Pager {
        page,
        pages,
        total,
        from,
        to: (from + page_size - 1).min(total),
        prev_href: (page > 1).then(|| href(page - 1)),
        next_href: (page < pages).then(|| href(page + 1)),
    })
}

/// A hand-written `?page=abc` or `?page=-3` is the first page, not a 400.
pub(super) fn lenient_page(raw: Option<&str>) -> i64 {
    raw.and_then(|s| s.trim().parse::<i64>().ok())
        .filter(|n| *n >= 1)
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ci_glob_folds_every_alphabet_and_keeps_wildcards_literal() {
        assert_eq!(ci_glob("Démo"), "*[Dd][éÉ][mM][oO]*");
        assert_eq!(ci_glob("a*b?[c]"), "*[aA][*][bB][?][[][cC]]*");
        assert_eq!(ci_glob("%_ 4"), "*%_ 4*");
        // ß has no single-character uppercase; ẞ lowercases to it.
        assert_eq!(ci_glob("ß"), "*ß*");
        assert_eq!(ci_glob("ẞ"), "*[ẞß]*");
        // İ lowercases to two characters, so it stands for itself.
        assert_eq!(ci_glob("İ"), "*İ*");
    }

    #[test]
    fn ci_contains_agrees_with_the_glob_rule() {
        assert!(ci_contains("Démo FR", "DÉMO fr"));
        assert!(ci_contains("Andy Host", "ndy h"));
        assert!(ci_contains("ANDY", "andy"));
        assert!(ci_contains("andy", "Andy"));
        assert!(ci_contains("Łukasz", "łuk"));
        assert!(!ci_contains("Demo", "démo"));
        assert!(!ci_contains("Andy", "andy host"));
        assert!(ci_contains("anything", ""));
    }

    #[test]
    fn characters_with_long_case_forms_still_match_themselves() {
        assert!(ci_contains("İlker Yılmaz", "İlker"));
        assert!(ci_contains("Visit İstanbul", "İstanbul"));
        assert!(ci_contains("Großmann", "GROẞMANN"));
        assert!(ci_contains("GROẞMANN", "GROẞMANN"));
        assert!(ci_contains("Großmann", "großmann"));
    }

    #[test]
    fn bounded_search_drops_control_characters() {
        assert_eq!(bounded_search("  a\0b\u{1b}[c  "), "ab[c");
        assert_eq!(bounded_search("a\tb\nc"), "a b c");
        assert_eq!(bounded_search("\0\0"), "");
        assert_eq!(bounded_search(&"x".repeat(500)).len(), MAX_SEARCH_CHARS);
    }

    #[test]
    fn pager_only_appears_past_one_page_and_links_its_neighbours() {
        assert!(pager(1, 0, 25, |n| n.to_string()).is_none());
        assert!(pager(1, 25, 25, |n| n.to_string()).is_none());
        let p = pager(2, 60, 25, |n| format!("?page={n}")).unwrap();
        assert_eq!((p.page, p.pages, p.from, p.to), (2, 3, 26, 50));
        assert_eq!(p.prev_href.as_deref(), Some("?page=1"));
        assert_eq!(p.next_href.as_deref(), Some("?page=3"));
        let last = pager(9, 60, 25, |n| format!("?page={n}")).unwrap();
        assert_eq!((last.page, last.from, last.to), (3, 51, 60));
        assert!(last.next_href.is_none());
        assert_eq!(clamp_page(9, 60, 25), 3);
        assert_eq!(clamp_page(0, 0, 25), 1);
    }

    #[test]
    fn lenient_page_defaults_to_the_first() {
        assert_eq!(lenient_page(Some("3")), 3);
        for raw in [None, Some(""), Some("abc"), Some("-2"), Some("0")] {
            assert_eq!(lenient_page(raw), 1, "{raw:?}");
        }
    }
}
