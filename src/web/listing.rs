//! Shared pieces of the long dashboard lists (booking directory, admin
//! users): case-blind text search pushed into SQL, and page links.

/// Search text is cut to this many characters before it reaches SQL, so a
/// pasted essay cannot turn into thousands of bound patterns.
pub(super) const MAX_SEARCH_CHARS: usize = 200;

/// The trimmed search text, cut to `MAX_SEARCH_CHARS`.
pub(super) fn bounded_search(raw: &str) -> String {
    raw.trim().chars().take(MAX_SEARCH_CHARS).collect()
}

/// The uppercase form of `c` when it is a single character, the case
/// variant `ci_glob` and `ci_contains` accept besides `c` itself.
fn upper_variant(c: char) -> Option<char> {
    let mut upper = c.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(u), None) if u != c => Some(u),
        _ => None,
    }
}

/// A SQLite `GLOB` pattern matching any text that contains `needle`, case
/// blind for every alphabet (SQLite's own `LIKE` folds ASCII only, which
/// would miss "DÉMO" for "Démo"). The needle is lowercased, then each
/// character becomes a set of itself and its uppercase form: `démo` gives
/// `*[dD][éÉ][mM][oO]*`. Everything else is literal, `%` and `_` included,
/// and the pattern is always bound as a parameter, never spliced into SQL.
pub(super) fn ci_glob(needle: &str) -> String {
    let mut pattern = String::from("*");
    for c in needle.to_lowercase().chars() {
        match upper_variant(c) {
            Some(u) => {
                pattern.push('[');
                pattern.push(c);
                pattern.push(u);
                pattern.push(']');
            }
            // The three GLOB metacharacters, as one-character sets.
            None if matches!(c, '*' | '?' | '[') => {
                pattern.push('[');
                pattern.push(c);
                pattern.push(']');
            }
            None => pattern.push(c),
        }
    }
    pattern.push('*');
    pattern
}

/// Whether `haystack` contains `needle` under the rule `ci_glob` gives
/// SQLite, so that what a page highlights agrees with what SQL selected.
pub(super) fn ci_contains(haystack: &str, needle: &str) -> bool {
    let needle: Vec<char> = needle.to_lowercase().chars().collect();
    if needle.is_empty() {
        return true;
    }
    let hay: Vec<char> = haystack.chars().collect();
    hay.windows(needle.len()).any(|window| {
        window
            .iter()
            .zip(&needle)
            .all(|(h, n)| h == n || upper_variant(*n) == Some(*h))
    })
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
        assert_eq!(ci_glob("Démo"), "*[dD][éÉ][mM][oO]*");
        assert_eq!(ci_glob("a*b?[c]"), "*[aA][*][bB][?][[][cC]]*");
        assert_eq!(ci_glob("%_ 4"), "*%_ 4*");
        // ß has no single-character uppercase; it stays itself.
        assert_eq!(ci_glob("ß"), "*ß*");
    }

    #[test]
    fn ci_contains_agrees_with_the_glob_rule() {
        assert!(ci_contains("Démo FR", "DÉMO fr"));
        assert!(ci_contains("Andy Host", "ndy h"));
        assert!(ci_contains("Łukasz", "łuk"));
        assert!(!ci_contains("Demo", "démo"));
        assert!(!ci_contains("Andy", "andy host"));
        assert!(ci_contains("anything", ""));
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
