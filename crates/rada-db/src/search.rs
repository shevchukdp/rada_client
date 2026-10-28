//! Title search over the local index.
//!
//! Strategy:
//! - multi-word query  → FTS5 MATCH (case-insensitive, ranked); AND of all
//!   tokens with a prefix match on the last token, falling back to OR when
//!   the AND expression finds nothing;
//! - single word       → `LIKE` substring (FTS can't do substrings);
//! - plus optional filters (org / type / status / date range / number).
//!
//! Numbered placeholders are used throughout so argument order is explicit.

use anyhow::{Context, Result};
use rusqlite::Connection;
use serde::Serialize;

/// Result ordering for [`SearchQuery`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    /// FTS rank when available, otherwise newest-first.
    Relevance,
    /// Newest adoption date first — the default.
    #[default]
    DateDesc,
    /// Oldest adoption date first.
    DateAsc,
    /// Title A→Я.
    NazvaAsc,
    /// Title Я→A.
    NazvaDesc,
    /// Newest edition date first.
    PoddDesc,
}

impl Sort {
    /// Case-insensitive parse; unknown values fall back to the default.
    pub fn parse(s: &str) -> Sort {
        match s.trim().to_ascii_lowercase().as_str() {
            "relevance" => Sort::Relevance,
            "date" | "date_desc" => Sort::DateDesc,
            "date_asc" => Sort::DateAsc,
            "nazva" | "nazva_asc" => Sort::NazvaAsc,
            "nazva_desc" => Sort::NazvaDesc,
            "podd" | "podd_desc" => Sort::PoddDesc,
            _ => Sort::default(),
        }
    }

