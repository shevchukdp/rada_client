//! Full-dump import (`doc.txt`) into the index.

use anyhow::{bail, Context, Result};
use encoding_rs::WINDOWS_1251;
use rada_core::models::DocCard;
use rusqlite::{params, Connection};
use std::time::Instant;

use crate::set_meta;

/// Import a raw `doc.txt` dump (Windows-1251 bytes) into `docs`.
///
/// Returns the number of rows imported. Uses a single transaction; the
/// upsert is keyed on `nreg` so re-running on a new dump is safe.
pub fn import_dump(conn: &mut Connection, raw: &[u8]) -> Result<usize> {
    let t = Instant::now();
    let (text, _, _) = WINDOWS_1251.decode(raw);
    let rows = parse_dump(&text)?;
    let n = upsert_docs(conn, &rows)?;

    set_meta(conn, "last_import", &now_stamp()).context("record last_import")?;

    eprintln!(
        "[import] {n} docs from {} bytes in {:?}",
        raw.len(),
        t.elapsed()
    );
    Ok(n)
}

/// Parse `doc.txt` content (already UTF-8) into cards.
pub fn parse_dump(text: &str) -> Result<Vec<DocCard>> {
    let mut out = Vec::with_capacity(300_000);
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        match DocCard::from_tsv_line(line) {
            Some(c) => out.push(c),
            // Malformed lines are skipped, not fatal.
            None => {}
        }
    }
    if out.is_empty() {
        bail!("no valid rows parsed from dump");
    }
    Ok(out)
}

/// Upsert cards into `docs` inside one transaction.
pub fn upsert_docs(conn: &mut Connection, rows: &[DocCard]) -> Result<usize> {
    let tx = conn.transaction().context("begin import tx")?;
    {
        let mut st = tx.prepare(
            "INSERT INTO docs
                (dokid, nreg, nazva, status, types, org_id, org_date, org_num,
                 organs, minjust, npix, poddat, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(nreg) DO UPDATE SET
                dokid      = excluded.dokid,
                nazva      = excluded.nazva,
                status     = excluded.status,
                types      = excluded.types,
                org_id     = excluded.org_id,
                org_date   = excluded.org_date,
                org_num    = excluded.org_num,
                organs     = excluded.organs,
                minjust    = excluded.minjust,
                npix       = excluded.npix,
                poddat     = excluded.poddat,
                fetched_at = excluded.fetched_at",
        )?;
        let now = now_stamp();
        for r in rows {
            let po = r.primary_org();
            st.execute(params![
                r.dokid,
                r.nreg,
                r.nazva,
                r.status,
                r.types,
                po.as_ref().map(|p| p.org_id).unwrap_or(0),
                po.as_ref().map(|p| p.org_date.as_str()).unwrap_or(""),
                po.as_ref().map(|p| p.org_num.as_str()).unwrap_or(""),
                r.organs,
                r.minjust,
                r.npix,
                r.poddat,
                now,
            ])?;
        }
    }
    tx.commit().context("commit import tx")?;
    Ok(rows.len())
}

/// UTC `YYYY-MM-DD HH:MM:SS` without pulling in chrono for the hot path.
pub fn now_stamp() -> String {
    // A few seconds of staleness is acceptable for an import stamp.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let (days, rem) = (now / 86400, now % 86400);
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // civil-from-days algorithm (Howard Hinnant)
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02}:{s:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A realistic `doc.txt` line (Windows-1251, as on the server).
    fn sample_line() -> Vec<u8> {
        let uk = "557982\tn0424500-26\tПро облікову ціну банківських металів\t0\t95\t70:20261023:\t\t2\t20261023";
        let (enc, _, _) = WINDOWS_1251.encode(uk);
        enc.to_vec()
    }
    #[test]
    fn parses_one_line() {
        let raw = sample_line();
        let (text, _, _) = WINDOWS_1251.decode(&raw);
        let rows = parse_dump(&text).unwrap();
        assert_eq!(rows.len(), 1);
        let r = &rows[0];
        assert_eq!(r.dokid, 557982);
        assert_eq!(r.nreg, "n0424500-26");
        assert_eq!(r.nazva, "Про облікову ціну банківських металів");
        assert_eq!(r.status, 0);
        assert_eq!(r.poddat, "20261023");
        assert_eq!(r.primary_org().unwrap().org_id, 70);
        assert_eq!(r.primary_org().unwrap().org_date, "20261023");
    }

    #[test]
    fn imports_and_round_trips() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::schema::migrate(&conn).unwrap();
        let raw = sample_line();
        let n = import_dump(&mut conn, &raw).unwrap();
        assert_eq!(n, 1);
        let nazva: String = conn
            .query_row(
                "SELECT nazva FROM docs WHERE nreg='n0424500-26'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(nazva, "Про облікову ціну банківських металів");
    }
}
