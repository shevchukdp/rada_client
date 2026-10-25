//! Title search over the local index.
//!
//! Strategy:
//! - multi-word query  → FTS5 MATCH (case-insensitive, ranked);
//! - single word       → `LIKE` substring (FTS can't do substrings);
//! - plus optional filters (org / type / status / date range / number).
//!
//! Numbered placeholders are used throughout so argument order is explicit.

use anyhow::{Context, Result};
use rusqlite::Connection;
use serde::Serialize;

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

fn is_multi_word(term: &str) -> bool {
    let t = term.trim();
    !t.is_empty() && t.split_whitespace().count() >= 2
}

/// Build a safe FTS5 expression: quote every token, OR them together.
fn fts_expr(term: &str) -> String {
    term.split_whitespace()
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{}\"", w.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" OR ")
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

/// Execute a search; returns `(hits, total)`.
pub fn search(conn: &Connection, q: &SearchQuery) -> Result<(Vec<SearchHit>, i64)> {
    let title = q
        .nazva
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let use_fts = title.as_deref().map(is_multi_word).unwrap_or(false);

    let mut args: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut where_sql = String::new();

    // In the FTS path, ?1 is the MATCH expression.
    if use_fts {
        args.push(Box::new(fts_expr(title.as_deref().unwrap_or(""))));
    }

    if let Some(t) = &title {
        if !use_fts {
            add_param(&mut where_sql, &mut args, " AND nazva LIKE ", format!("%{t}%"));
        }
    }
    if let Some(org) = q.org_id {
        add_param(&mut where_sql, &mut args, " AND org_id = ", org);
    }
    if let Some(typ) = q.typ {
        // `types` is stored as "2|168|11"; pad with '|' on both sides for an
        // exact member match.
        add_param(
            &mut where_sql,
            &mut args,
            " AND ('||' || types || '|') LIKE ",
            format!("%|{typ}|%"),
        );
    }
    if let Some(st) = q.status {
        add_param(&mut where_sql, &mut args, " AND status = ", st);
    }
    if let Some(d) = &q.date_from {
        add_param(&mut where_sql, &mut args, " AND org_date >= ", d.clone());
    }
    if let Some(d) = &q.date_to {
        add_param(&mut where_sql, &mut args, " AND org_date <= ", d.clone());
    }
    if let Some(n) = &q.doc_number {
        add_param(
            &mut where_sql,
            &mut args,
            " AND org_num LIKE ",
            format!("%{n}%"),
        );
    }

    let (base, order) = if use_fts {
        (
            "SELECT d.nreg, d.nazva, d.status, d.org_date, d.org_num, d.poddat
             FROM docs_fts f JOIN docs d ON d.rowid = f.rowid
             WHERE docs_fts MATCH ?1",
            " ORDER BY f.rank",
        )
    } else {
        (
            "SELECT nreg, nazva, status, org_date, org_num, poddat FROM docs
             WHERE 1=1",
            " ORDER BY org_date DESC, nreg",
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
}
