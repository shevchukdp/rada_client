//! Schema and migrations for the local index.

use anyhow::{Context, Result};
use rusqlite::Connection;

const SCHEMA_VERSION: i64 = 3;

/// Bring the database to the current schema (idempotent).
pub fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS meta(
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS docs(
            dokid    INTEGER PRIMARY KEY,
            nreg     TEXT NOT NULL UNIQUE,
            nazva    TEXT NOT NULL,
            status   INTEGER NOT NULL DEFAULT 0,
            types    TEXT NOT NULL DEFAULT '',
            org_id   INTEGER NOT NULL DEFAULT 0,
            org_date TEXT NOT NULL DEFAULT '',
            org_num  TEXT NOT NULL DEFAULT '',
            organs   TEXT NOT NULL DEFAULT '',
            minjust  TEXT NOT NULL DEFAULT '',
            npix     INTEGER NOT NULL DEFAULT 0,
            poddat   TEXT NOT NULL DEFAULT '',
            fetched_at TEXT NOT NULL DEFAULT ''
        );
        CREATE INDEX IF NOT EXISTS idx_docs_nazva  ON docs(nazva);
        CREATE INDEX IF NOT EXISTS idx_docs_status ON docs(status);
        CREATE INDEX IF NOT EXISTS idx_docs_org    ON docs(org_id);
        CREATE INDEX IF NOT EXISTS idx_docs_date   ON docs(org_date);
        CREATE INDEX IF NOT EXISTS idx_docs_podd   ON docs(poddat);

        -- Full-text index over the title (contentless FTS5 with sync triggers).
        CREATE VIRTUAL TABLE IF NOT EXISTS docs_fts USING fts5(
            nazva,
            content='docs',
            content_rowid='rowid'
        );
        CREATE TRIGGER IF NOT EXISTS docs_fts_ai AFTER INSERT ON docs BEGIN
            INSERT INTO docs_fts(rowid, nazva) VALUES (new.rowid, new.nazva);
        END;
        CREATE TRIGGER IF NOT EXISTS docs_fts_ad AFTER DELETE ON docs BEGIN
            INSERT INTO docs_fts(docs_fts, rowid, nazva) VALUES('delete', old.rowid, old.nazva);
        END;
        CREATE TRIGGER IF NOT EXISTS docs_fts_au AFTER UPDATE OF nazva ON docs BEGIN
            INSERT INTO docs_fts(docs_fts, rowid, nazva) VALUES('delete', old.rowid, old.nazva);
            INSERT INTO docs_fts(rowid, nazva) VALUES (new.rowid, new.nazva);
        END;

        -- Reference dictionaries (id -> label).
        CREATE TABLE IF NOT EXISTS dict(
            kind  TEXT NOT NULL,           -- 'stan' | 'typs' | 'organs' | ...
            id    INTEGER NOT NULL,
            label TEXT NOT NULL,
            PRIMARY KEY (kind, id)
        );
        -- User annotations (bookmarks / highlights / comments).
        CREATE TABLE IF NOT EXISTS annotations(
            id         INTEGER PRIMARY KEY,
            nreg       TEXT NOT NULL,
            kind       TEXT NOT NULL CHECK(kind IN ('bookmark','highlight','comment')),
            text       TEXT NOT NULL DEFAULT '',
            span_start INTEGER,
            span_end   INTEGER,
            created_at TEXT NOT NULL DEFAULT ''
        );
        CREATE INDEX IF NOT EXISTS idx_ann_nreg ON annotations(nreg);

        -- Offline cache of document bodies (full text fetched on demand).
        -- `edition` is opaque: '' for the current text, an edition id
        -- (e.g. 'ed20241115') for a specific historical edition.
        CREATE TABLE IF NOT EXISTS docs_text(
            nreg       TEXT NOT NULL,
            edition    TEXT NOT NULL DEFAULT '',
            fetched_at TEXT NOT NULL,
            body       TEXT NOT NULL,
            PRIMARY KEY (nreg, edition)
        );
        "#,
    )
    .context("apply schema")?;

    let cur: i64 = conn
        .query_row("SELECT COALESCE(value,'0') FROM meta WHERE key='schema_version'", [], |r| {
            r.get(0)
        })
        .unwrap_or(0);
    if cur < SCHEMA_VERSION {
        conn.execute(
            "INSERT INTO meta(key,value) VALUES('schema_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [SCHEMA_VERSION.to_string()],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM sqlite_master", [], |r| r.get(0))
            .unwrap();
        assert!(n > 5);
    }
}
