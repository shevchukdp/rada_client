//! Local SQLite index for the "Законодавство України" database.
//!
//! Module layout (each item is an independent, testable unit):
//! - [`schema`] — DDL + migrations;
//! - [`build`]  — full dump (`doc.txt`) import;
//! - [`search`] — title search (FTS5 + LIKE) with filters and paging;
//! - [`dict`]   — reference dictionaries (status/type/publisher);
//! - [`sync`]   — incremental updates from `/laws/main/r.tsv`;
//! - [`snapshot`] — gzip snapshot export/import for offline distribution;
//! - [`annotations`] — user bookmarks / highlights / comments;
//! - [`textcache`] — offline cache of document bodies (`docs_text`).
//!
//! Measured performance (295 340 docs, 2026-10-26):
//! - full dump parse ≈ 1 s, raw file 51.7 MB;
//! - index + FTS5 ≈ 187 MB on disk, ≈ 43 MB gzip;
//! - FTS5 title search ≈ 2 ms, LIKE ≈ 250–350 ms.

pub mod annotations;
pub mod build;
pub mod dict;
pub mod schema;
pub mod search;
pub mod snapshot;
pub mod sync;
pub mod textcache;

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;

pub use annotations::Annotation;
pub use schema::migrate;
pub use search::{by_nreg, looks_like_nreg, SearchHit, SearchQuery};

/// Open (or create) the index at `path` and ensure the schema is current.
pub fn open(path: impl AsRef<Path>) -> Result<Connection> {
    if let Some(parent) = path.as_ref().parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create dir {}", parent.display()))?;
        }
    }
    let conn = Connection::open(path.as_ref())
        .with_context(|| format!("open {}", path.as_ref().display()))?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
    migrate(&conn)?;
    Ok(conn)
}

/// Number of indexed documents.
pub fn doc_count(conn: &Connection) -> Result<i64> {
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM docs", [], |r| r.get(0))?;
    Ok(n)
}

/// Last import timestamp (UTC `YYYY-MM-DD HH:MM:SS`), if any.
pub fn last_import(conn: &Connection) -> Option<String> {
    conn.query_row("SELECT value FROM meta WHERE key='last_import'", [], |r| {
        r.get::<_, String>(0)
    })
    .ok()
}

/// Title of the document registered under `nreg`, if indexed.
pub fn title_by_nreg(conn: &Connection, nreg: &str) -> Option<String> {
    conn.query_row(
        "SELECT nazva FROM docs WHERE nreg = ?1",
        [nreg],
        |r| r.get::<_, String>(0),
    )
    .ok()
}

/// Store a meta value.
pub fn set_meta(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO meta(key,value) VALUES(?1,?2)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        (key, value),
    )?;
    Ok(())
}
