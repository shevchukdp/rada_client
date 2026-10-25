//! Data models mirroring the "Законодавство України" database.
//!
//! Field semantics come from the official structure files
//! (`/ogd/zak/stru/doc-stru.csv`, `/ogd/zak/laws/data/csv/`) and the
//! `doc.txt` full dump. Verified against live `card/{nreg}.json` responses.

use serde::{Deserialize, Serialize};

/// Document status (dictionary `#stan`).
/// Values verified from `/ogd/zak/laws/data/csv/stan.txt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StatusId(pub i64);

impl StatusId {
    pub const NOT_SET: StatusId = StatusId(0);
    pub const LOST_FORCE: StatusId = StatusId(1);
    pub const ENTERING_FORCE: StatusId = StatusId(2);
    pub const SUSPENDED: StatusId = StatusId(3);
    pub const RESTORED: StatusId = StatusId(4);
    pub const IN_FORCE: StatusId = StatusId(5);
    pub const NOT_ENTERED: StatusId = StatusId(6);
    pub const NOT_APPLICABLE: StatusId = StatusId(7);
    pub const PARTIALLY_ENTERED: StatusId = StatusId(8);
    pub const LOST_PARTIALLY: StatusId = StatusId(9);

    /// Human-readable label (dictionary `stan.txt`).
    pub const fn label(self) -> &'static str {
        match self.0 {
            0 => "Не визначено",
            1 => "Втратив чинність",
            2 => "Набирає чинності",
            3 => "Дію зупинено",
            4 => "Дію відновлено",
            5 => "Чинний",
            6 => "Не набрав чинності",
            7 => "Не застосовується на території України",
            8 => "Частково набрав чинності",
            9 => "Втратив чинність крім окремих положень",
            _ => "Невідомий стан",
        }
    }
}

/// A single publisher/org of a document with its date and number.
/// Wire format in `doc.txt`: `orgid:orgdat:orgnum` (multiple via `|`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgCard {
    /// Publisher id (dictionary `#orgs` / `organs.txt`).
    pub org_id: i64,
    /// Adoption date as `YYYYMMDD` (empty when absent).
    pub org_date: String,
    /// Document number, e.g. `2041-IV` (empty when absent).
    pub org_num: String,
}

/// One row of the `doc.txt` full dump / a `docs` entry of the index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocCard {
    /// Internal numeric id (`dokid`), primary key of the index.
    pub dokid: i64,
    /// System number, e.g. `2041-15`, `254к/96-вр`, `1178-2022-п`.
    /// The identifier used in all API URLs.
    pub nreg: String,
    /// Document title.
    pub nazva: String,
    /// Status (see [`StatusId`]).
    pub status: i64,
    /// Document types (`|`-separated ids, dictionary `#typs` / `typs.txt`).
    pub types: String,
    /// Publisher/org card(s), raw `orgid:orgdat:orgnum[|...]`.
    pub organs: String,
    /// Ministry-of-Justice registration, raw or empty.
    pub minjust: String,
    /// Image number for list display (almost always 0 or 1).
    pub npix: i64,
    /// Date of the current edition as `YYYYMMDD`.
    pub poddat: String,
}

impl DocCard {
    /// Parse one `doc.txt` line (Windows-1251 already decoded to UTF-8).
    ///
    /// Columns: `dokid \t nreg \t nazva \t status \t types \t organs \t
    /// minjust \t npix \t poddat` — `nazva` may itself contain tabs, so the
    /// trailing 6 fixed columns are taken from the right.
    pub fn from_tsv_line(line: &str) -> Option<DocCard> {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 9 {
            return None;
        }
        let dokid = f[0].parse().ok()?;
        let nreg = f[1].to_string();
        if nreg.is_empty() {
            return None;
        }
        // nazva = everything between the 2nd and the (len-6)th column
        let nazva = f[2..f.len() - 6].join("\t");
        let status = f[f.len() - 6].parse().ok().unwrap_or(0);
        let types = f[f.len() - 5].to_string();
        let organs = f[f.len() - 4].to_string();
        let minjust = f[f.len() - 3].to_string();
        let npix = f[f.len() - 2].parse().ok().unwrap_or(0);
        let poddat = f[f.len() - 1].to_string();
        Some(DocCard {
            dokid,
            nreg,
            nazva,
            status,
            types,
            organs,
            minjust,
            npix,
            poddat,
        })
    }

