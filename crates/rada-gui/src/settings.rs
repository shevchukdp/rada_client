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
    s
}

fn render(s: &Settings) -> String {
    let clean = |v: &str| v.replace(['\r', '\n', '\t'], " ");
    let mut out = format!(
        "last_nreg={}\nlast_title={}\nlast_query={}\ntheme={}\n",
        clean(&s.last_nreg),
        clean(&s.last_title),
        clean(&s.last_query),
        clean(&s.theme),
    );
    for (n, t) in &s.recents {
        out.push_str(&format!("recent={}\t{}\n", clean(n), clean(t)));
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