    /// Stable identifier for URLs and persisted state.
    pub fn as_str(self) -> &'static str {
        match self {
            Sort::Relevance => "relevance",
            Sort::DateDesc => "date_desc",
            Sort::DateAsc => "date_asc",
            Sort::NazvaAsc => "nazva_asc",
            Sort::NazvaDesc => "nazva_desc",
            Sort::PoddDesc => "podd_desc",
        }
    }

    /// `ORDER BY` body (without the `ORDER BY` keyword). `prefix` qualifies
    /// columns on the joined FTS query (e.g. `"d."`).
    fn order_body(self, prefix: &str) -> String {
        let (col, dir) = match self {
            Sort::Relevance | Sort::DateDesc => ("org_date", "DESC"),
            Sort::DateAsc => ("org_date", "ASC"),
            Sort::NazvaAsc => ("nazva", "ASC"),
            Sort::NazvaDesc => ("nazva", "DESC"),
            Sort::PoddDesc => ("poddat", "DESC"),
        };
        match self {
            Sort::Relevance => "f.rank".to_string(),
            Sort::NazvaAsc | Sort::NazvaDesc => {
                format!("{prefix}{col} COLLATE NOCASE {dir}, {prefix}nreg")
            }
            _ => format!("{prefix}{col} {dir}, {prefix}nreg"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SearchQuery {
    /// Title term.
    pub nazva: Option<String>,
    pub org_id: Option<i64>,
    pub typ: Option<i64>,
    pub status: Option<i64>,
    /// Inclusive `YYYYMMDD` bounds on the adoption date.
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    /// Document-number substring (e.g. `2041`).
    pub doc_number: Option<String>,
    /// Result ordering (default: newest first).
    pub sort: Sort,
    pub limit: usize,
    pub offset: usize,
}

/// Manual `Default` — `limit` must be a sane page size, not the derived 0.
impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            nazva: None,
            org_id: None,
            typ: None,
            status: None,
            date_from: None,
            date_to: None,
            doc_number: None,
            sort: Sort::default(),
            limit: 50,
            offset: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub nreg: String,
    pub nazva: String,
    pub status: i64,
    pub org_date: String,
    pub org_num: String,
    pub poddat: String,
}

/// Per-option result counts for the dictionary-driven filters («Статус»,
/// «Тип», «Видавник» on the web search page). Each list is `(dict_id, count)`
/// sorted by count descending, then id ascending; the unset/unknown value 0
/// is kept, so those documents still surface.
#[derive(Debug, Clone, Serialize)]
pub struct FacetCounts {
    pub status: Vec<(i64, usize)>,
    pub typ: Vec<(i64, usize)>,
    pub org: Vec<(i64, usize)>,
}

/// Exact-`nreg` lookup for offline "search by number" (`254к/96-вр`,
/// `1178-2022-п`, `n0423500-26`…). Returns the hit without any FTS/LIKE.
pub fn by_nreg(conn: &Connection, nreg: &str) -> Result<Option<SearchHit>> {
    let n = nreg.trim();
    if n.is_empty() {
        return Ok(None);
    }
    let res = conn.query_row(
        "SELECT nreg, nazva, status, org_date, org_num, poddat FROM docs WHERE nreg = ?1",
        [n],
        |r| {
            Ok(SearchHit {
                nreg: r.get(0)?,
                nazva: r.get(1)?,
                status: r.get(2)?,
                org_date: r.get(3)?,
                org_num: r.get(4)?,
                poddat: r.get(5)?,
            })
        },
    );
    match res {
        Ok(h) => Ok(Some(h)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e).context("by_nreg"),
    }
}

fn is_multi_word(term: &str) -> bool {
    let t = term.trim();
    !t.is_empty() && t.split_whitespace().count() >= 2
}

/// Join the tokens of `term` into an FTS5 expression separated by `sep`:
/// every whitespace-separated token is quoted (an inner `"` is doubled),
/// empty tokens are dropped, and the **last** token becomes a prefix match
/// (`"tok"*`) — FTS5 only honours `*` right after the closing quote, never
/// inside the quoted string («кодек» thus matches кодек/кодексу).
fn fts_join(term: &str, sep: &str) -> String {
    let toks: Vec<&str> = term
        .split_whitespace()
        .filter(|w| !w.is_empty())
        .collect();
    let last = toks.len().saturating_sub(1);
    toks.iter()
        .enumerate()
        .map(|(i, w)| {
            let q = format!("\"{}\"", w.replace('"', "\"\""));
            if i == last {
                format!("{q}*")
            } else {
                q
            }
        })
        .collect::<Vec<_>>()
        .join(sep)
}

/// Strict FTS5 expression: all tokens required, last token as prefix —
/// `"tok1" AND "tok2" AND "tokN"*`.
fn fts_expr_and(term: &str) -> String {
    fts_join(term, " AND ")
}

/// Fallback FTS5 expression: any token may match, last token as prefix —
/// `"tok1" OR "tok2" OR "tokN"*`.
fn fts_expr_or(term: &str) -> String {
    fts_join(term, " OR ")
}

fn add_param(
    where_sql: &mut String,
    args: &mut Vec<Box<dyn rusqlite::types::ToSql>>,
    cond: &str,
    val: impl rusqlite::types::ToSql + 'static,
) {
    args.push(Box::new(val));
    where_sql.push_str(cond);
    where_sql.push_str(&format!("?{}", args.len()));
}

/// Column whose own filter is excluded from a facet count (standard facet
/// semantics: counting the options of one column respects every *other*
/// filter but ignores that column's own selection).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FacetColumn {
    Status,
    Typ,
    Org,
}

/// Build the shared `WHERE` fragment (and its numbered arguments) used by
/// both [`search_with`] and [`facets`]. `use_fts` suppresses the `LIKE`
/// title clause (the FTS branch matches via `MATCH ?1`, which must stay
/// `?1`, so callers push it into `args` first). `skip` omits one column's
/// own filter for facet counts.
fn build_where(
    q: &SearchQuery,
    title: Option<&str>,
    use_fts: bool,
    skip: Option<FacetColumn>,
    args: &mut Vec<Box<dyn rusqlite::types::ToSql>>,
) -> String {
    let mut where_sql = String::new();
    if let Some(t) = title {
        if !use_fts {
            add_param(&mut where_sql, args, " AND nazva LIKE ", format!("%{t}%"));
        }
    }
    if let Some(org) = q.org_id {
        if skip != Some(FacetColumn::Org) {
            add_param(&mut where_sql, args, " AND org_id = ", org);
        }
    }
    if let Some(typ) = q.typ {
        if skip != Some(FacetColumn::Typ) {
            // `types` is stored as "2|168|11"; pad with '|' on both sides for
            // an exact member match.
            add_param(
                &mut where_sql,
                args,
                " AND ('||' || types || '|') LIKE ",
                format!("%|{typ}|%"),
            );
        }
    }
    if let Some(st) = q.status {
        if skip != Some(FacetColumn::Status) {
            add_param(&mut where_sql, args, " AND status = ", st);
        }
    }
    if let Some(d) = &q.date_from {
        add_param(&mut where_sql, args, " AND org_date >= ", d.clone());
    }
    if let Some(d) = &q.date_to {
        add_param(&mut where_sql, args, " AND org_date <= ", d.clone());
    }
    if let Some(n) = &q.doc_number {
        add_param(
            &mut where_sql,
            args,
            " AND org_num LIKE ",
            format!("%{n}%"),
        );
    }
    where_sql
}

/// Execute a search; returns `(hits, total)`.
///
/// Multi-word titles take the FTS5 path: first the strict AND expression
/// (all tokens required, last token as prefix); when that yields nothing,
/// the same query is retried with the OR expression so the user still gets
/// ranked partial matches. The 0–1-word `LIKE` path is untouched.
pub fn search(conn: &Connection, q: &SearchQuery) -> Result<(Vec<SearchHit>, i64)> {
    let title = q
        .nazva
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    if title.as_deref().map(is_multi_word).unwrap_or(false) {
        let t = title.as_deref().unwrap_or("");
        let (hits, total) = search_with(conn, q, title.as_deref(), Some(fts_expr_and(t)))?;
        if total == 0 {
            return search_with(conn, q, title.as_deref(), Some(fts_expr_or(t)));
        }
        return Ok((hits, total));
    }
    search_with(conn, q, title.as_deref(), None)
}

/// Shared count/select flow. `fts` carries the MATCH expression for the
/// FTS5 branch (its expression becomes `?1`); `None` selects the plain
/// `docs` table with the `LIKE` title filter.
fn search_with(
    conn: &Connection,
    q: &SearchQuery,
    title: Option<&str>,
    fts: Option<String>,
) -> Result<(Vec<SearchHit>, i64)> {
    let use_fts = fts.is_some();

    let mut args: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    // In the FTS path, ?1 is the MATCH expression.
    if let Some(expr) = fts {
        args.push(Box::new(expr));
    }

    let where_sql = build_where(q, title, use_fts, None, &mut args);

    let (base, order) = if use_fts {
        (
            "SELECT d.nreg, d.nazva, d.status, d.org_date, d.org_num, d.poddat
             FROM docs_fts f JOIN docs d ON d.rowid = f.rowid
             WHERE docs_fts MATCH ?1",
            format!(" ORDER BY {}", q.sort.order_body("d.")),
        )
    } else {
        // No FTS rank available: `Relevance` degrades to newest-first.
        let sort = if q.sort == Sort::Relevance { Sort::DateDesc } else { q.sort };
        (
            "SELECT nreg, nazva, status, org_date, org_num, poddat FROM docs
             WHERE 1=1",
            format!(" ORDER BY {}", sort.order_body("")),
        )
    };

    let select_sql = format!("{base}{where_sql}{order} LIMIT ?A OFFSET ?B");
    // replace placeholder markers with real numbers (limit, offset)
    let n1 = args.len() + 1;
    let n2 = args.len() + 2;
    let select_sql = select_sql.replace("?A", &format!("?{n1}")).replace("?B", &format!("?{n2}"));

    let count_sql = if use_fts {
        format!(
            "SELECT COUNT(*) FROM docs_fts f JOIN docs d ON d.rowid = f.rowid
             WHERE docs_fts MATCH ?1{where_sql}"
        )
    } else {
        format!("SELECT COUNT(*) FROM docs WHERE 1=1{where_sql}")
    };

    // Count uses all args except the trailing limit/offset.
    let n_filters = args.len();
    let count_refs: Vec<&dyn rusqlite::types::ToSql> =
        args[..n_filters].iter().map(|b| b.as_ref()).collect();
    let total: i64 = conn
        .query_row(&count_sql, count_refs.as_slice(), |r| r.get(0))
        .context("count query")?;

    args.push(Box::new(q.limit as i64));
    args.push(Box::new(q.offset as i64));
    let all_refs: Vec<&dyn rusqlite::types::ToSql> =
        args.iter().map(|b| b.as_ref()).collect();

    let mut stmt = conn.prepare(&select_sql).context("prepare search")?;
    let mut rows = stmt
        .query_map(all_refs.as_slice(), |r| {
            Ok(SearchHit {
                nreg: r.get(0)?,
                nazva: r.get(1)?,
                status: r.get(2)?,
                org_date: r.get(3)?,
                org_num: r.get(4)?,
                poddat: r.get(5)?,
            })
        })
        .context("run search")?;
    let mut hits = Vec::with_capacity(q.limit);
    while let Some(row) = rows.next() {
        hits.push(row.context("next search row")?);
    }
    Ok((hits, total))
}

/// Per-option counts for the current search (dictionary dropdown facets).
/// Standard facet semantics: the counts of one column respect every filter
/// **except that column's own** (the status facet ignores `q.status`, the
/// typ facet ignores `q.typ`, the org facet ignores `q.org_id`), so the UI
/// can show what each *other* option would yield. The title term goes
/// through the same FTS5/LIKE dual path as [`search`] (AND expression, OR
/// fallback when it matches nothing).
pub fn facets(conn: &Connection, q: &SearchQuery) -> Result<FacetCounts> {
    let title = q
        .nazva
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    // Same effective expression choice as `search()`: strict AND when it
    // finds anything, the OR fallback otherwise; `None` = LIKE path.
    let fts: Option<String> = if title.as_deref().map(is_multi_word).unwrap_or(false) {
        let t = title.as_deref().unwrap_or("");
        let and_expr = fts_expr_and(t);
        if count_fts(conn, q, title.as_deref(), &and_expr)? == 0 {
            Some(fts_expr_or(t))
        } else {
            Some(and_expr)
        }
    } else {
        None
    };

    Ok(FacetCounts {
        status: group_facet(conn, q, title.as_deref(), fts.as_deref(), "status", FacetColumn::Status)?,
        typ: typ_facet(conn, q, title.as_deref(), fts.as_deref())?,
        org: group_facet(conn, q, title.as_deref(), fts.as_deref(), "org_id", FacetColumn::Org)?,
    })
}

/// Row count on the FTS5 path with the full filter set (drives the AND→OR
/// fallback decision shared with [`search`]).
fn count_fts(conn: &Connection, q: &SearchQuery, title: Option<&str>, expr: &str) -> Result<i64> {
    let mut args: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(expr.to_string())];
    let where_sql = build_where(q, title, true, None, &mut args);
    let sql = format!(
        "SELECT COUNT(*) FROM docs_fts f JOIN docs d ON d.rowid = f.rowid
         WHERE docs_fts MATCH ?1{where_sql}"
    );
    let refs: Vec<&dyn rusqlite::types::ToSql> = args.iter().map(|b| b.as_ref()).collect();
    conn.query_row(&sql, refs.as_slice(), |r| r.get(0))
        .context("facet count")
}

