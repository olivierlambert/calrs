//! Shared pieces of the long dashboard lists (booking directory, admin
//! users and groups, bookings, invites, teams): case-blind text search
//! pushed into SQL, offset paging (`paged`) and page links.

use sqlx::sqlite::{Sqlite, SqliteRow};
use sqlx::{QueryBuilder, SqlitePool};

/// Rows on one page of a dashboard list, unless the list says otherwise.
pub(super) const PAGE_SIZE: i64 = 50;

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

/// The page asked for and how many rows a page holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PageParams {
    pub page: i64,
    pub size: i64,
}

impl PageParams {
    /// Page `page` of `PAGE_SIZE` rows.
    pub(super) fn new(page: i64) -> Self {
        Self::sized(page, PAGE_SIZE)
    }

    pub(super) fn sized(page: i64, size: i64) -> Self {
        Self {
            page: page.max(1),
            size: size.max(1),
        }
    }

    /// The page named by `key` in a request's query, leniently read.
    pub(super) fn from_query(query: &std::collections::HashMap<String, String>, key: &str) -> Self {
        Self::new(lenient_page(query.get(key).map(String::as_str)))
    }
}

/// One page of a list, as `paged` reads it.
#[derive(Debug)]
pub(super) struct Page<T> {
    pub rows: Vec<T>,
    /// Every row the list holds, on all pages.
    pub total: i64,
    /// The page shown, after clamping to the pages that exist.
    pub page: i64,
    /// `None` when the whole list fits on one page.
    pub pager: Option<Pager>,
}

/// Whether the last term of an `ORDER BY` is the row id, ascending, so that
/// rows tied on every earlier key still come in one fixed order and no row
/// shows up on two pages or on none.
pub(super) fn order_ends_with_id(order: &str) -> bool {
    let last = order.rsplit(',').next().unwrap_or("").trim();
    let column = last.strip_suffix(" ASC").unwrap_or(last).trim();
    column == "id" || column.rsplit_once('.').is_some_and(|(_, c)| c == "id")
}

/// One page of a list: `count` is `SELECT COUNT(...) FROM ... WHERE ...`,
/// `select` the same rows' columns with the same filter; `paged` adds
/// `ORDER BY order LIMIT size OFFSET ...` to it. The order is a fixed SQL
/// string (never request text) ending with the row id. A page past the end
/// lands on the last page. `href(n)` links page `n`.
pub(super) async fn paged<'q, T>(
    pool: &SqlitePool,
    mut count: QueryBuilder<'q, Sqlite>,
    mut select: QueryBuilder<'q, Sqlite>,
    order: &'static str,
    params: PageParams,
    href: impl Fn(i64) -> String,
) -> Result<Page<T>, sqlx::Error>
where
    T: for<'r> sqlx::FromRow<'r, SqliteRow> + Send + Unpin,
{
    debug_assert!(
        order_ends_with_id(order),
        "ORDER BY must end with the id: {order}"
    );
    let total: i64 = count.build_query_scalar().fetch_one(pool).await?;
    let page = clamp_page(params.page, total, params.size);
    select.push(" ORDER BY ");
    select.push(order);
    select.push(" LIMIT ");
    select.push_bind(params.size);
    select.push(" OFFSET ");
    select.push_bind((page - 1) * params.size);
    let rows: Vec<T> = select.build_query_as().fetch_all(pool).await?;
    Ok(Page {
        rows,
        total,
        page,
        pager: pager(page, total, params.size, href),
    })
}

/// Push the fixed SQL `sql` onto `qb`, binding `value` at each `?` it holds,
/// so a scope written for `sqlx::query` (`NOTE_HOSTS`, the bookings page's
/// scopes) can also build a paged query. `sql` is a constant with no `?`
/// inside a string literal.
pub(super) fn push_with_binds(qb: &mut QueryBuilder<'_, Sqlite>, sql: &'static str, value: &str) {
    let mut parts = sql.split('?');
    if let Some(first) = parts.next() {
        qb.push(first);
    }
    for part in parts {
        qb.push_bind(value.to_string());
        qb.push(part);
    }
}

/// `path?a=1&b=2#anchor` from query parts already encoded (`key=value`),
/// empty parts skipped, so a list in its default state links the bare path.
pub(super) fn href(path: &str, parts: &[String], anchor: &str) -> String {
    let query = parts
        .iter()
        .filter(|p| !p.is_empty())
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join("&");
    let anchor = if anchor.is_empty() {
        String::new()
    } else {
        format!("#{anchor}")
    };
    if query.is_empty() {
        format!("{path}{anchor}")
    } else {
        format!("{path}?{query}{anchor}")
    }
}

