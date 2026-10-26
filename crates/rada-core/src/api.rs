//! Minimal `data.rada.gov.ua` client.
//!
//! Design notes (see crate docs for verified facts):
//! - All requests use `User-Agent: OpenData` (the only accepted anonymous UA).
//! - GZIP is enabled: the server sends gzip even without `Accept-Encoding`.
//! - A client-wide limiter enforces "≤60 req/min, 5–7 s pause recommended".
//!   Default: minimum 5 s between requests (configurable; bulk tools lower it
//!   carefully and raise it on 429).

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::models::{LawCardJson, UpdatedDoc};
use crate::{DATA_BASE, UA_OPENDATA, ZAKON_BASE};

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("HTTP {status} for {url}")]
    Http { status: u16, url: String },
    #[error("network: {0}")]
    Network(#[from] reqwest::Error),
    #[error("parse: {0}")]
    Parse(String),
}

pub struct RadaClient {
    http: reqwest::blocking::Client,
    /// Same as `http` but with redirects disabled, used to observe the
    /// `find/a` 302 `Location` directly.
    http_nofollow: reqwest::blocking::Client,
    /// Minimum delay between two outgoing requests (server asks for 5–7 s).
    min_interval: Mutex<Duration>,
    last_request: Mutex<Option<Instant>>,
    /// Total outgoing requests (for the 100k/day budget).
    pub requests_sent: std::sync::atomic::AtomicU64,
}

impl RadaClient {
    /// Build a client with the recommended conservative pacing.
    pub fn new() -> Self {
        Self::with_pacing(Duration::from_secs(5))
    }

    pub fn with_pacing(min_interval: Duration) -> Self {
        let common = || {
            reqwest::blocking::Client::builder()
                .user_agent(UA_OPENDATA)
                .gzip(true)
                .timeout(Duration::from_secs(120))
                .connect_timeout(Duration::from_secs(20))
        };
        let http = common().build().expect("client");
        let http_nofollow = common()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("client");
        Self {
            http,
            http_nofollow,
            min_interval: Mutex::new(min_interval),
            last_request: Mutex::new(None),
            requests_sent: std::sync::atomic::AtomicU64::new(0),
        }
    }

    fn pace(&self) {
        let mut last = self.last_request.lock().unwrap();
        let interval = *self.min_interval.lock().unwrap();
        if let Some(t) = *last {
            let elapsed = t.elapsed();
            if elapsed < interval {
                std::thread::sleep(interval - elapsed);
            }
        }
        *last = Some(Instant::now());
        self.requests_sent
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    fn get(&self, url: &str) -> Result<String, ApiError> {
        self.pace();
        let resp = self
            .http
            .get(url)
            .send()
            .map_err(ApiError::Network)?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            return Err(ApiError::Http {
                status,
                url: url.to_string(),
            });
        }
        resp.text().map_err(ApiError::Network)
    }

