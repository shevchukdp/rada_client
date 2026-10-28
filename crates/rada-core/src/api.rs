//! Minimal `data.rada.gov.ua` client.
//!
//! Design notes (see crate docs for verified facts):
//! - All requests use `User-Agent: OpenData` (the only accepted anonymous UA).
//! - GZIP is enabled: the server sends gzip even without `Accept-Encoding`.
//! - A client-wide limiter enforces "≤60 req/min, 5–7 s pause recommended".
//!   Default: minimum 5 s between requests (configurable; bulk tools lower it
//!   carefully and raise it on 429).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::models::{LawCardJson, UpdatedDoc};
use crate::{DATA_BASE, UA_OPENDATA, ZAKON_BASE};

/// A cached response body plus the validators needed to revalidate it.
///
/// `body` stores the raw response bytes losslessly: each byte is kept as the
/// `char` with the same code point (U+0000..=U+00FF, i.e. Latin-1), so
/// non-UTF-8 payloads (the Windows-1251 dictionary dumps) round-trip through
/// the cache exactly via [`bytes_to_cached`]/[`cached_to_bytes`].
struct CachedRes {
    body: String,
    etag: Option<String>,
    last_modified: Option<String>,
}

/// Lossless raw-bytes → Latin-1 `String` mapping (byte value == code point).
fn bytes_to_cached(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

/// Inverse of [`bytes_to_cached`]; only ever applied to strings that
/// function produced, so every code point is ≤ 0xFF.
fn cached_to_bytes(s: &str) -> Vec<u8> {
    s.chars().map(|c| c as u8).collect()
}

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
    /// Runtime-adjustable via [`RadaClient::set_pace`].
    pub pace_secs: std::sync::atomic::AtomicU64,
    last_request: Mutex<Option<Instant>>,
    /// Total outgoing requests (for the 100k/day budget).
    pub requests_sent: std::sync::atomic::AtomicU64,
    /// Conditional-GET cache: full URL → body + validators. Shared by
    /// `dictionary`/`updated_docs` to avoid re-downloading unchanged files.
    cache: Mutex<HashMap<String, CachedRes>>,
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
            pace_secs: std::sync::atomic::AtomicU64::new(min_interval.as_secs()),
            last_request: Mutex::new(None),
            requests_sent: std::sync::atomic::AtomicU64::new(0),
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Change the minimum delay between outgoing requests at runtime
    /// (e.g. raise it after a 429, lower it for a bulk import).
    pub fn set_pace(&self, secs: u64) {
        self.pace_secs.store(secs, std::sync::atomic::Ordering::Relaxed);
    }

    /// Current minimum delay between outgoing requests.
    pub fn pace(&self) -> u64 {
        self.pace_secs.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Sleep until the pacing interval since the previous request has
    /// elapsed, then record this moment and count the request.
    fn throttle(&self) {
        let mut last = self.last_request.lock().unwrap();
        let interval =
            Duration::from_secs(self.pace_secs.load(std::sync::atomic::Ordering::Relaxed));
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
        self.throttle();
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
        self.throttle();
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

    /// GET with HTTP conditional-request caching (RFC 9110 §13.1–13.2),
    /// returning the raw response bytes.
    ///
    /// First call: plain GET; the body plus any `ETag`/`Last-Modified`
    /// response headers are stored per URL. Later calls revalidate with
    /// `If-None-Match`/`If-Modified-Since`; a `304 Not Modified` replays the
    /// stored body without re-downloading it (the request is still counted
    /// in `requests_sent`). A `200` refreshes the entry. Any other status or
    /// a network failure behaves exactly like [`RadaClient::get`]/
    /// [`RadaClient::get_bytes`].
    ///
    /// The cache mutex is never held across the network call: validators are
    /// cloned out under the lock, then the lock is re-acquired to store.
    fn get_conditional_bytes(&self, url: &str) -> Result<Vec<u8>, ApiError> {
        let cached = self
            .cache
            .lock()
            .unwrap()
            .get(url)
            .map(|c| (c.etag.clone(), c.last_modified.clone()));
        let (known_etag, known_lm) = match cached.as_ref() {
            Some((etag, last_modified)) => (etag.clone(), last_modified.clone()),
            None => (None, None),
        };

        self.throttle();
        let mut req = self.http.get(url);
        if let Some(etag) = known_etag.as_deref() {
            if let Ok(v) = reqwest::header::HeaderValue::from_str(etag) {
                req = req.header(reqwest::header::IF_NONE_MATCH, v);
            }
        }
        if let Some(lm) = known_lm.as_deref() {
            if let Ok(v) = reqwest::header::HeaderValue::from_str(lm) {
                req = req.header(reqwest::header::IF_MODIFIED_SINCE, v);
            }
        }
        let resp = req.send().map_err(ApiError::Network)?;
        let status = resp.status().as_u16();

        // 304 must not hit the error path: replay the stored body. (reqwest
        // surfaces 304 to us — `is_success()` is false for 3xx — and a 304
        // carries no body, so we never call `.bytes()` on it.)
        if status == 304 {
            if let Some(body) = self
                .cache
                .lock()
                .unwrap()
                .get(url)
                .map(|c| cached_to_bytes(&c.body))
            {
                return Ok(body);
            }
            // A 304 without a cache entry cannot be satisfied; fall through
            // to the error path below.
        }

        if !resp.status().is_success() {
            return Err(ApiError::Http {
                status,
                url: url.to_string(),
            });
        }
        let etag = resp
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let last_modified = resp
            .headers()
            .get(reqwest::header::LAST_MODIFIED)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let bytes = resp.bytes().map_err(ApiError::Network)?;
        self.cache.lock().unwrap().insert(
            url.to_string(),
            CachedRes {
                body: bytes_to_cached(&bytes),
                etag,
                last_modified,
            },
        );
        Ok(bytes.to_vec())
    }

    /// [`RadaClient::get_conditional_bytes`] decoded as UTF-8 (lossy), for
    /// the UTF-8 endpoints. Behaviorally equivalent to [`RadaClient::get`]
    /// for valid UTF-8 bodies.
    fn get_conditional(&self, url: &str) -> Result<String, ApiError> {
        let bytes = self.get_conditional_bytes(url)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
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

    /// «Пов'язані документи»: parse the document page for links to other acts
    /// (`/laws/show/…` and `go/…` hrefs), skipping `nreg` itself.
    /// One paced request; pure parsing lives in [`crate::related`].
    pub fn related(&self, nreg: &str) -> Result<Vec<crate::related::RelatedDoc>, ApiError> {
        let html = self.show_html(nreg)?;
        Ok(crate::related::extract_related(&html, nreg))
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
    /// Repeated calls revalidate via `ETag`/`Last-Modified` and re-download
    /// only when the feed actually changed.
    pub fn updated_docs(&self) -> Result<Vec<UpdatedDoc>, ApiError> {
        let body = self.get_conditional(&format!("{DATA_BASE}/laws/main/r.tsv"))?;
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
    /// Repeated calls revalidate via `ETag`/`Last-Modified` and re-download
    /// only when the file actually changed (these dumps are large and
    /// mostly static, so this saves most of the daily bandwidth).
    pub fn dictionary(&self, name: &str) -> Result<Vec<u8>, ApiError> {
        let file = if name == "typs" { "typ" } else { name };
        self.get_conditional_bytes(&format!(
            "{DATA_BASE}/ogd/zak/laws/data/csv/{file}.txt"
        ))
    }

    /// Name → nreg resolver: `GET /laws/find/a?text=...`.
    /// Returns the nreg when the query is unambiguous — the server replies
    /// `302` with a `Location` like `/laws/main/2755-17` or `/laws/show/...`.
    /// Returns `None` when the server returns an HTML candidate list instead
    /// of a redirect (ambiguous query).
    pub fn resolve_name(&self, text: &str) -> Option<String> {
        self.throttle();
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

    // ------------------------------------------------------------------
    // Conditional GET / pacing tests (local fake server, no live network)
    // ------------------------------------------------------------------

    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;

    const BODY: &str = "B1";
    const LM: &str = "Wed, 21 Oct 2015 07:28:00 GMT";

    #[derive(Clone, Copy)]
    enum FakeMode {
        /// First GET: 200 + `ETag: abc` + body. Requests carrying
        /// `If-None-Match: abc` get a bodiless 304.
        Etag,
        /// Same, using only `Last-Modified`/`If-Modified-Since`.
        LastModified,
        /// Always 500.
        Fail,
    }

    /// Tiny hand-rolled HTTP/1.1 responder on 127.0.0.1:0. Serves sequential
    /// requests per connection (so reqwest's pool can reuse it). Returns the
    /// base URL, all request header lines observed (lowercased), and a
    /// request counter.
    fn spawn_fake_server(mode: FakeMode) -> (String, Arc<Mutex<Vec<String>>>, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let seen = Arc::new(Mutex::new(Vec::new()));
        let count = Arc::new(AtomicUsize::new(0));
        let (seen_t, count_t) = (Arc::clone(&seen), Arc::clone(&count));
        std::thread::spawn(move || {
            for incoming in listener.incoming() {
                let Ok(mut stream) = incoming else { break };
                let seen = Arc::clone(&seen_t);
                let count = Arc::clone(&count_t);
                let mut buf: Vec<u8> = Vec::new();
                let mut chunk = [0u8; 1024];
                loop {
                    // Read until the end of the request headers.
                    let end = loop {
                        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                            break pos;
                        }
                        match stream.read(&mut chunk) {
                            Ok(0) | Err(_) => return,
                            Ok(n) => buf.extend_from_slice(&chunk[..n]),
                        }
                    };
                    let head = String::from_utf8_lossy(&buf[..end]).into_owned();
                    buf.drain(..end + 4);
                    let headers: Vec<String> =
                        head.lines().skip(1).map(|l| l.trim().to_ascii_lowercase()).collect();
                    seen.lock().unwrap().extend(headers.iter().cloned());
                    count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let resp = match mode {
                        FakeMode::Etag
                            if headers.iter().any(|h| h.as_str() == "if-none-match: abc") =>
                        {
                            "HTTP/1.1 304 Not Modified\r\n\r\n".to_string()
                        }
                        FakeMode::Etag => format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\
                             ETag: abc\r\nContent-Length: {}\r\n\r\n{BODY}",
                            BODY.len()
                        ),
                        FakeMode::LastModified
                            if headers.iter().any(|h| {
                                h.as_str()
                                    == format!("if-modified-since: {}", LM.to_ascii_lowercase())
                            }) =>
                        {
                            "HTTP/1.1 304 Not Modified\r\n\r\n".to_string()
                        }
                        FakeMode::LastModified => format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\
                             Last-Modified: {LM}\r\nContent-Length: {}\r\n\r\n{BODY}",
                            BODY.len()
                        ),
                        FakeMode::Fail => {
                            "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n"
                                .to_string()
                        }
                    };
                    if stream.write_all(resp.as_bytes()).is_err() {
                        return;
                    }
                    let _ = stream.flush();
                }
            }
        });
        (format!("http://{addr}"), seen, count)
    }

    #[test]
    fn conditional_caches_and_revalidates() {
        let (base, seen, count) = spawn_fake_server(FakeMode::Etag);
        let client = RadaClient::with_pacing(Duration::from_secs(0));
        let url = format!("{base}/dict/stan.txt");

        // First call: plain GET, body + ETag stored.
        assert_eq!(client.get_conditional(&url).unwrap(), BODY);
        // Second call: conditional GET → 304 → stored body replayed.
        assert_eq!(client.get_conditional(&url).unwrap(), BODY);

        let seen = seen.lock().unwrap();
        assert!(
            seen.iter().any(|h| h.as_str() == "if-none-match: abc"),
            "server never saw If-None-Match: {seen:?}"
        );
        assert_eq!(count.load(std::sync::atomic::Ordering::Relaxed), 2);
        // The 304 still counts as an outgoing request (requests_sent
        // semantics unchanged).
        assert_eq!(
            client.requests_sent.load(std::sync::atomic::Ordering::Relaxed),
            2
        );
    }

    #[test]
    fn not_modified_without_etag_uses_last_modified() {
        let (base, seen, count) = spawn_fake_server(FakeMode::LastModified);
        let client = RadaClient::with_pacing(Duration::from_secs(0));
        let url = format!("{base}/laws/main/r.tsv");

        assert_eq!(client.get_conditional(&url).unwrap(), BODY);
        assert_eq!(client.get_conditional(&url).unwrap(), BODY);

        let seen = seen.lock().unwrap();
        let want = format!("if-modified-since: {}", LM.to_ascii_lowercase());
        assert!(
            seen.iter().any(|h| h.as_str() == want.as_str()),
            "server never saw If-Modified-Since: {seen:?}"
        );
        assert!(
            !seen.iter().any(|h| h.starts_with("if-none-match")),
            "no ETag was sent, so no If-None-Match may appear: {seen:?}"
        );
        assert_eq!(count.load(std::sync::atomic::Ordering::Relaxed), 2);
    }

    #[test]
    fn plain_error_still_fails() {
        let (base, _seen, _count) = spawn_fake_server(FakeMode::Fail);
        let client = RadaClient::with_pacing(Duration::from_secs(0));
        let err = client
            .get_conditional(&format!("{base}/nope"))
            .expect_err("500 must be an error");
        match err {
            ApiError::Http { status, .. } => assert_eq!(status, 500),
            other => panic!("expected ApiError::Http, got {other:?}"),
        }
    }

    #[test]
    fn set_pace_updates_atomic() {
        let client = RadaClient::new();
        assert_eq!(client.pace(), 5);
        client.set_pace(9);
        assert_eq!(client.pace(), 9);

        let client = RadaClient::with_pacing(Duration::from_secs(3));
        assert_eq!(client.pace(), 3);
        client.set_pace(0);
        assert_eq!(client.pace(), 0);
    }

    #[test]
    fn cache_roundtrips_non_utf8_bytes() {
        // Windows-1251 bytes of a Cyrillic string — the dictionary dumps are
        // not valid UTF-8, so the Latin-1 cache representation must be
        // byte-exact.
        let raw: Vec<u8> = vec![
            0xC3, 0xE0, 0xE2, 0xE3, 0xE8, 0xE9, 0x20, 0xE7, 0xE0, 0xEA, 0xEE, 0xED, 0xFF,
        ];
        assert_eq!(cached_to_bytes(&bytes_to_cached(&raw)), raw);
    }
}