/// `raw` when it is a path of this site under `prefix` (`/dashboard`,
/// `/dashboard/admin`), so a form can say which page of a list to return
/// to without opening a redirect to anywhere else: no scheme, no `//` or
/// backslash host, no whitespace or control character, and no `.` or `..`
/// segment (also percent-encoded), which the browser would resolve out of
/// `prefix`. `None` otherwise.
pub(super) fn return_path(raw: Option<&str>, prefix: &str) -> Option<String> {
    let raw = raw?;
    let rest = raw.strip_prefix(prefix)?;
    let boundary = matches!(rest.chars().next(), None | Some('?' | '#' | '/'));
    let path = raw.split(['?', '#']).next().unwrap_or("");
    let dot_segment = path.split('/').any(|segment| {
        let segment = segment.to_ascii_lowercase().replace("%2e", ".");
        segment == "." || segment == ".."
    });
    let clean = !raw.contains('\\')
        && !raw.starts_with("//")
        && !dot_segment
        && raw.len() <= 2048
        && raw.chars().all(|c| !c.is_whitespace() && !c.is_control());
    (boundary && clean).then(|| raw.to_string())
}

/// `path` with `key=value` added to its query, before its anchor.
pub(super) fn with_param(path: &str, key: &str, value: &str) -> String {
    let (head, anchor) = match path.split_once('#') {
        Some((head, anchor)) => (head, format!("#{anchor}")),
        None => (path, String::new()),
    };
    let sep = if head.contains('?') { '&' } else { '?' };
    format!("{head}{sep}{key}={}{anchor}", urlencoding::encode(value))
}

