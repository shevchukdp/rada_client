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