/// `GROUP BY` facet over a single-valued column (`status` / `org_id`).
/// NULL/0 buckets are kept (id 0 = unknown value).
fn group_facet(
    conn: &Connection,
    q: &SearchQuery,
    title: Option<&str>,
    fts: Option<&str>,
    col: &str,
    skip: FacetColumn,
) -> Result<Vec<(i64, usize)>> {
    let mut args: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    if let Some(e) = fts {
        args.push(Box::new(e.to_string()));
    }
    let where_sql = build_where(q, title, fts.is_some(), Some(skip), &mut args);
    let sql = if fts.is_some() {
        format!(
            "SELECT d.{col}, COUNT(*) FROM docs_fts f JOIN docs d ON d.rowid = f.rowid
             WHERE docs_fts MATCH ?1{where_sql}
             GROUP BY d.{col} ORDER BY COUNT(*) DESC, d.{col}"
        )
    } else {
        format!(
            "SELECT {col}, COUNT(*) FROM docs WHERE 1=1{where_sql}
             GROUP BY {col} ORDER BY COUNT(*) DESC, {col}"
        )
    };
    let refs: Vec<&dyn rusqlite::types::ToSql> = args.iter().map(|b| b.as_ref()).collect();
    let mut stmt = conn.prepare(&sql).context("prepare facet")?;
    let mut rows = stmt
        .query_map(refs.as_slice(), |r| Ok((r.get(0)?, r.get(1)?)))
        .context("run facet")?;
    let mut out = Vec::new();
    while let Some(row) = rows.next() {
        out.push(row.context("next facet row")?);
    }
    Ok(out)
}