/// `key=n` for a page past the first, nothing for the first.
pub(super) fn page_part(key: &str, page: i64) -> String {
    if page > 1 {
        format!("{key}={page}")
    } else {
        String::new()
    }
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
    fn the_order_must_end_with_the_id() {
        assert!(order_ends_with_id("created_at, id"));
        assert!(order_ends_with_id("b.start_at DESC, b.id"));
        assert!(order_ends_with_id("t.name, t.id ASC"));
        assert!(!order_ends_with_id("b.start_at"));
        assert!(!order_ends_with_id("b.id, b.start_at"));
        assert!(!order_ends_with_id("b.id DESC"));
        assert!(!order_ends_with_id("guest_id"));
    }

    #[test]
    fn push_with_binds_binds_every_placeholder() {
        let mut qb = QueryBuilder::<Sqlite>::new("SELECT 1 WHERE ");
        push_with_binds(&mut qb, "(a = ? OR b = ?)", "x");
        assert_eq!(qb.sql(), "SELECT 1 WHERE (a = ? OR b = ?)");
        let mut qb = QueryBuilder::<Sqlite>::new("");
        push_with_binds(&mut qb, "a = 1", "x");
        assert_eq!(qb.sql(), "a = 1");
    }

    #[tokio::test]
    async fn push_with_binds_reaches_sqlite_in_order() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let mut qb = QueryBuilder::<Sqlite>::new("SELECT ");
        push_with_binds(&mut qb, "? || '-' || ?", "v");
        qb.push(" || ");
        qb.push_bind("w");
        let joined: String = qb.build_query_scalar().fetch_one(&pool).await.unwrap();
        assert_eq!(joined, "v-vw");
    }

    #[test]
    fn return_paths_stay_on_this_site_under_their_prefix() {
        let ok = |raw: &str| return_path(Some(raw), "/dashboard");
        assert_eq!(ok("/dashboard").as_deref(), Some("/dashboard"));
        assert_eq!(
            ok("/dashboard/bookings?bp=3#bk-upcoming").as_deref(),
            Some("/dashboard/bookings?bp=3#bk-upcoming")
        );
        for bad in [
            "https://evil.example/dashboard",
            "//evil.example/dashboard",
            "/dashboardx",
            "/dashboard/../u/x",
            "/dashboard/..//evil.example",
            "/dashboard/%2e%2e/u/x",
            "/dashboard/.%2E/u/x",
            "/dashboard/./bookings",
            "/dashboard/bookings/..",
            "/dashboard/..?x=1",
            "/\\evil.example",
            "/dashboard\\..",
            "/dashboard/bookings?q=a b",
            "/dashboard/\n",
            "javascript:alert(1)",
            "",
        ] {
            assert_eq!(ok(bad), None, "{bad:?}");
        }
        assert_eq!(return_path(None, "/dashboard"), None);
        assert_eq!(
            return_path(Some("/dashboard/bookings"), "/dashboard/admin"),
            None
        );
    }

    #[test]
    fn with_param_goes_before_the_anchor() {
        assert_eq!(with_param("/d#a", "error", "x y"), "/d?error=x%20y#a");
        assert_eq!(with_param("/d?p=2#a", "error", "e"), "/d?p=2&error=e#a");
        assert_eq!(with_param("/d", "e", "1"), "/d?e=1");
    }

    #[test]
    fn hrefs_leave_out_defaults() {
        assert_eq!(href("/x", &[], ""), "/x");
        assert_eq!(href("/x", &[page_part("p", 1)], "a"), "/x#a");
        assert_eq!(
            href("/x", &[page_part("p", 2), String::new(), "q=a".into()], "a"),
            "/x?p=2&q=a#a"
        );
    }

    #[test]
    fn page_params_never_go_below_one() {
        assert_eq!(PageParams::sized(0, 0), PageParams { page: 1, size: 1 });
        assert_eq!(PageParams::new(3).size, PAGE_SIZE);
        let mut query = std::collections::HashMap::new();
        query.insert("xp".to_string(), "abc".to_string());
        assert_eq!(PageParams::from_query(&query, "xp").page, 1);
        query.insert("xp".to_string(), "4".to_string());
        assert_eq!(PageParams::from_query(&query, "xp").page, 4);
        assert_eq!(PageParams::from_query(&query, "other").page, 1);
    }

    /// Ties on the sort key, the last page and a page past the end, on a
    /// real query.
    #[tokio::test]
    async fn paged_reads_each_row_once_across_pages() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query("CREATE TABLE t (id TEXT PRIMARY KEY, k INTEGER, tag TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        // 23 rows, all tied on k, inserted out of id order.
        for i in (0..23).rev() {
            sqlx::query("INSERT INTO t VALUES (?, 1, ?)")
                .bind(format!("r{i:02}"))
                .bind(if i % 2 == 0 { "even" } else { "odd" })
                .execute(&pool)
                .await
                .unwrap();
        }
        let read = |page: i64, tag: Option<&'static str>| {
            let pool = pool.clone();
            async move {
                let mut count = QueryBuilder::new("SELECT COUNT(*) FROM t WHERE 1");
                let mut select = QueryBuilder::new("SELECT id FROM t WHERE 1");
                if let Some(tag) = tag {
                    for qb in [&mut count, &mut select] {
                        qb.push(" AND tag = ");
                        qb.push_bind(tag);
                    }
                }
                paged::<(String,)>(
                    &pool,
                    count,
                    select,
                    "k, id",
                    PageParams::sized(page, 10),
                    |n| format!("?p={n}"),
                )
                .await
                .unwrap()
            }
        };
        let mut seen = Vec::new();
        for page in 1..=3 {
            let p = read(page, None).await;
            assert_eq!(p.total, 23);
            assert_eq!(p.page, page);
            seen.extend(p.rows.into_iter().map(|(id,)| id));
        }
        let expected: Vec<String> = (0..23).map(|i| format!("r{i:02}")).collect();
        assert_eq!(seen, expected, "every row once, in id order");
        let last = read(3, None).await;
        assert_eq!(last.rows.len(), 3);
        let pager = last.pager.unwrap();
        assert!(pager.next_href.is_none());
        assert_eq!(pager.prev_href.as_deref(), Some("?p=2"));
        let past = read(99, None).await;
        assert_eq!(past.page, 3, "past the end lands on the last page");
        assert_eq!(past.rows.len(), 3);
        let filtered = read(2, Some("even")).await;
        assert_eq!(filtered.total, 12);
        assert_eq!(filtered.rows.len(), 2);
        assert_eq!(filtered.rows[0].0, "r20");
        let one = read(1, Some("nothing")).await;
        assert_eq!((one.total, one.page, one.rows.len()), (0, 1, 0));
        assert!(one.pager.is_none());
    }

    #[test]
    fn lenient_page_defaults_to_the_first() {
        assert_eq!(lenient_page(Some("3")), 3);
        for raw in [None, Some(""), Some("abc"), Some("-2"), Some("0")] {
            assert_eq!(lenient_page(raw), 1, "{raw:?}");
        }
    }
}
