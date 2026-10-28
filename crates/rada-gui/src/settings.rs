//! Tiny key=value settings file (%LOCALAPPDATA%\rada-client\settings.ini).
//! Stores the last opened document and the last search query so that
//! "Continue" can offer them on the next launch. Best-effort: every IO
//! error is silently tolerated (the app must work without write access).

use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Settings {
    pub last_nreg: String,
    pub last_title: String,
    pub last_query: String,
    /// Recently opened documents, newest first: `(nreg, title)`, max 10.
    pub recents: Vec<(String, String)>,
    /// Color theme id (see `themes::THEMES`); empty = "classical".
    pub theme: String,
    /// Reader body-text zoom in percent (0 = default 100%).
    pub zoom: u32,
    /// Plain (unformatted) reader text: disables structural styling of
    /// titles/sections/articles (default false = formatted).
    pub raw: bool,
    /// Last folder used by save/open dialogs (remembered across sessions).
    pub export_dir: String,
    /// Live-request pause in seconds (0 = not configured, use CLI/default).
    pub pace: u64,
    /// Recent search queries, newest first, max 10.
    pub recent_q: Vec<String>,
    /// Arrivals-feed nregs the user has already seen (the «нові» badges and
    /// the «лише нові» filter work off this set). Cap 500.
    pub arr_seen: Vec<String>,
    /// Arrivals page text filter (persisted so a domain filter survives
    /// restarts).
    pub arr_filter: String,
    /// Chosen search sort as the stable id from `rada_db::search::Sort::as_str()`
    /// («relevance» / «date_desc» / …); empty = default. Parsed leniently.
    pub sort: String,
    /// Last searched-with dictionary filters (0 = unset), restored on launch
    /// so a saved domain filter (e.g. «чинний» + «Міністерство юстиції»)
    /// survives restarts.
    pub f_status: i64,
    pub f_typ: i64,
    pub f_org: i64,
    /// Publisher label belonging to `f_org` (shown on the active-filter chip
    /// before the dictionaries are loaded).
    pub f_org_label: String,
}

impl Settings {
    /// Theme id with the legacy-empty fallback.
    pub fn theme_id(&self) -> &str {
        if self.theme.is_empty() {
            "classical"
        } else {
            &self.theme
        }
    }

    /// Reader text scale factor derived from `zoom` (clamped 60%..250%).
    pub fn text_scale(&self) -> f32 {
        if self.zoom == 0 {
            1.0
        } else {
            (self.zoom as f32 / 100.0).clamp(0.6, 2.5)
        }
    }
}

/// Push a document to the front of the recent list (dedup by nreg, cap 10)
/// and refresh the "last opened" fields.
pub fn remember(s: &mut Settings, nreg: &str, title: &str) {
    if nreg.trim().is_empty() {
        return;
    }
    s.last_nreg = nreg.to_string();
    s.last_title = title.to_string();
    s.recents.retain(|(n, _)| n != nreg);
    s.recents.insert(0, (nreg.to_string(), title.to_string()));
    s.recents.truncate(10);
}

/// Push a query to the front of the recent-queries list (dedup
/// case-insensitively, cap 10) and refresh the "last query" field.
pub fn remember_query(s: &mut Settings, q: &str) {
    let q = q.trim();
    if q.is_empty() {
        return;
    }
    s.last_query = q.to_string();
    let low = q.to_lowercase();
    s.recent_q.retain(|x| x.to_lowercase() != low);
    s.recent_q.insert(0, q.to_string());
    s.recent_q.truncate(10);
}

/// Merge arrivals-feed nregs into the seen set (dedup, cap 500, oldest
/// entries dropped first).
pub fn mark_arrivals_seen<I: IntoIterator<Item = String>>(s: &mut Settings, nregs: I) {
    for n in nregs {
        let n = n.trim().to_string();
        if !n.is_empty() && !s.arr_seen.contains(&n) {
            s.arr_seen.push(n);
        }
    }
    if s.arr_seen.len() > 500 {
        let extra = s.arr_seen.len() - 500;
        s.arr_seen.drain(0..extra);
    }
}

pub fn path() -> Option<PathBuf> {
    let base = std::env::var("LOCALAPPDATA").ok()?;
    Some(PathBuf::from(base).join("rada-client").join("settings.ini"))
}

pub fn load() -> Settings {
    match path().and_then(|p| std::fs::read_to_string(p).ok()) {
        Some(text) => parse(&text),
        None => Settings::default(),
    }
}

pub fn save(s: &Settings) {
    let Some(p) = path() else { return };
    let _ = save_to(&p, s);
}

pub fn save_to(p: &Path, s: &Settings) -> std::io::Result<()> {
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(p, render(s))
}

