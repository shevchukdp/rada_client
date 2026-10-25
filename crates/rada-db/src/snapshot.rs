//! Gzip snapshots of the index for offline distribution.
//!
//! A snapshot is the SQLite file (WAL checkpointed first) wrapped in gzip.
//! Measured on the 2026-10-26 full dump: 187.1 MB raw → 43.3 MB gzip.
//! That fits under GitHub's 100 MB per-file limit, so monthly snapshots can
//! be published as repository files or release assets.

use anyhow::{Context, Result};
use flate2::{
    read::GzDecoder,
    write::GzEncoder,
    Compression,
};
use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};

/// Merge WAL into the main DB file so the snapshot is a single file.
pub fn checkpoint(conn: &Connection) -> Result<()> {
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .context("wal checkpoint")?;
    Ok(())
}

/// Create a gzip snapshot of the DB at `db_path`, writing to `out_path`.
/// Returns the snapshot size in bytes.
pub fn create(db_path: impl AsRef<Path>, out_path: impl AsRef<Path>) -> Result<u64> {
    let db_path = db_path.as_ref();
    let out_path = out_path.as_ref();

    let conn = Connection::open(db_path)
        .with_context(|| format!("open {}", db_path.display()))?;
    checkpoint(&conn)?;
    // Fully close before reading the file, so no hot journal lingers.
    drop(conn);

    let db = fs::File::open(db_path)
        .with_context(|| format!("read {}", db_path.display()))?;
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create dir {}", parent.display()))?;
        }
    }
    let out = fs::File::create(out_path)
        .with_context(|| format!("write {}", out_path.display()))?;
    let mut gz = GzEncoder::new(out, Compression::new(6));
    let mut src = db;
    std::io::copy(&mut src, &mut gz)
        .with_context(|| format!("gzip {}", db_path.display()))?;
    // Finish the gzip stream (writes the trailer) and drop the encoder,
    // which flushes and closes the underlying file.
    gz.try_finish()
        .with_context(|| format!("finish gzip {}", db_path.display()))?;
    drop(gz);
    let size = fs::metadata(out_path)
        .with_context(|| format!("read size {}", out_path.display()))?
        .len();
    Ok(size)
}

/// Restore a gzip snapshot from `src_path` to `out_db_path`.
pub fn restore(src_path: impl AsRef<Path>, out_db_path: impl AsRef<Path>) -> Result<PathBuf> {
    let src = src_path.as_ref();
    let out = out_db_path.as_ref();
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create dir {}", parent.display()))?;
        }
    }
    let in_file = fs::File::open(src)
        .with_context(|| format!("open {}", src.display()))?;
    let out_file = fs::File::create(out)
        .with_context(|| format!("write {}", out.display()))?;
    let mut gz = GzDecoder::new(in_file);
    let mut dst = out_file;
    std::io::copy(&mut gz, &mut dst).with_context(|| format!("gunzip {}", src.display()))?;

    // Sanity-check that the restored file is a usable SQLite DB.
    let conn = Connection::open(out)
        .with_context(|| format!("open restored {}", out.display()))?;
    let ok: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
        .unwrap_or_else(|_| "error".into());
    if ok != "ok" {
        anyhow::bail!("restored DB failed integrity check: {ok}");
    }
    let version: String = conn
        .query_row("SELECT sqlite_version()", [], |r| r.get(0))
        .context("sqlite_version")?;
    drop(conn);
    // Delete WAL/SHM siblings if present (we shipped a clean file).
    let _ = fs::remove_file(out.with_extension("sqlite-wal"));
    let _ = fs::remove_file(out.with_extension("sqlite-shm"));
    eprintln!(
        "[snapshot] restored {} (sqlite {})",
        out.display(),
        version
    );
    Ok(out.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_round_trip() {
        let dir = std::env::temp_dir().join("rada-db-snapshot-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let db = dir.join("t.sqlite");
        let gz = dir.join("t.sqlite.gz");

        let mut conn = crate::open(&db).unwrap();
        crate::dict::load_simple_dict(&mut conn, "stan", b"0\tNe viznacheno\n5\tChyunnyj\n").unwrap();
        drop(conn);

        let size = create(&db, &gz).unwrap();
        assert!(size > 0 && size < fs::metadata(&db).unwrap().len());

        let restored = restore(&gz, &dir.join("t2.sqlite")).unwrap();
        let conn = crate::open(&restored).unwrap();
        assert_eq!(crate::dict::label(&conn, "stan", 5), Some("Chyunnyj".into()));
        let _ = fs::remove_dir_all(&dir);
    }
}