/// Facet over the multi-valued `types` column ("2|168|11"): rows are read
/// raw and each document counts once per member id — exactly the membership
/// the typ filter matches (`'|' || types || '|' LIKE '%|<id>|%'`), since a
/// token counts iff it parses as the same i64 the padded LIKE would hit.
fn typ_facet(
    conn: &Connection,
    q: &SearchQuery,
    title: Option<&str>,
    fts: Option<&str>,
) -> Result<Vec<(i64, usize)>> {
    let mut args: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    if let Some(e) = fts {
        args.push(Box::new(e.to_string()));
    }
    let where_sql = build_where(q, title, fts.is_some(), Some(FacetColumn::Typ), &mut args);
    let sql = if fts.is_some() {
        format!(
            "SELECT d.types FROM docs_fts f JOIN docs d ON d.rowid = f.rowid
             WHERE docs_fts MATCH ?1{where_sql}"
        )
    } else {
        format!("SELECT types FROM docs WHERE 1=1{where_sql}")
    };
    let refs: Vec<&dyn rusqlite::types::ToSql> = args.iter().map(|b| b.as_ref()).collect();
    let mut stmt = conn.prepare(&sql).context("prepare typ facet")?;
    let mut rows = stmt
        .query_map(refs.as_slice(), |r| r.get::<_, String>(0))
        .context("run typ facet")?;
    let mut counts: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
    while let Some(row) = rows.next() {
        for tok in row.context("next typ facet row")?.split('|') {
            if let Ok(id) = tok.parse::<i64>() {
                *counts.entry(id).or_insert(0) += 1;
            }
        }
    }
    let mut out: Vec<(i64, usize)> = counts.into_iter().collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    Ok(out)
}

