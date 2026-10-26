//! Incremental sync from the updated-documents feed (`/laws/main/r.tsv`).
//!
//! The feed lists documents changed since the last server build (hourly).
//! For each entry we refresh the card in the local index (metadata only —
//! the body is always fetched on demand in the UI layer).

use anyhow::{Context, Result};
use rada_core::models::{DocCard, UpdatedDoc};
use rusqlite::{params, Connection};

use crate::dict::load_simple_dict;

/// Apply a batch of updated docs to the index.
///
/// `fetch_card` is called for each doc to refresh metadata; pass a closure
/// that talks to the API. Returns `(applied, failed)`.
pub fn apply_updates<F>(conn: &mut Connection, updates: &[UpdatedDoc], mut fetch_card: F) -> Result<(usize, usize)>
where
    F: FnMut(&str) -> Result<DocCard>,
{
    let mut applied = 0usize;
    let mut failed = 0usize;
    let tx = conn.transaction().context("begin sync tx")?;
    {
        let mut st = tx.prepare(
            "INSERT INTO docs
                (dokid, nreg, nazva, status, types, org_id, org_date, org_num,
                 organs, minjust, npix, poddat, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(nreg) DO UPDATE SET
                dokid = excluded.dokid, nazva = excluded.nazva,
                status = excluded.status, types = excluded.types,
                org_id = excluded.org_id, org_date = excluded.org_date,
                org_num = excluded.org_num, organs = excluded.organs,
                minjust = excluded.minjust, npix = excluded.npix,
                poddat = excluded.poddat, fetched_at = excluded.fetched_at",
        )?;
        let now = crate::build::now_stamp();
        for u in updates {
            match fetch_card(&u.nreg) {
                Ok(card) => {
                    let po = card.primary_org();
                    st.execute(params![
                        card.dokid,
                        card.nreg,
                        card.nazva,
                        card.status,
                        card.types,
                        po.as_ref().map(|p| p.org_id).unwrap_or(0),
                        po.as_ref().map(|p| p.org_date.as_str()).unwrap_or(""),
                        po.as_ref().map(|p| p.org_num.as_str()).unwrap_or(""),
                        card.organs,
                        card.minjust,
                        card.npix,
                        card.poddat,
                        now,
                    ])?;
                    applied += 1;
                }
                Err(_) => failed += 1,
            }
        }
    }
    tx.commit().context("commit sync tx")?;
    Ok((applied, failed))
}

/// Convenience: load the three standard dictionaries in one call.
/// `fetch_dict` returns raw bytes for a dictionary name.
///
/// Tolerant: a kind that fails to fetch or load (e.g. the server does not
/// publish that file) is skipped; the list of failed kinds (or
/// `"{kind}: {error}"` for load errors) is returned so the caller can
/// surface it without aborting the rest of the sync.
pub fn refresh_dicts<F>(conn: &mut Connection, mut fetch_dict: F) -> Result<Vec<String>>
where
    F: FnMut(&str) -> Result<Vec<u8>>,
{
    let mut failed = Vec::new();
    for kind in ["stan", "typs", "organs"] {
        let raw = match fetch_dict(kind) {
            Ok(raw) => raw,
            Err(e) => {
                failed.push(format!("{kind}: {e}"));
                continue;
            }
        };
        if let Err(e) = load_simple_dict(conn, kind, &raw) {
            failed.push(format!("{kind}: {e}"));
        }
    }
    Ok(failed)
}