fn parse(text: &str) -> Settings {
    let mut s = Settings::default();
    for line in text.lines() {
        if let Some((k, v)) = line.split_once('=') {
            match k.trim() {
                "last_nreg" => s.last_nreg = v.trim().to_string(),
                "last_title" => s.last_title = v.trim().to_string(),
                "last_query" => s.last_query = v.trim().to_string(),
                "theme" => s.theme = v.trim().to_string(),
                "zoom" => s.zoom = v.trim().parse().unwrap_or(0),
                "raw" => s.raw = v.trim() == "1",
                "export_dir" => s.export_dir = v.trim().to_string(),
                "pace" => s.pace = v.trim().parse().unwrap_or(0),
                "recentq" => {
                    let q = v.trim().to_string();
                    if !q.is_empty() {
                        s.recent_q.push(q);
                    }
                }
                "arrseen" => {
                    let n = v.trim().to_string();
                    if !n.is_empty() {
                        s.arr_seen.push(n);
                    }
                }
                "arrfilter" => s.arr_filter = v.trim().to_string(),
                "sort" => s.sort = v.trim().to_string(),
                "status" => s.f_status = v.trim().parse().unwrap_or(0),
                "typ" => s.f_typ = v.trim().parse().unwrap_or(0),
                "org" => s.f_org = v.trim().parse().unwrap_or(0),
                "orglabel" => s.f_org_label = v.trim().to_string(),
                "recent" => {
                    if let Some((n, t)) = v.split_once('\t') {
                        s.recents.push((n.to_string(), t.to_string()));
                    }
                }
                _ => {}
            }
        }
    }
    // settings from v0.4 (only a "last" pair): seed the list from it
    if s.recents.is_empty() && !s.last_nreg.is_empty() {
        s.recents.push((s.last_nreg.clone(), s.last_title.clone()));
    }
    // same for the query history: seed from the single remembered query
    if s.recent_q.is_empty() && !s.last_query.trim().is_empty() {
        s.recent_q.push(s.last_query.trim().to_string());
    }
    s
}