    /// Split `organs` into cards. Format: `orgid:orgdat:orgnum` repeated via `|`.
    pub fn org_cards(&self) -> Vec<OrgCard> {
        self.organs
            .split('|')
            .filter(|s| !s.is_empty())
            .map(|s| {
                let p: Vec<&str> = s.splitn(3, ':').collect();
                OrgCard {
                    org_id: p.first().and_then(|x| x.parse().ok()).unwrap_or(0),
                    org_date: p.get(1).copied().unwrap_or("").to_string(),
                    org_num: p.get(2).copied().unwrap_or("").to_string(),
                }
            })
            .collect()
    }

    /// Primary publisher (first card).
    pub fn primary_org(&self) -> Option<OrgCard> {
        self.org_cards().into_iter().next()
    }

    /// Primary adoption date (`YYYYMMDD`) from the first org card.
    pub fn adoption_date(&self) -> Option<String> {
        self.primary_org().map(|o| o.org_date).filter(|s| !s.is_empty())
    }

    /// Primary document number (e.g. `2041-IV`).
    pub fn doc_number(&self) -> Option<String> {
        self.primary_org().map(|o| o.org_num).filter(|s| !s.is_empty())
    }
}

/// One edition of a document (from `card/{nreg}.json` → `eds`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Edition {
    /// Edition date `YYYYMMDD`.
    pub datred: i64,
    /// 1 = main, 4 = ... (format id); kept as-is.
    pub format: Option<i64>,
    /// Page count.
    pub pages: Option<i64>,
    /// Body size in bytes (useful to plan bulk downloads).
    pub size: Option<i64>,
    /// Number of the document that introduced this edition (may be empty).
    pub pidstava: Option<String>,
}

/// Deserialize `types` whether the server sends a JSON number or a
/// `|`-separated string.
fn de_types<'de, D>(d: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(d)?;
    Ok(match v {
        serde_json::Value::String(s) => s,
        other => other.to_string(),
    })
}

/// Parsed `card/{nreg}.json` response (partial; only fields the UI needs).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LawCardJson {
    pub nreg: String,
    pub nazva: String,
    pub status: i64,
    /// Adoption date `YYYYMMDD`.
    pub orgdat: Option<i64>,
    /// Document number.
    pub orgnum: Option<String>,
    /// Edition date of the current text.
    pub poddat: Option<i64>,
    /// Date the current edition entered force (may be in the future).
    pub pridat: Option<i64>,
    /// Basis document that introduced the current edition.
    pub pidstava: Option<String>,
    /// Edition list.
    pub eds: Vec<Edition>,
    /// Minjust registration (raw string).
    pub minjust: Option<String>,
    /// Document types, raw `|`-separated (may arrive as a number or a string).
    #[serde(default, deserialize_with = "de_types")]
    pub types: String,
    /// Publisher id.
    #[serde(default)]
    pub org: i64,
    /// Body size of the current edition in bytes.
    pub size: Option<i64>,
    /// Archive flag.
    pub is_archive: Option<bool>,
}

impl Default for LawCardJson {
    fn default() -> Self {
        Self {
            nreg: String::new(),
            nazva: String::new(),
            status: 0,
            orgdat: None,
            orgnum: None,
            poddat: None,
            pridat: None,
            pidstava: None,
            eds: Vec::new(),
            minjust: None,
            types: String::new(),
            org: 0,
            size: None,
            is_archive: None,
        }
    }
}

/// One entry of the updated-documents feed (`/laws/main/r.tsv`).
/// Header: `num \t card \t nazva \t status \t publics \t link \t size \t file`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatedDoc {
    /// Row number in the feed.
    pub num: i64,
    /// nreg (system number).
    pub nreg: String,
    /// Title.
    pub nazva: String,
    /// Status label (text).
    pub status: String,
    /// Publication info (text).
    pub publics: String,
    /// Link to the current edition, e.g.
    /// `https://zakon.rada.gov.ua/go/1178-2022-п/ed20241115`.
    pub link: String,
    /// Body size in bytes.
    pub size: Option<i64>,
    /// File name like `d557982.htm` (dokid-based).
    pub file: String,
}

impl UpdatedDoc {
    pub fn from_tsv_line(line: &str) -> Option<Self> {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 8 {
            return None;
        }
        let num = f[0].trim_end_matches('.').parse().ok()?;
        // Column 2 is the nreg (redundant with the `link` column).
        let nreg = f[1].to_string();
        if nreg.is_empty() {
            return None;
        }
        let nazva = f[2].to_string();
        let status = f[3].to_string();
        let publics = f[4].to_string();
        let link = f[5].to_string();
        let size = f[6].parse().ok();
        let file = f[7].to_string();
        Some(UpdatedDoc {
            num,
            nreg,
            nazva,
            status,
            publics,
            link,
            size,
            file,
        })
    }
}

// Note: search types (`SearchQuery` / `SearchHit`) live in the `rada-db`
// crate, which owns the local index; they are not duplicated here.
