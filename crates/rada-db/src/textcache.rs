//! Offline cache of document bodies (`docs_text`).
//!
//! The UI fetches full text from the API on demand; every successful fetch
//! is stored here so the document stays readable offline. Keys are
//! `(nreg, edition)` pairs where `edition` is opaque: callers pass `""`
//! for the current text and the edition id (e.g. `"ed20241115"`) for a
//! specific historical edition.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

/// Insert or replace the cached body for `(nreg, edition)`; `fetched_at`
/// is refreshed to the current UTC stamp on every call.
pub fn put(conn: &Connection, nreg: &str, edition: &str, body: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO docs_text(nreg, edition, fetched_at, body)
         VALUES(?1, ?2, ?3, ?4)
         ON CONFLICT(nreg, edition) DO UPDATE SET
            fetched_at = excluded.fetched_at,
            body       = excluded.body",
        params![nreg, edition, crate::build::now_stamp(), body],
    )
    .context("put docs_text")?;
    Ok(())
}

/// Exact `(nreg, edition)` lookup.
pub fn get(conn: &Connection, nreg: &str, edition: &str) -> Result<Option<String>> {
    match conn.query_row(
        "SELECT body FROM docs_text WHERE nreg = ?1 AND edition = ?2",
        params![nreg, edition],
        |r| r.get::<_, String>(0),
    ) {
        Ok(body) => Ok(Some(body)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e).context("get docs_text"),
    }
}

/// True when *any* edition of `nreg` is cached.
pub fn cached(conn: &Connection, nreg: &str) -> Result<bool> {
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM docs_text WHERE nreg = ?1",
            params![nreg],
            |r| r.get(0),
        )
        .context("cached docs_text")?;
    Ok(n > 0)
}

/// Cache size: `(rows, total body bytes)`.
pub fn stats(conn: &Connection) -> Result<(i64, i64)> {
    let pair = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(LENGTH(body)), 0) FROM docs_text",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .context("stats docs_text")?;
    Ok(pair)
}

/// Delete one document's rows (`Some(nreg)`) or the whole cache (`None`);
/// returns the number of deleted rows.
pub fn purge(conn: &Connection, nreg: Option<&str>) -> Result<usize> {
    let deleted = match nreg {
        Some(nreg) => conn
            .execute("DELETE FROM docs_text WHERE nreg = ?1", params![nreg])
            .context("purge docs_text")?,
        None => conn
            .execute("DELETE FROM docs_text", [])
            .context("purge docs_text")?,
    };
    Ok(deleted)
}

/// Collapse whitespace and cut a window of `max_chars` chars around `pos`
/// (or from the start), adding ellipses when clipped.
fn window(flat: &str, pos: Option<usize>, max_chars: usize) -> String {
    let chars: Vec<char> = flat.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    let max = max_chars.clamp(20, 2000);
    let (start, end) = match pos {
        Some(p) => {
            let start = p.saturating_sub(max / 3);
            (start, (start + max).min(chars.len()))
        }
        None => (0, max.min(chars.len())),
    };
    let mut out: String = chars[start..end].iter().collect();
    if start > 0 {
        out = format!("…{out}");
    }
    if end < chars.len() {
        out.push('…');
    }
    out
}

/// Short context snippet of the cached current text for favorites/lists:
/// a window centered (case-insensitively) on `needle` when it occurs,
/// otherwise the document opening. Whitespace is collapsed. `None` when
/// nothing is cached for `nreg`.
pub fn preview(conn: &Connection, nreg: &str, needle: &str, max_chars: usize) -> Option<String> {
    let body = get(conn, nreg, "").ok()??;
    let flat: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() {
        return None;
    }
    let pos = find_ci(&flat, needle);
    Some(window(&flat, pos, max_chars))
}

/// Case-insensitive (Unicode) char-index of `needle` in `hay`; `None` when
/// the needle is blank or absent.
fn find_ci(hay: &str, needle: &str) -> Option<usize> {
    let ndl = needle.trim().to_lowercase();
    if ndl.is_empty() {
        return None;
    }
    let hay_chars: Vec<char> = hay.to_lowercase().chars().collect();
    let needle_chars: Vec<char> = ndl.chars().collect();
    if needle_chars.len() > hay_chars.len() {
        return None;
    }
    'outer: for i in 0..=hay_chars.len() - needle_chars.len() {
        for j in 0..needle_chars.len() {
            if hay_chars[i + j] != needle_chars[j] {
                continue 'outer;
            }
        }
        return Some(i);
    }
    None
}

