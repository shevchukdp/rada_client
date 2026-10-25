//! Reference dictionaries: status, document types, publishers.
//!
//! Sources (verified):
//! - `/ogd/zak/laws/data/csv/stan.txt`  — `id<TAB>label` (Windows-1251);
//! - `/ogd/zak/laws/data/csv/typs.txt`  — same shape;
//! - `/ogd/zak/laws/data/csv/organs.txt` — `id<TAB>full label` (Windows-1251).

use anyhow::{Context, Result};
use encoding_rs::WINDOWS_1251;
use rusqlite::{params, Connection};

/// Parse a `id<TAB>label` dictionary (Windows-1251 bytes).
/// Lines whose first column is not an integer are skipped.
pub fn parse_simple_dict(raw: &[u8]) -> Vec<(i64, String)> {
    let (text, _, _) = WINDOWS_1251.decode(raw);
    let mut out = Vec::new();
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, '\t');
        let id = parts.next().unwrap_or("").trim();
        let label = parts.next().unwrap_or("").trim();
        if let Ok(id) = id.parse::<i64>() {
            if !label.is_empty() {
                out.push((id, label.to_string()));
            }
        }
    }
    out
}

/// Upsert a dictionary into `dict(kind, id, label)`.
pub fn load_simple_dict(conn: &mut Connection, kind: &str, raw: &[u8]) -> Result<usize> {
    let rows = parse_simple_dict(raw);
    let tx = conn.transaction().context("begin dict tx")?;
    {
        let mut st = tx.prepare(
            "INSERT INTO dict(kind, id, label) VALUES(?1, ?2, ?3)
             ON CONFLICT(kind, id) DO UPDATE SET label = excluded.label",
        )?;
        for (id, label) in &rows {
            st.execute(params![kind, id, label])?;
        }
    }
    tx.commit().context("commit dict tx")?;
    Ok(rows.len())
}

/// Resolve a dictionary id to its label.
pub fn label(conn: &Connection, kind: &str, id: i64) -> Option<String> {
    conn.query_row(
        "SELECT label FROM dict WHERE kind=?1 AND id=?2",
        params![kind, id],
        |r| r.get::<_, String>(0),
    )
    .ok()
}

/// All (id, label) pairs of a dictionary, sorted by id.
pub fn all_labels(conn: &Connection, kind: &str) -> Vec<(i64, String)> {
    let mut out = Vec::new();
    let stmt = conn
        .prepare("SELECT id, label FROM dict WHERE kind=?1 ORDER BY id")
        .ok();
    if let Some(mut st) = stmt {
        if let Ok(rows) = st.query_map(params![kind], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        }) {
            for row in rows.flatten() {
                out.push(row);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cp1251_dict() {
        let uk = "0\tНе визначено\n5\tЧинний\n9\tВтратив чинність крім окремих положень\n";
        let (enc, _, _) = WINDOWS_1251.encode(uk);
        let rows = parse_simple_dict(&enc);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], (0, "Не визначено".to_string()));
        assert_eq!(rows[1].1, "Чинний");
    }

    #[test]
    fn round_trips_labels() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::schema::migrate(&conn).unwrap();
        let uk = "1\tВтратив чинність\n5\tЧинний\n";
        let (enc, _, _) = WINDOWS_1251.encode(uk);
        load_simple_dict(&mut conn, "stan", &enc).unwrap();
        assert_eq!(label(&conn, "stan", 5), Some("Чинний".into()));
        assert_eq!(label(&conn, "stan", 42), None);
        assert_eq!(all_labels(&conn, "stan").len(), 2);
    }
}
