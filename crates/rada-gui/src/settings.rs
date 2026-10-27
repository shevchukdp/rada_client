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
                _ => {}
            }
        }
    }
    s
}

fn render(s: &Settings) -> String {
    let clean = |v: &str| v.replace(['\r', '\n'], " ");
    format!(
        "last_nreg={}\nlast_title={}\nlast_query={}\n",
        clean(&s.last_nreg),
        clean(&s.last_title),
        clean(&s.last_query),
    )
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
}