fn render(s: &Settings) -> String {
    let clean = |v: &str| v.replace(['\r', '\n', '\t'], " ");
    let mut out = format!(
        "last_nreg={}\nlast_title={}\nlast_query={}\ntheme={}\nzoom={}\nraw={}\nexport_dir={}\npace={}\narrfilter={}\nsort={}\nstatus={}\ntyp={}\norg={}\norglabel={}\n",
        clean(&s.last_nreg),
        clean(&s.last_title),
        clean(&s.last_query),
        clean(&s.theme),
        s.zoom,
        if s.raw { 1 } else { 0 },
        clean(&s.export_dir),
        s.pace,
        clean(&s.arr_filter),
        clean(&s.sort),
        s.f_status,
        s.f_typ,
        s.f_org,
        clean(&s.f_org_label),
    );
    for (n, t) in &s.recents {
        out.push_str(&format!("recent={}\t{}\n", clean(n), clean(t)));
    }
    for q in &s.recent_q {
        out.push_str(&format!("recentq={}\n", clean(q)));
    }
    for n in &s.arr_seen {
        out.push_str(&format!("arrseen={}\n", clean(n)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let mut s = Settings {
            last_nreg: "2041-15".into(),
            last_title: "Про проведення слухань".into(),
            last_query: "податковий кодекс".into(),
            recents: vec![
                ("2041-15".into(), "Про проведення слухань".into()),
                ("16-15".into(), "Про доступ до публічної інформації".into()),
            ],
            theme: "matrix".into(),
            zoom: 130,
            raw: true,
            export_dir: "C:\\temp".into(),
            pace: 7,
            recent_q: vec!["податковий кодекс".into(), "лік".into()],
            arr_seen: vec!["1556-18".into(), "16-15".into()],
            arr_filter: "податков".into(),
            sort: "nazva_desc".into(),
            f_status: 5,
            f_typ: 12,
            f_org: 45,
            f_org_label: "Міністерство юстиції".into(),
        };
        let p = std::env::temp_dir().join("rada-settings-test.ini");
        save_to(&p, &s).unwrap();
        let got = parse(&std::fs::read_to_string(&p).unwrap());
        assert_eq!(got, s);
        // values with newlines collapse into spaces
        s.last_query = "row1\nrow2".into();
        let got2 = parse(&render(&s));
        assert_eq!(got2.last_query, "row1 row2");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn sort_key_defaults_empty_and_roundtrips() {
        // absent key = empty = the Sort default is used by the app
        assert_eq!(parse("last_nreg=x\n").sort, "");
        // a stored id comes back verbatim and parses to a stable Sort
        let s = parse("sort=podd_desc\n");
        assert_eq!(s.sort, "podd_desc");
        assert_eq!(
            rada_db::search::Sort::parse(&s.sort),
            rada_db::search::Sort::PoddDesc
        );
        // garbage falls back leniently to the default ordering
        assert_eq!(
            rada_db::search::Sort::parse(&parse("sort=???!\n").sort),
            rada_db::search::Sort::default()
        );
        // render→parse keeps the value
        let mut s = Settings::default();
        s.sort = rada_db::search::Sort::NazvaAsc.as_str().to_string();
        assert_eq!(parse(&render(&s)).sort, "nazva_asc");
    }

    #[test]
    fn filter_ids_roundtrip_and_garbage_defaults_zero() {
        let mut s = Settings::default();
        s.f_status = 5;
        s.f_typ = 0;
        s.f_org = 45;
        s.f_org_label = "Кабінет Міністрів".into();
        let got = parse(&render(&s));
        assert_eq!((got.f_status, got.f_typ, got.f_org), (5, 0, 45));
        assert_eq!(got.f_org_label, "Кабінет Міністрів");
        // garbage ids fall back to «unset»
        let g = parse("status=abc\ntyp=12x\norg=\n");
        assert_eq!((g.f_status, g.f_typ, g.f_org), (0, 0, 0));
    }

    #[test]
    fn arr_seen_merge_dedups_and_caps() {
        let mut s = Settings::default();
        mark_arrivals_seen(&mut s, (0..512).map(|i| format!("{i}-1")));
        assert_eq!(s.arr_seen.len(), 500, "cap 500, oldest dropped");
        assert_eq!(s.arr_seen[499], "511-1");
        // dedup keeps the first occurrence, empties ignored
        mark_arrivals_seen(&mut s, vec!["511-1".into(), String::new(), "  ".into(), "x-1".into()]);
        assert_eq!(s.arr_seen.len(), 500, "cap enforced per merge");
        assert_eq!(s.arr_seen[498], "511-1", "re-adding does not move it");
        assert_eq!(s.arr_seen[499], "x-1");
        // round-trip through the ini format
        let p = std::env::temp_dir().join("rada-settings-arrseen.ini");
        save_to(&p, &s).unwrap();
        let got = parse(&std::fs::read_to_string(&p).unwrap());
        assert_eq!(got.arr_seen, s.arr_seen);
    }

    #[test]
    fn raw_toggle_roundtrip() {        assert!(!parse("").raw, "formatted is the default");
        assert!(parse("raw=1\n").raw);
        assert!(!parse("raw=0\n").raw);
        let s = parse("raw=1\n");
        assert_eq!(parse(&render(&s)).raw, true);
    }

    #[test]
    fn export_dir_roundtrip() {
        assert_eq!(parse("").export_dir, "");
        let s = parse("export_dir=C:\\Users\\me\\Documents\\законодавство\n");
        assert_eq!(s.export_dir, "C:\\Users\\me\\Documents\\законодавство");
        assert_eq!(parse(&render(&s)).export_dir, s.export_dir);
    }

    #[test]
    fn pace_roundtrip() {
        assert_eq!(parse("").pace, 0, "unset by default");
        assert_eq!(parse("pace=7\n").pace, 7);
        assert_eq!(parse("pace=abc\n").pace, 0, "garbage falls back to unset");
        let s = parse("pace=9\n");
        assert_eq!(parse(&render(&s)).pace, 9);
    }

    #[test]
    fn recent_query_history() {
        // dedup is case-insensitive, newest first, cap 10.
        let mut s = Settings::default();
        for i in 0..12 {
            remember_query(&mut s, &format!("запит {i}"));
        }
        remember_query(&mut s, "ЗАПИТ 11");
        assert_eq!(s.recent_q.len(), 10);
        assert_eq!(s.recent_q[0], "ЗАПИТ 11", "case-insensitive dedup keeps the newest form");
        assert_eq!(s.last_query, "ЗАПИТ 11");
        // round-trip through the ini file format
        let p = std::env::temp_dir().join("rada-settings-recentq.ini");
        save_to(&p, &s).unwrap();
        let got = parse(&std::fs::read_to_string(&p).unwrap());
        assert_eq!(got.recent_q, s.recent_q);
        // empty queries are ignored; legacy single-query settings seed the list
        remember_query(&mut s, "   ");
        assert_eq!(s.recent_q.len(), 10);
        let legacy = parse("last_query=податковий кодекс\n");
        assert_eq!(legacy.recent_q, vec!["податковий кодекс".to_string()]);
    }

    #[test]
    fn missing_file_is_default() {
        assert_eq!(parse(""), Settings::default());
        assert_eq!(parse("junk line\nlast_nreg=1-2\n").last_nreg, "1-2");
    }

    #[test]
    fn v04_settings_seed_one_recent() {
        let s = parse("last_nreg=2041-15\nlast_title=Title\nlast_query=q\n");
        assert_eq!(s.recents, vec![("2041-15".into(), "Title".into())]);
    }

    #[test]
    fn theme_key_roundtrip_and_fallback() {
        assert_eq!(parse("").theme_id(), "classical");
        assert_eq!(parse("theme=nord\n").theme_id(), "nord");
        let s = parse("theme=dracula\n");
        assert_eq!(parse(&render(&s)).theme, "dracula");
    }

    #[test]
    fn remember_dedupes_and_caps() {
        let mut s = Settings::default();
        for i in 0..12 {
            remember(&mut s, &format!("{i}-26"), "t");
        }
        assert_eq!(s.recents.len(), 10);
        assert_eq!(s.recents[0], ("11-26".into(), "t".into()));
        remember(&mut s, "5-26", "t2");
        assert_eq!(s.recents.len(), 10);
        assert_eq!(s.recents[0], ("5-26".into(), "t2".into()));
        assert_eq!(s.recents.iter().filter(|(n, _)| n == "5-26").count(), 1);
        assert_eq!(s.last_nreg, "5-26");
        assert_eq!(s.last_title, "t2");
    }
}