/// One hit of [`search_bodies`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheHit {
    pub nreg: String,
    /// Number of occurrences of the needle in the document.
    pub hits: usize,
    /// Context window around the first occurrence.
    pub snippet: String,
}

/// Offline full-text search over the cached *current* texts: Unicode
/// case-insensitive substring scan in Rust (SQLite `LIKE`/FTS do not fold
/// Cyrillic without ICU). Results are ranked by occurrence count (desc),
/// ties by nreg, capped at `max_results`.
pub fn search_bodies(
    conn: &Connection,
    needle: &str,
    max_results: usize,
) -> Result<Vec<CacheHit>> {
    let ndl = needle.trim().to_lowercase();
    if ndl.is_empty() {
        return Ok(Vec::new());
    }
    let needle_chars: Vec<char> = ndl.chars().collect();
    let mut stmt = conn
        .prepare("SELECT nreg, body FROM docs_text WHERE edition = ''")
        .context("search_bodies docs_text")?;
    let mut rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .context("search_bodies docs_text")?;
    let mut found: Vec<CacheHit> = Vec::new();
    while let Some((nreg, body)) = rows.next().transpose()? {
        let flat: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
        let low: Vec<char> = flat.to_lowercase().chars().collect();
        if low.len() < needle_chars.len() {
            continue;
        }
        let mut hits = 0usize;
        let mut first: Option<usize> = None;
        for i in 0..=low.len() - needle_chars.len() {
            let mut same = true;
            for j in 0..needle_chars.len() {
                if low[i + j] != needle_chars[j] {
                    same = false;
                    break;
                }
            }
            if same {
                hits += 1;
                if first.is_none() {
                    first = Some(i);
                }
            }
        }
        if hits > 0 {
            found.push(CacheHit {
                nreg,
                hits,
                snippet: window(&flat, first, 200),
            });
        }
    }
    found.sort_by(|a, b| b.hits.cmp(&a.hits).then_with(|| a.nreg.cmp(&b.nreg)));
    found.truncate(max_results);
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::migrate;

    fn conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        conn
    }

    /// put/get round-trip (incl. Cyrillic); unknown key → None.
    #[test]
    fn put_get_roundtrip() {
        let conn = conn();
        put(&conn, "1556-18", "", "Про вищу освіту").unwrap();
        assert_eq!(
            get(&conn, "1556-18", "").unwrap().as_deref(),
            Some("Про вищу освіту")
        );
        assert_eq!(get(&conn, "1556-18", "ed20241115").unwrap(), None);
        assert_eq!(get(&conn, "0000-00", "").unwrap(), None);
    }

    /// preview(): needle window, opening fallback, whitespace collapse, miss.
    #[test]
    fn preview_windows_and_falls_back() {
        let conn = conn();
        assert_eq!(preview(&conn, "1-1", "", 50), None, "nothing cached");
        let body = "Стаття 1.   Визначення\n\t понять.     ".to_string()
            + &"Дуже довгий текст основи. ".repeat(20)
            + "Ключова фраза тут. Далі ще текст.";
        put(&conn, "1-1", "", &body).unwrap();
        // No needle: document opening, collapsed, with a tail ellipsis.
        let head = preview(&conn, "1-1", "", 60).unwrap();
        assert!(head.starts_with("Стаття 1. Визначення понять."), "{head}");
        assert!(head.ends_with('…'), "{head}");
        // Needle found: window is centered near the match.
        let win = preview(&conn, "1-1", "ключова фраза", 80).unwrap();
        assert!(win.contains("Ключова фраза"), "{win}");
        assert!(win.starts_with('…'), "window starts before the match: {win}");
        // Needle absent: still the opening.
        let miss = preview(&conn, "1-1", "голова не болить", 60).unwrap();
        assert!(miss.starts_with("Стаття 1."), "{miss}");
        // Short bodies have no ellipses at all.
        put(&conn, "2-2", "", "короткий текст").unwrap();
        assert_eq!(preview(&conn, "2-2", "", 60).unwrap(), "короткий текст");
    }

    /// search_bodies(): Unicode case folding (Cyrillic), ranking, edition
    /// filter, snippet around the first hit, empty-needle guard.
    #[test]
    fn search_bodies_ranks_and_folds_case() {
        let conn = conn();
        put(&conn, "a-1", "", "Стаття 1. КРИМІНАЛЬНА відповідальність настає за КРИМІНАЛЬНИЙ злочин.").unwrap();
        put(&conn, "b-2", "", "кримінальний кодекс у тексту одного разу").unwrap();
        put(&conn, "c-3", "", "повністю інший акт про освіту").unwrap();
        put(&conn, "d-4", "ed20200101", "кримінальний кримінальний кримінальний").unwrap();
        // Empty needle: nothing.
        assert!(search_bodies(&conn, "  ", 10).unwrap().is_empty());
        let hits = search_bodies(&conn, "КРИМІНАЛЬН", 10).unwrap();
        // d-4 has 3 occurrences but only the CURRENT edition counts → excluded.
        assert_eq!(
            hits.iter().map(|h| h.nreg.as_str()).collect::<Vec<_>>(),
            ["a-1", "b-2"],
            "ranked by occurrences, current edition only"
        );
        assert_eq!(hits[0].hits, 2);
        assert_eq!(hits[1].hits, 1);
        // Snippet contains the match (folded comparison, no ellipsis for short docs).
        assert!(hits[1].snippet.contains("кримінальний"), "{}", hits[1].snippet);
        // max_results cap.
        assert_eq!(search_bodies(&conn, "кримінальн", 1).unwrap().len(), 1);
    }

    /// Editions are distinct keys; re-putting the same key replaces the body.
    #[test]
    fn editions_distinct_and_upsert_replaces() {
        let conn = conn();
        put(&conn, "1556-18", "", "поточна редакція").unwrap();
        put(&conn, "1556-18", "ed20241115", "редакція від 2024").unwrap();
        assert_eq!(get(&conn, "1556-18", "").unwrap().as_deref(), Some("поточна редакція"));
        assert_eq!(
            get(&conn, "1556-18", "ed20241115").unwrap().as_deref(),
            Some("редакція від 2024")
        );

        // UPSERT: same key must replace the body (not duplicate) and refresh fetched_at.
        put(&conn, "1556-18", "", "нова редакція").unwrap();
        assert_eq!(get(&conn, "1556-18", "").unwrap().as_deref(), Some("нова редакція"));
        let (n, fetched_at): (i64, String) = conn
            .query_row(
                "SELECT COUNT(*), MAX(fetched_at) FROM docs_text WHERE nreg = '1556-18' AND edition = ''",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(n, 1, "upsert must not create a second row");
        assert!(!fetched_at.is_empty(), "fetched_at must be stamped");
    }

    /// cached() is true for any edition, false when nothing is stored.
    #[test]
    fn cached_any_edition() {
        let conn = conn();
        assert!(!cached(&conn, "1556-18").unwrap());
        put(&conn, "1556-18", "ed20241115", "лише стара редакція").unwrap();
        assert!(cached(&conn, "1556-18").unwrap());
        assert!(!cached(&conn, "9999-26").unwrap());
    }

    /// stats() counts rows and body bytes (ASCII body: LENGTH = bytes).
    #[test]
    fn stats_counts_rows_and_bytes() {
        let conn = conn();
        assert_eq!(stats(&conn).unwrap(), (0, 0));
        let a = "law text one"; // 12 ASCII bytes
        let b = "law text two three"; // 18 ASCII bytes
        assert_eq!(a.len(), 12);
        assert_eq!(b.len(), 18);
        put(&conn, "1-1", "", a).unwrap();
        put(&conn, "1-1", "ed2", b).unwrap();
        assert_eq!(stats(&conn).unwrap(), (2, (a.len() + b.len()) as i64));
    }

    /// purge(Some) removes only that doc; purge(None) empties the table.
    #[test]
    fn purge_one_and_all() {
        let conn = conn();
        put(&conn, "1-1", "", "a").unwrap();
        put(&conn, "1-1", "ed2", "b").unwrap();
        put(&conn, "2-2", "", "c").unwrap();

        assert_eq!(purge(&conn, Some("1-1")).unwrap(), 2);
        assert!(!cached(&conn, "1-1").unwrap());
        assert!(cached(&conn, "2-2").unwrap());
        assert_eq!(get(&conn, "2-2", "").unwrap().as_deref(), Some("c"));

        assert_eq!(purge(&conn, Some("missing")).unwrap(), 0);
        assert_eq!(purge(&conn, None).unwrap(), 1);
        assert_eq!(stats(&conn).unwrap(), (0, 0));
    }

    /// migrate() is idempotent across the v2→v3 bump and creates docs_text.
    #[test]
    fn migrate_twice_creates_docs_text() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        // The new table exists and is writable through the cache API.
        put(&conn, "1556-18", "", "тіло").unwrap();
        assert_eq!(get(&conn, "1556-18", "").unwrap().as_deref(), Some("тіло"));
    }
}
