//! User annotations: bookmarks, highlights and comments per document.
//!
//! Stored in the same SQLite index (`annotations` table) so everything the
//! end user writes stays next to the data — no separate config files.

use anyhow::{anyhow, Context, Result};
use rusqlite::{params, Connection};
use serde::Serialize;

/// One annotation row.
#[derive(Debug, Clone, Serialize)]
pub struct Annotation {
    pub id: i64,
    /// Document nreg the annotation belongs to.
    pub nreg: String,
    /// `bookmark` | `highlight` | `comment`.
    pub kind: String,
    /// Free text: highlight snippet or comment body (empty for bookmarks).
    pub text: String,
    /// Character offsets into the text at creation time (highlights only).
    pub span_start: Option<i64>,
    pub span_end: Option<i64>,
    /// UTC `YYYY-MM-DD HH:MM:SS`.
    pub created_at: String,
}

/// All annotations for a document, newest first.
pub fn list(conn: &Connection, nreg: &str) -> Result<Vec<Annotation>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, nreg, kind, text, span_start, span_end, created_at
             FROM annotations WHERE nreg = ?1 ORDER BY id DESC",
        )
        .context("prepare annotations list")?;
    let mut rows = stmt
        .query_map(params![nreg], |r| {
            Ok(Annotation {
                id: r.get(0)?,
                nreg: r.get(1)?,
                kind: r.get(2)?,
                text: r.get(3)?,
                span_start: r.get(4)?,
                span_end: r.get(5)?,
                created_at: r.get(6)?,
            })
        })
        .context("run annotations list")?;
    let mut out = Vec::new();
    while let Some(row) = rows.next() {
        out.push(row.context("next annotation row")?);
    }
    Ok(out)
}

/// Insert an annotation; returns its id.
pub fn insert(
    conn: &Connection,
    nreg: &str,
    kind: &str,
    text: &str,
    span_start: Option<i64>,
    span_end: Option<i64>,
) -> Result<i64> {
    match kind {
        "bookmark" | "highlight" | "comment" => {}
        _ => return Err(anyhow!("unknown annotation kind {kind:?}")),
    }
    if kind != "bookmark" && text.trim().is_empty() {
        return Err(anyhow!(
            "{kind:?} annotation requires non-empty text (the highlight snippet / comment body)"
        ));
    }
    let now = crate::build::now_stamp();
    conn.execute(
        "INSERT INTO annotations(nreg, kind, text, span_start, span_end, created_at)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
        params![nreg, kind, text, span_start, span_end, now],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Delete by id; returns whether a row was removed.
pub fn remove(conn: &Connection, id: i64) -> Result<bool> {
    let n = conn.execute("DELETE FROM annotations WHERE id = ?1", [id])?;
    Ok(n > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::migrate;

    fn seeded() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn bookmark_round_trip() {
        let conn = seeded();
        let id = insert(&conn, "2341-14", "bookmark", "", None, None).unwrap();
        let items = list(&conn, "2341-14").unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, id);
        assert_eq!(items[0].kind, "bookmark");
        assert!(items[0].text.is_empty());
        assert!(remove(&conn, id).unwrap());
        assert!(list(&conn, "2341-14").unwrap().is_empty());
    }

    #[test]
    fn highlight_requires_text() {
        let conn = seeded();
        assert!(insert(&conn, "2341-14", "highlight", "  ", None, None).is_err());
        let id = insert(&conn, "2341-14", "highlight", "ст. 3", Some(12), Some(17)).unwrap();
        let items = list(&conn, "2341-14").unwrap();
        assert_eq!(items[0].span_start, Some(12));
        assert_eq!(items[0].span_end, Some(17));
        assert!(remove(&conn, id).unwrap());
    }

    #[test]
    fn unknown_kind_rejected() {
        let conn = seeded();
        assert!(insert(&conn, "2341-14", "note", "hi", None, None).is_err());
    }

    #[test]
    fn per_doc_isolation() {
        let conn = seeded();
        insert(&conn, "2041-15", "comment", "peremyka", None, None).unwrap();
        insert(&conn, "2341-14", "comment", "krym", None, None).unwrap();
        assert_eq!(list(&conn, "2041-15").unwrap().len(), 1);
        assert_eq!(list(&conn, "2341-14").unwrap().len(), 1);
    }
}
