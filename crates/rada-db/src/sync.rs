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
                dokid = CASE WHEN excluded.dokid = 0 THEN docs.dokid ELSE excluded.dokid END,
                nazva = CASE WHEN excluded.nazva = '' THEN docs.nazva ELSE excluded.nazva END,
                status = excluded.status, types = excluded.types,
                org_id = CASE WHEN excluded.org_id = 0 THEN docs.org_id ELSE excluded.org_id END,
                org_date = CASE WHEN excluded.org_date = '' THEN docs.org_date ELSE excluded.org_date END,
                org_num = CASE WHEN excluded.org_num = '' THEN docs.org_num ELSE excluded.org_num END,
                organs = CASE WHEN excluded.organs = '' THEN docs.organs ELSE excluded.organs END,
                minjust = CASE WHEN excluded.minjust = '' THEN docs.minjust ELSE excluded.minjust END,
                npix = CASE WHEN excluded.npix = 0 THEN docs.npix ELSE excluded.npix END,
                poddat = CASE WHEN excluded.poddat = '' THEN docs.poddat ELSE excluded.poddat END,
                fetched_at = excluded.fetched_at",
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

/// Persist a snapshot of the arrivals feed (`r.tsv`) into `meta`, together
/// with its save time, so the page can be shown offline.
pub fn save_arrivals(conn: &Connection, docs: &[UpdatedDoc]) -> Result<()> {
    let json = serde_json::to_string(docs).context("encode arrivals")?;
    crate::set_meta(conn, "arrivals", &json)?;
    crate::set_meta(conn, "arrivals_at", &crate::build::now_stamp())
}

/// Last saved arrivals snapshot: `(docs, saved_at)`, `None` when absent or
/// unreadable.
pub fn load_arrivals(conn: &Connection) -> Option<(Vec<UpdatedDoc>, String)> {
    let json = conn
        .query_row("SELECT value FROM meta WHERE key='arrivals'", [], |r| {
            r.get::<_, String>(0)
        })
        .ok()?;
    let at = conn
        .query_row("SELECT value FROM meta WHERE key='arrivals_at'", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap_or_default();
    serde_json::from_str(&json).ok().map(|d| (d, at))
}

/// Convenience: load the three standard dictionaries in one call./// `fetch_dict` returns raw bytes for a dictionary name.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::migrate;
    use rada_core::models::UpdatedDoc;

    fn doc(nreg: &str, status: i64) -> rada_core::models::DocCard {
        rada_core::models::DocCard {
            dokid: 4242,
            nreg: nreg.into(),
            nazva: "Про вищу освіту".into(),
            status,
            types: "12".into(),
            organs: "291:20140705:1556-VIII".into(),
            minjust: "1234".into(),
            npix: 0,
            poddat: "20200101".into(),
        }
    }

    /// Syncing with a card that lacks dokid/org_num/nazva (the JSON card
    /// path fabricates zeros/empties) must NOT wipe the stored values.
    #[test]
    fn apply_updates_preserves_unknown_fields() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        crate::build::upsert_docs(&mut conn, &[doc("1556-18", 1)]).unwrap();

        let degraded = |nreg: &str| -> Result<rada_core::models::DocCard> {
            Ok(rada_core::models::DocCard {
                dokid: 0,
                nreg: nreg.into(),
                nazva: String::new(),
                status: 43, // the only genuinely new field
                types: "12".into(),
                organs: String::new(),
                minjust: String::new(),
                npix: 0,
                poddat: String::new(),
            })
        };
        let upd = vec![UpdatedDoc {
            num: 1,
            nreg: "1556-18".into(),
            nazva: String::new(),
            status: String::new(),
            publics: String::new(),
            file: String::new(),
            size: None,
            link: String::new(),
        }];
        let (applied, failed) = apply_updates(&mut conn, &upd, degraded).unwrap();
        assert_eq!((applied, failed), (1, 0));

        let (dokid, nazva, status, org_num): (i64, String, i64, String) = conn
            .query_row(
                "SELECT dokid, nazva, status, org_num FROM docs WHERE nreg = '1556-18'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(dokid, 4242, "dokid must survive a degraded sync");
        assert_eq!(nazva, "Про вищу освіту", "nazva must survive");
        assert_eq!(org_num, "1556-VIII", "org_num must survive");
        assert_eq!(status, 43, "changed fields must still update");
    }

    /// Arrivals snapshot round-trips through `meta`.
    #[test]
    fn arrivals_snapshot_roundtrip() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        assert!(crate::sync::load_arrivals(&conn).is_none(), "empty at first");
        let docs = vec![UpdatedDoc {
            num: 7,
            nreg: "123-26".into(),
            nazva: "Про тест".into(),
            status: "Чинний".into(),
            publics: "ВР, 2026, №1".into(),
            file: "123-26.doc.txt".into(),
            size: Some(1234),
            link: "https://zakon.rada.gov.ua/go/123-26/ed20260101".into(),
        }];
        crate::sync::save_arrivals(&conn, &docs).unwrap();
        let (got, at) = crate::sync::load_arrivals(&conn).expect("saved");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].nreg, "123-26");
        assert_eq!(got[0].nazva, "Про тест");
        assert!(!at.is_empty(), "save stamp recorded");
        // corrupt JSON is tolerated (None), not a panic.
        crate::set_meta(&conn, "arrivals", "{oops").unwrap();
        assert!(crate::sync::load_arrivals(&conn).is_none());
    }

    /// A new document (not in the index) is inserted normally.
    #[test]
    fn apply_updates_inserts_new_doc() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        let upd = vec![UpdatedDoc {
            num: 1,
            nreg: "9999-26".into(),
            nazva: String::new(),
            status: String::new(),
            publics: String::new(),
            file: String::new(),
            size: None,
            link: String::new(),
        }];
        let (applied, failed) = apply_updates(&mut conn, &upd, |_| Ok(doc("9999-26", 1))).unwrap();
        assert_eq!((applied, failed), (1, 0));
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM docs WHERE nreg = '9999-26'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }
}