    fn get_bytes(&self, url: &str) -> Result<Vec<u8>, ApiError> {
        self.pace();
        let resp = self
            .http
            .get(url)
            .send()
            .map_err(ApiError::Network)?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            return Err(ApiError::Http {
                status,
                url: url.to_string(),
            });
        }
        resp.bytes().map(|b| b.to_vec()).map_err(ApiError::Network)
    }

    /// Percent-encode a path segment, leaving `/` intact.
    /// Verified: `254к/96-вр` → `254%D0%BA/96-%D0%B2%D1%80`; encoding the
    /// slash as `%2F` yields 404.
    pub fn encode_segment(seg: &str) -> String {
        let mut out = String::with_capacity(seg.len());
        for b in seg.as_bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'/' | b'~' => {
                    out.push(*b as char)
                }
                _ => out.push_str(&format!("%{b:02X}")),
            }
        }
        out
    }

    fn doc_path(nreg: &str) -> String {
        // nreg may contain several `/`-separated segments (e.g. `254к/96-вр`);
        // encode each segment, keep slashes.
        nreg.split('/')
            .map(Self::encode_segment)
            .collect::<Vec<_>>()
            .join("/")
    }

    // ------------------------------------------------------------------
    // Endpoints (verified live, October 2026)
    // ------------------------------------------------------------------

    /// JSON card of a document: `{base}/laws/card/{nreg}.json`.
    pub fn card(&self, nreg: &str) -> Result<LawCardJson, ApiError> {
        let url = format!("{DATA_BASE}/laws/card/{}.json", Self::doc_path(nreg));
        let body = self.get(&url)?;
        serde_json::from_str(&body).map_err(|e| ApiError::Parse(e.to_string()))
    }

    /// Full HTML of the current edition: `/laws/show/{nreg}`.
    pub fn show_html(&self, nreg: &str) -> Result<String, ApiError> {
        let url = format!("{DATA_BASE}/laws/show/{}", Self::doc_path(nreg));
        self.get(&url)
    }

    /// Clean plain text of the current edition: `/laws/show/{nreg}.txt`.
    /// Verified: 200, UTF-8, no HTML.
    pub fn show_text(&self, nreg: &str) -> Result<String, ApiError> {
        let url = format!("{DATA_BASE}/laws/show/{}.txt", Self::doc_path(nreg));
        self.get(&url)
    }

    /// HTML of a specific edition: `/laws/show/{nreg}/ed{YYYYMMDD}`.
    pub fn show_edition_html(&self, nreg: &str, ed: &str) -> Result<String, ApiError> {
        let url = format!("{DATA_BASE}/laws/show/{}/ed{}", Self::doc_path(nreg), ed);
        self.get(&url)
    }

    /// Clean text of a specific edition.
    pub fn show_edition_text(&self, nreg: &str, ed: &str) -> Result<String, ApiError> {
        let url = format!("{DATA_BASE}/laws/show/{}/ed{}.txt", Self::doc_path(nreg), ed);
        self.get(&url)
    }

    /// Feed of recently updated documents (TSV): `/laws/main/r.tsv`.
    /// First line is the header; remaining lines are [`UpdatedDoc`].
    pub fn updated_docs(&self) -> Result<Vec<UpdatedDoc>, ApiError> {
        let body = self.get(&format!("{DATA_BASE}/laws/main/r.tsv"))?;
        Ok(body
            .lines()
            .skip(1)
            .filter_map(UpdatedDoc::from_tsv_line)
            .collect())
    }

    /// Full `doc.txt` dump (Windows-1251). ~51 MB.
    /// Callers are responsible for pacing/progress; this blocks.
    pub fn full_dump(&self) -> Result<Vec<u8>, ApiError> {
        self.get_bytes(&format!(
            "{DATA_BASE}/ogd/zak/laws/data/csv/doc.txt"
        ))
    }

    /// One dictionary file, e.g. `stan`, `typs`, `organs` (Windows-1251).
    /// The server publishes the types dictionary as `typ.txt` while the
    /// local table kind is called `typs`.
    pub fn dictionary(&self, name: &str) -> Result<Vec<u8>, ApiError> {
        let file = if name == "typs" { "typ" } else { name };
        self.get_bytes(&format!(
            "{DATA_BASE}/ogd/zak/laws/data/csv/{file}.txt"
        ))
    }

    /// Name → nreg resolver: `GET /laws/find/a?text=...`.
    /// Returns the nreg when the query is unambiguous — the server replies
    /// `302` with a `Location` like `/laws/main/2755-17` or `/laws/show/...`.
    /// Returns `None` when the server returns an HTML candidate list instead
    /// of a redirect (ambiguous query).
    pub fn resolve_name(&self, text: &str) -> Option<String> {
        self.pace();
        let url = format!(
            "{DATA_BASE}/laws/find/a?text={}",
            Self::encode_segment(text)
        );
        let req = self
            .http_nofollow
            .get(&url)
            .build()
            .ok()?;
        let resp = self.http_nofollow.execute(req).ok()?;
        if resp.status().is_redirection() {
            let loc = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())?;
            Self::nreg_from_location(loc)
        } else {
            None
        }
    }

    /// Extract an nreg from a `find/a` `Location` header such as
    /// `/laws/main/2755-17` or `/laws/show/254к/96-вр[/ed...]`.
    fn nreg_from_location(loc: &str) -> Option<String> {
        let path = loc.split('?').next().unwrap_or(loc).trim_end_matches('/');
        // .../laws/main/<nreg>  or  .../laws/show/<nreg>
        let rest = path
            .rsplit_once("/laws/main/")
            .or_else(|| path.rsplit_once("/laws/show/"))
            .map(|(_, r)| r)?;
        // strip a trailing /edYYYYMMDD
        let nreg = rest.split("/ed").next().unwrap_or(rest).trim();
        if nreg.is_empty() {
            None
        } else {
            Some(nreg.to_string())
        }
    }

    /// Public site URL for a document (for "open in browser" links).
    pub fn site_url(nreg: &str) -> String {
        format!("{ZAKON_BASE}/go/{}", Self::doc_path(nreg))
    }
}

impl Default for RadaClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_cyrillic_but_keeps_slash() {
        assert_eq!(
            RadaClient::encode_segment("254к/96-вр"),
            "254%D0%BA/96-%D0%B2%D1%80"
        );
        assert_eq!(RadaClient::encode_segment("2041-15"), "2041-15");
    }

    #[test]
    fn doc_path_joins_encoded_segments() {
        assert_eq!(RadaClient::doc_path("254к/96-вр"), "254%D0%BA/96-%D0%B2%D1%80");
        assert_eq!(RadaClient::doc_path("1178-2022-п"), "1178-2022-%D0%BF");
    }

    #[test]
    fn extracts_nreg_from_find_locations() {
        // Live-verified shape: /laws/main/<nreg>
        assert_eq!(
            RadaClient::nreg_from_location("/laws/main/2755-17"),
            Some("2755-17".to_string())
        );
        // /laws/show/<nreg>
        assert_eq!(
            RadaClient::nreg_from_location("/laws/show/2341-14"),
            Some("2341-14".to_string())
        );
        // Trailing edition stripped.
        assert_eq!(
            RadaClient::nreg_from_location("/laws/show/1178-2022-п/ed20241115"),
            Some("1178-2022-п".to_string())
        );
        // Nothing recognizable → None.
        assert_eq!(RadaClient::nreg_from_location("/laws/find/list"), None);
    }
}