/// Heuristic: could this token be an nreg (system number) rather than a
/// title? Single word, 4..=20 chars, has a digit, only nreg-safe chars,
/// and contains a separator (`-`, `_` or `/`).
pub fn looks_like_nreg(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() || t.split_whitespace().count() != 1 {
        return false;
    }
    let bytes_ok = t
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '/'));
    if !bytes_ok || !t.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    let len = t.chars().count();
    (4..=20).contains(&len) && t.contains(['-', '_', '/'])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::{parse_dump, upsert_docs};
    use crate::schema::migrate;

    fn seeded() -> Connection {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let uk = [
            "1\t2041-15\tПро проведення парламентських слухань\t5\t2\t1:20040923:2041-IV\t\t0\t20041203",
            "2\t4505-12\tПро оцінку ринкової вартості\t5\t1\t2:20120810:4505-VI\t\t0\t20120810",
            "3\t2341-14\tКримінальний кодекс України\t1\t1\t1:19940401:2341-XII\t\t0\t20010408",
        ]
        .join("\n");
        // parse_dump works on already-decoded UTF-8 (as import_dump does after
        // the cp1251 decode), so pass the string directly.
        let rows = parse_dump(&uk).unwrap();
        upsert_docs(&mut conn, &rows).unwrap();
        conn
    }

    #[test]
    fn fts_finds_multi_word() {
        let conn = seeded();
        let (hits, total) = search(
            &conn,
            &SearchQuery {
                nazva: Some("парламентських слухань".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(total, 1);
        assert_eq!(hits[0].nreg, "2041-15");
    }

    #[test]
    fn like_finds_single_word_substring() {
        let conn = seeded();
        let (hits, total) = search(
            &conn,
            &SearchQuery {
                nazva: Some("код".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(total, 1);
        assert_eq!(hits[0].nreg, "2341-14");
    }

    #[test]
    fn no_term_lists_newest_first() {
        let conn = seeded();
        let (hits, total) = search(&conn, &SearchQuery::default()).unwrap();
        assert_eq!(total, 3);
        assert_eq!(hits[0].nreg, "4505-12"); // newest adoption date
    }

    #[test]
    fn status_filter() {
        let conn = seeded();
        let (hits, total) = search(
            &conn,
            &SearchQuery {
                status: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(total, 1);
        assert_eq!(hits[0].nreg, "2341-14");
    }

    #[test]
    fn type_filter() {
        let conn = seeded();
        let (_hits, total) = search(
            &conn,
            &SearchQuery {
                typ: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(total, 2);
    }

    #[test]
    fn date_range_filter() {
        let conn = seeded();
        let (hits, total) = search(
            &conn,
            &SearchQuery {
                date_from: Some("20050101".into()),
                date_to: Some("20150101".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(total, 1);
        assert_eq!(hits[0].nreg, "4505-12");
    }

    #[test]
    fn combined_title_and_status() {
        let conn = seeded();
        let (_hits, total) = search(
            &conn,
            &SearchQuery {
                nazva: Some("кодекс".into()),
                status: Some(5),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(total, 0); // the only кодекс is status 1
    }

    #[test]
    fn sort_date_asc_flips_default() {
        let conn = seeded();
        let (hits, _) = search(
            &conn,
            &SearchQuery {
                sort: Sort::DateAsc,
                ..Default::default()
            },
        )
        .unwrap();
        // org_dates: 2341-14=19940401, 2041-15=20040923, 4505-12=20120810
        assert_eq!(hits[0].nreg, "2341-14");
        assert_eq!(hits[2].nreg, "4505-12");
    }

    #[test]
    fn sort_nazva_ignores_case() {
        let conn = seeded();
        let (hits, _) = search(
            &conn,
            &SearchQuery {
                sort: Sort::NazvaAsc,
                ..Default::default()
            },
        )
        .unwrap();
        // "Кримінальний…" (К) before "Про…" (П) in Cyrillic order.
        assert_eq!(hits[0].nreg, "2341-14");
        let (desc, _) = search(
            &conn,
            &SearchQuery {
                sort: Sort::NazvaDesc,
                ..Default::default()
            },
        )
        .unwrap();
        assert_ne!(desc[0].nreg, hits[0].nreg);
    }

    #[test]
    fn sort_podd_desc() {
        let conn = seeded();
        let (hits, _) = search(
            &conn,
            &SearchQuery {
                sort: Sort::PoddDesc,
                ..Default::default()
            },
        )
        .unwrap();
        // poddat: 4505-12=20120810 newest.
        assert_eq!(hits[0].nreg, "4505-12");
    }

    #[test]
    fn relevance_in_fts_matches_rank() {
        let conn = seeded();
        let (hits, _) = search(
            &conn,
            &SearchQuery {
                nazva: Some("кримінальний кодекс".into()),
                sort: Sort::Relevance,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(hits[0].nreg, "2341-14");
    }

    #[test]
    fn sort_parse_round_trip() {
        for s in [
            Sort::Relevance,
            Sort::DateDesc,
            Sort::DateAsc,
            Sort::NazvaAsc,
            Sort::NazvaDesc,
            Sort::PoddDesc,
        ] {
            assert_eq!(Sort::parse(s.as_str()), s);
        }
        assert_eq!(Sort::parse("garbage"), Sort::DateDesc);
    }

    #[test]
    fn by_nreg_exact_hit_and_miss() {
        let conn = seeded();
        let h = by_nreg(&conn, "2341-14").unwrap().expect("hit");
        assert_eq!(h.nazva, "Кримінальний кодекс України");
        assert!(by_nreg(&conn, "2341-14 ").unwrap().is_some(), "trims");
        assert!(by_nreg(&conn, "9999-99").unwrap().is_none());
        assert!(by_nreg(&conn, "").unwrap().is_none());
        // partial nreg is NOT a match (exact only).
        assert!(by_nreg(&conn, "2341").unwrap().is_none());
    }

    #[test]
    fn looks_like_nreg_heuristic() {
        assert!(looks_like_nreg("254к/96-вр"), "Cyrillic nreg letters");
        assert!(looks_like_nreg("1178-2022-п"));
        assert!(looks_like_nreg("n0423500-26"));
        assert!(looks_like_nreg("984_006-06"));
        assert!(!looks_like_nreg("кодекс")); // no digit/separator
        assert!(!looks_like_nreg("abc")); // too short
        assert!(!looks_like_nreg("2041")); // no separator
        assert!(!looks_like_nreg("два слова 12-3")); // multi-word
        assert!(!looks_like_nreg("has space 1-2"));
        assert!(!looks_like_nreg("1-2..3")); // dot not allowed
        assert!(!looks_like_nreg(""));
    }

    /// `seeded()` plus one more «кодекс» doc so AND and OR expressions give
    /// different answers. Kept separate from `seeded()` so the existing
    /// count-based tests stay valid.
    fn seeded_extra() -> Connection {
        let mut conn = seeded();
        let uk = "4\t5555-20\tПодатковий кодекс України\t2\t2\t2:20101111:5555-VI\t\t0\t20101118";
        let rows = parse_dump(uk).unwrap();
        upsert_docs(&mut conn, &rows).unwrap();
        conn
    }

    /// Count FTS rows matching a raw MATCH expression (bypasses `search()`).
    fn fts_count(conn: &Connection, expr: &str) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM docs_fts WHERE docs_fts MATCH ?1",
            [expr],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn fts_and_narrows() {
        let conn = seeded_extra();
        // Old OR behaviour matched both кодек docs; AND keeps only the
        // criminal-code one.
        assert_eq!(fts_count(&conn, &fts_expr_or("кримінальний кодекс")), 2);
        let (hits, total) = search(
            &conn,
            &SearchQuery {
                nazva: Some("кримінальний кодекс".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(total, 1);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].nreg, "2341-14");
    }

    #[test]
    fn fts_and_falls_back_to_or() {
        let conn = seeded_extra();
        // No doc contains both words → AND yields 0 → the same query is
        // retried with OR and finds each partial match.
        assert_eq!(fts_count(&conn, &fts_expr_and("кримінальний податковий")), 0);
        let (hits, total) = search(
            &conn,
            &SearchQuery {
                nazva: Some("кримінальний податковий".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(total, 2);
        let nregs: Vec<&str> = hits.iter().map(|h| h.nreg.as_str()).collect();
        assert!(nregs.contains(&"2341-14"));
        assert!(nregs.contains(&"5555-20"));
    }

    #[test]
    fn fts_prefix_matches_inflection() {
        let conn = seeded_extra();
        // The AND expression alone (no OR fallback to mask it) must match
        // «Кримінальний кодекс України» via the `кодек*` prefix.
        assert_eq!(fts_count(&conn, &fts_expr_and("кримінальний кодек")), 1);
        let (hits, total) = search(
            &conn,
            &SearchQuery {
                nazva: Some("кримінальний кодек".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(total, 1);
        assert_eq!(hits[0].nreg, "2341-14");
    }

    #[test]
    fn fts_expr_and_quotes_and_prefixes_last() {
        assert_eq!(
            fts_expr_and("кримінальний кодекс"),
            "\"кримінальний\" AND \"кодекс\"*"
        );
        // truncated last token → prefix query outside the quotes
        assert_eq!(
            fts_expr_and("кримінальний кодек"),
            "\"кримінальний\" AND \"кодек\"*"
        );
        // single token → just the quoted prefix token
        assert_eq!(fts_expr_and("код"), "\"код\"*");
        // whitespace runs collapse, empty tokens dropped
        assert_eq!(
            fts_expr_and("  один   два  три  "),
            "\"один\" AND \"два\" AND \"три\"*"
        );
        // inner `"` doubled
        assert_eq!(fts_expr_and("a\"b c"), "\"a\"\"b\" AND \"c\"*");
        // nothing but whitespace → empty expression
        assert_eq!(fts_expr_and("   "), "");
    }

    #[test]
    fn fts_expr_or_quotes_and_prefixes_last() {
        assert_eq!(
            fts_expr_or("кримінальний кодекс"),
            "\"кримінальний\" OR \"кодекс\"*"
        );
        assert_eq!(fts_expr_or("код"), "\"код\"*");
        assert_eq!(fts_expr_or("  a  b  "), "\"a\" OR \"b\"*");
        assert_eq!(fts_expr_or("a\"b"), "\"a\"\"b\"*");
        assert_eq!(fts_expr_or(""), "");
    }

    // ---- facets («Пошук» dictionary dropdown counts) -----------------------

    /// Three docs sharing the word «документ», each pair differing in
    /// status / types / org: (5, "2|168", 10), (1, "2", 10), (5, "168", 20).
    fn facet_seeded() -> Connection {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let uk = [
            "1\t1000-01\tПро перший документ\t5\t2|168\t10:20040923:1000-IV\t\t0\t20041203",
            "2\t2000-02\tПро другий документ\t1\t2\t10:20120810:2000-II\t\t0\t20120810",
            "3\t3000-03\tПро третій документ\t5\t168\t20:20150303:3000-III\t\t0\t20150303",
        ]
        .join("\n");
        let rows = parse_dump(&uk).unwrap();
        upsert_docs(&mut conn, &rows).unwrap();
        conn
    }

    #[test]
    fn facets_count_per_column_with_title_term() {
        let conn = facet_seeded();
        let f = facets(
            &conn,
            &SearchQuery {
                nazva: Some("документ".into()), // single word → LIKE path
                ..Default::default()
            },
        )
        .unwrap();
        // Count desc, then id; the multi-valued "2|168" counts in both buckets.
        assert_eq!(f.status, vec![(5, 2), (1, 1)]);
        assert_eq!(f.typ, vec![(2, 2), (168, 2)]);
        assert_eq!(f.org, vec![(10, 2), (20, 1)]);
    }

    #[test]
    fn facets_ignore_their_own_column_filter() {
        let conn = facet_seeded();
        let f = facets(
            &conn,
            &SearchQuery {
                status: Some(5),
                ..Default::default()
            },
        )
        .unwrap();
        // status facet ignores the status filter: the unselected option (1)
        // is still counted so the user can flip the choice.
        assert_eq!(f.status, vec![(5, 2), (1, 1)]);
        // …while the other facets honor it (only the two status-5 docs).
        assert_eq!(f.typ, vec![(168, 2), (2, 1)]);
        assert_eq!(f.org, vec![(10, 1), (20, 1)]);
    }

    #[test]
    fn facets_empty_table_returns_empty_vecs() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let f = facets(&conn, &SearchQuery::default()).unwrap();
        assert!(f.status.is_empty());
        assert!(f.typ.is_empty());
        assert!(f.org.is_empty());
    }

    #[test]
    fn facets_honor_date_and_number_filters() {
        let conn = facet_seeded();
        // Only the two docs adopted ≥ 2010 (status 1 + 5, types 2 + 168, org 10 + 20).
        let f = facets(
            &conn,
            &SearchQuery {
                date_from: Some("20100101".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(f.status, vec![(1, 1), (5, 1)]);
        assert_eq!(f.typ, vec![(2, 1), (168, 1)]);
        assert_eq!(f.org, vec![(10, 1), (20, 1)]);
        // org_num "2000-II" → a single doc.
        let f = facets(
            &conn,
            &SearchQuery {
                doc_number: Some("2000".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(f.status, vec![(1, 1)]);
        assert_eq!(f.typ, vec![(2, 1)]);
        assert_eq!(f.org, vec![(10, 1)]);
    }

    #[test]
    fn facets_follow_the_or_fallback_of_multi_word_search() {
        let conn = seeded_extra();
        // AND matches nothing → `search()` answers with the OR result set;
        // the facets must count that same set (both «кодекс» docs).
        assert_eq!(fts_count(&conn, &fts_expr_and("кримінальний податковий")), 0);
        let f = facets(
            &conn,
            &SearchQuery {
                nazva: Some("кримінальний податковий".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(f.status, vec![(1, 1), (2, 1)]); // 2341-14 + 5555-20
        assert_eq!(f.org, vec![(1, 1), (2, 1)]);
    }
}
