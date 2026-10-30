//! Local web UI for the rada index.
//!
//! One embedded HTML page (vanilla JS, no network assets) plus a small JSON
//! API served by a blocking local HTTP server (no async runtime, no
//! framework — keeps the portable-exe promise: zero external dependencies
//! on the end-user machine).
//!
//! Local data (search, dictionaries, annotations) is fast; the live
//! endpoints (card / text / arrivals / resolve / export) go to
//! `data.rada.gov.ua` and obey the 5–7 s pacing via the shared [`RadaClient`].

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use rada_core::models::{LawCardJson, StatusId};
use rada_core::related::RelatedDoc;
use rada_core::RadaClient;
use rada_db::dict::all_labels;
use rada_db::search::{facets, search, SearchQuery, Sort};
use rada_db::{doc_count, last_import, open as db_open};
use rusqlite::Connection;
use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

pub mod ui;

/// All responses in this server are in-memory bodies.
type Resp = Response<std::io::Cursor<Vec<u8>>>;

/// Configuration for [`serve`].
#[derive(Debug, Clone)]
pub struct WebConfig {
    pub db_path: PathBuf,
    pub host: String,
    pub port: u16,
    /// Minimum pause between outgoing API requests (server asks for 5–7 s).
    pub pace_secs: u64,
    pub open_browser: bool,
}

/// One background index-update job («Дані» area of the UI). At most one
/// `running` job per kind at a time; the UI polls [`jobs_list`] and shows
/// `line` while the job runs, then `result` once it is `done` / `error`.
#[derive(Debug, Clone, serde::Serialize)]
struct JobRecord {
    id: String,
    /// `sync` | `snapshot` | `backup` | `restore` | `init`.
    kind: String,
    /// `running` | `done` | `error`.
    state: String,
    /// Latest progress line (Ukrainian, shown while running).
    line: String,
    /// Outcome message once the job left `running`.
    result: Option<String>,
    /// Monotonic registry counter of the last update (list ordering).
    updated: u64,
}

struct State {
    db_path: PathBuf,
    client: RadaClient,
    /// Minimum pause between outgoing API requests (server asks for 5–7 s);
    /// jobs build their own client with the same pace.
    pace_secs: u64,
    /// Background index-update jobs, keyed by job id.
    jobs: Mutex<HashMap<String, JobRecord>>,
    /// Monotonic id/counter for the job registry.
    job_seq: AtomicU64,
}

impl State {
    /// Registry lock (a panicking job must never poison the whole UI).
    fn jobs_lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, JobRecord>> {
        self.jobs.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Record a progress / result update for job `id`.
    fn job_set(&self, id: &str, state: &str, line: &str, result: Option<&str>) {
        let updated = self.job_seq.fetch_add(1, Ordering::SeqCst);
        if let Some(rec) = self.jobs_lock().get_mut(id) {
            rec.state = state.to_string();
            rec.line = line.to_string();
            rec.result = result.map(str::to_string);
            rec.updated = updated;
        }
    }
}

/// Run the server until the process is killed.
pub fn serve(cfg: &WebConfig) -> Result<()> {
    let addr: SocketAddr = format!("{}:{}", cfg.host, cfg.port)
        .parse()
        .with_context(|| format!("bind {}:{}", cfg.host, cfg.port))?;
    let server = Server::http(addr).map_err(|e| anyhow!("open socket: {e}"))?;
    let state = Arc::new(State {
        db_path: cfg.db_path.clone(),
        client: RadaClient::with_pacing(Duration::from_secs(cfg.pace_secs)),
        pace_secs: cfg.pace_secs,
        jobs: Mutex::new(HashMap::new()),
        job_seq: AtomicU64::new(0),
    });
    let url = format!("http://{}:{}", cfg.host, cfg.port);
    println!("[web] serving UI at {url}");
    println!(
        "[web] live endpoints (card/text/arrivals/export) pace at {} s per request",
        cfg.pace_secs
    );
    if cfg.open_browser {
        open_browser(&url);
    }
    loop {
        match server.recv() {
            Ok(req) => {
                let st = state.clone();
                std::thread::spawn(move || {
                    if let Err(e) = handle_request(req, &st) {
                        eprintln!("[web] request error: {e:#}");
                    }
                });
            }
            Err(e) => {
                eprintln!("[web] server stopped: {e}");
                break;
            }
        }
    }
    Ok(())
}

fn open_browser(url: &str) {
    #[cfg(windows)]
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", url])
        .spawn();
    #[cfg(unix)]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

// ---------------------------------------------------------------------------
// Request plumbing
// ---------------------------------------------------------------------------

fn handle_request(req: Request, st: &Arc<State>) -> Result<()> {
    let method = req.method().clone();
    let raw = req.url();
    let (path, query) = match raw.split_once('?') {
        Some((p, q)) => (p.to_string(), Some(q.to_string())),
        None => (raw.to_string(), None),
    };
    let qmap = parse_query(query.as_deref().unwrap_or(""));
    let mut req = req;
    let resp = route(&method, &decode_component(&path, false), &qmap, &mut req, st);
    req.respond(resp?)?;
    Ok(())
}

fn route(
    method: &Method,
    path: &str,
    q: &[(String, String)],
    req: &mut Request,
    st: &Arc<State>,
) -> Result<Resp> {
    if method == &Method::Get && path == "/" {
        return Ok(html_response());
    }
    let Some(rest) = path.strip_prefix("/api/") else {
        return Ok(json_err(404, &format!("unknown path {path}")));
    };
    let result = if rest.starts_with("doc/") {
        doc_handler(method, rest, q, st)
    } else if rest == "related" || rest.starts_with("related/") {
        related_handler(method, rest, st)
    } else if rest.starts_with("cache/") {
        cache_handler(method, rest, q, st)
    } else if rest.starts_with("export/") {
        export_handler(method, rest, q, st, req)
    } else if rest == "annotations" || rest.starts_with("annotations/") {
        annotations_handler(method, rest, q, st, req)
    } else if rest.starts_with("dict/") {
        dict_handler(method, rest, st)
    } else if rest == "backups" {
        if method == &Method::Get {
            backups_list(st)
        } else {
            json_err(405, "method not allowed")
        }
    } else if rest.starts_with("jobs") {
        jobs_handler(method, rest, st, req)
    } else {
        match rest {
            "status" => {
                if method == &Method::Get {
                    status_handler(st)
                } else {
                    json_err(405, "method not allowed")
                }
            }
            "search" => {
                if method == &Method::Get {
                    search_handler(st, q)
                } else {
                    json_err(405, "method not allowed")
                }
            }
            "arrivals" => {
                if method == &Method::Get {
                    arrivals_handler(st, q)
                } else {
                    json_err(405, "method not allowed")
                }
            }
            "fresh" => {
                if method == &Method::Get {
                    fresh_handler(st, q)
                } else {
                    json_err(405, "method not allowed")
                }
            }
            "years" => {
                if method == &Method::Get {
                    years_handler(st)
                } else {
                    json_err(405, "method not allowed")
                }
            }
            "yeardocs" => {
                if method == &Method::Get {
                    yeardocs_handler(st, q)
                } else {
                    json_err(405, "method not allowed")
                }
            }
            "dicts" => {
                if method == &Method::Get {
                    dicts_handler(st, q)
                } else {
                    json_err(405, "method not allowed")
                }
            }
            "resolve" => {
                if method == &Method::Get {
                    resolve_handler(st, q)
                } else {
                    json_err(405, "method not allowed")
                }
            }
            "cached" => {
                if method == &Method::Get {
                    cached_handler(st, q)
                } else {
                    json_err(405, "method not allowed")
                }
            }
            _ => json_err(404, &format!("unknown api path /api/{rest}")),
        }
    };
    Ok(result)
}

fn open_db(st: &State) -> Result<Connection> {
    db_open(&st.db_path)
        .with_context(|| format!("open index {}", st.db_path.display()))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

fn status_handler(st: &State) -> Resp {
    let conn = match open_db(st) {
        Ok(c) => c,
        Err(e) => return json_err(500, &format!("index unavailable: {e:#}")),
    };
    let docs = doc_count(&conn).unwrap_or(0);
    // The same Option value the desktop shows as «останній імпорт/синк»:
    // serialized as a string, or null when the index was never imported
    // (the footer then answers «оновлено: невідомо»).
    let last = last_import(&conn);
    let annotations: i64 = conn
        .query_row("SELECT COUNT(*) FROM annotations", [], |r| r.get(0))
        .unwrap_or(0);
    let dicts = json!({
        "stan": all_labels(&conn, "stan").len(),
        "typs": all_labels(&conn, "typs").len(),
        "organs": all_labels(&conn, "organs").len(),
    });
    // Offline text-cache size for the footer; status must never fail
    // because of cache stats, so an error here degrades to 0/0.
    let (cache_rows, cache_bytes) = rada_db::textcache::stats(&conn).unwrap_or((0, 0));
    json_ok(&json!({
        "version": env!("CARGO_PKG_VERSION"),
        "docs": docs,
        "last_import": last,
        "db_path": st.db_path.display().to_string(),
        "annotations": annotations,
        "dicts": dicts,
        "cache_rows": cache_rows,
        "cache_bytes": cache_bytes,
    }))
}

fn search_handler(st: &State, q: &[(String, String)]) -> Resp {
    let q = SearchQuery {
        nazva: qget(q, "q").filter(|s| !s.is_empty()).map(str::to_string),
        org_id: qget(q, "org").and_then(|s| s.parse().ok()),
        typ: qget(q, "typ").and_then(|s| s.parse().ok()),
        status: qget(q, "status").and_then(|s| s.parse().ok()),
        date_from: qget(q, "date_from").filter(|s| !s.is_empty()).map(str::to_string),
        date_to: qget(q, "date_to").filter(|s| !s.is_empty()).map(str::to_string),
        doc_number: qget(q, "number").filter(|s| !s.is_empty()).map(str::to_string),
        sort: qget(q, "sort").map(Sort::parse).unwrap_or_default(),
        limit: qget(q, "limit")
            .and_then(|s| s.parse().ok())
            .unwrap_or(50)
            .min(200),
        offset: qget(q, "offset").and_then(|s| s.parse().ok()).unwrap_or(0),
    };
    match open_db(st) {
        Ok(conn) => {
            // Offline number search: an nreg-shaped `q` with an exact index
            // hit answers with that single document (see `nreg_exact`).
            if let Some(t) = q.nazva.as_deref() {
                if q.org_id.is_none()
                    && q.typ.is_none()
                    && q.status.is_none()
                    && q.date_from.is_none()
                    && q.date_to.is_none()
                    && q.doc_number.is_none()
                    && rada_db::looks_like_nreg(t)
                {
                    if let Ok(Some(hit)) = rada_db::by_nreg(&conn, t) {
                        return json_ok(&json!({ "total": 1, "hits": [hit], "nreg_exact": true }));
                    }
                }
            }
            match search(&conn, &q) {
                Ok((hits, total)) => {
                    // Dictionary-dropdown facet counts («Чинний (1204)»).
                    // Search must never fail because of facets: on error
                    // the field is simply null.
                    let facet_json = match facets(&conn, &q) {
                        Ok(f) => serde_json::to_value(f).unwrap_or(Value::Null),
                        Err(_) => Value::Null,
                    };
                    json_ok(&json!({
                        "total": total,
                        "hits": hits,
                        "facets": facet_json,
                    }))
                }
                Err(e) => json_err(500, &format!("search error: {e:#}")),
            }
        }
        Err(e) => json_err(500, &format!("index unavailable: {e:#}")),
    }
}

/// Live feed of recently updated documents (pacing applies).
fn arrivals_handler(st: &State, q: &[(String, String)]) -> Resp {
    let limit = qget(q, "limit")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(200)
        .max(1)
        .min(1000);
    match st.client.updated_docs() {
        Ok(items) => {
            let items: Vec<_> = items.into_iter().take(limit).collect();
            json_ok(&json!({ "items": items }))
        }
        Err(e) => json_err(502, &format!("live feed error: {e}")),
    }
}

/// `GET /api/fresh?limit=N` — the freshest cards by adoption date, the web
/// twin of the desktop home card (`read_index_status` in the GUI): only
/// full 8-digit `org_date` rows (the CLI `recent` anchor rules), newest
/// first, same-day rows tie on the nreg. Answers up to min(N, 30) rows
/// (default 14), fully offline through the index.
fn fresh_handler(st: &State, q: &[(String, String)]) -> Resp {
    let limit = qget(q, "limit")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(14)
        .max(1)
        .min(30);
    let conn = match open_db(st) {
        Ok(c) => c,
        Err(e) => return json_err(500, &format!("index unavailable: {e:#}")),
    };
    let rows = conn
        .prepare(
            "SELECT org_date, nreg, nazva FROM docs \
             WHERE org_date GLOB '[0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9]' \
             ORDER BY org_date DESC, nreg ASC LIMIT ?1",
        )
        .and_then(|mut ps| {
            let rows = ps.query_map([limit as i64], |r| {
                Ok(json!({
                    "date": r.get::<_, String>(0)?,
                    "nreg": r.get::<_, String>(1)?,
                    "title": r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                }))
            })?;
            rows.collect::<Result<Vec<Value>, _>>()
        });
    match rows {
        Ok(docs) => json_ok(&json!({ "docs": docs })),
        Err(e) => json_err(500, &format!("fresh error: {e:#}")),
    }
}

/// Pure normalizer behind `GET /api/years`: keeps only full 4-digit ASCII
/// year keys carrying a positive count (zero-count years are simply
/// absent, exactly like the CLI `years` histogram) and orders the rows
/// ascending by the year string.
fn years_rows(rows: Vec<(String, i64)>) -> Vec<Value> {
    let mut rows: Vec<(String, i64)> = rows
        .into_iter()
        .filter(|(y, c)| *c > 0 && y.len() == 4 && y.bytes().all(|b| b.is_ascii_digit()))
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows.into_iter()
        .map(|(year, count)| json!({ "count": count, "year": year }))
        .collect()
}

/// `GET /api/years` — the whole-index year histogram: document counts per
/// calendar year over every doc with a full 8-digit `org_date` (the same
/// anchor rule as `/api/fresh`), ascending, no range params, zero-count
/// years omitted — the web twin of the CLI `years` report without its
/// `--from`/`--to` window. Fully offline through the index.
fn years_handler(st: &State) -> Resp {
    let conn = match open_db(st) {
        Ok(c) => c,
        Err(e) => return json_err(500, &format!("index unavailable: {e:#}")),
    };
    let rows = conn
        .prepare(
            "SELECT substr(org_date,1,4) AS y, COUNT(*) FROM docs \
             WHERE org_date GLOB '[0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9]' \
             GROUP BY y ORDER BY y ASC",
        )
        .and_then(|mut ps| {
            let rows = ps.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
            rows.collect::<Result<Vec<(String, i64)>, _>>()
        });
    match rows {
        Ok(rows) => json_ok(&json!({ "rows": years_rows(rows) })),
        Err(e) => json_err(500, &format!("years error: {e:#}")),
    }
}

/// `GET /api/yeardocs?year=РРРР&limit=N` — the documents behind one year of
/// the landing histogram (`/api/years`): only full 8-digit `org_date` rows
/// whose first four digits are the requested year (the same anchor rule as
/// `/api/fresh`), newest first, same-day rows tie on the nreg. The year must
/// be exactly 4 ASCII digits (anything else is a 400 in the file's error
/// family); answers up to N rows (default 200, clamped 1..=500, the
/// arrivals/fresh clamp idiom), fully offline through the index.
fn yeardocs_handler(st: &State, q: &[(String, String)]) -> Resp {
    let Some(year) = qget(q, "year") else {
        return json_err(400, "year required");
    };
    if year.len() != 4 || !year.bytes().all(|b| b.is_ascii_digit()) {
        return json_err(400, "year must be 4 digits");
    }
    let limit = qget(q, "limit")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(200)
        .max(1)
        .min(500);
    let conn = match open_db(st) {
        Ok(c) => c,
        Err(e) => return json_err(500, &format!("index unavailable: {e:#}")),
    };
    let glob = format!("{year}[0-9][0-9][0-9][0-9]");
    let rows = conn
        .prepare(
            "SELECT org_date, nreg, nazva FROM docs \
             WHERE org_date GLOB ?1 ORDER BY org_date DESC, nreg ASC LIMIT ?2",
        )
        .and_then(|mut ps| {
            let rows = ps.query_map(rusqlite::params![glob, limit as i64], |r| {
                Ok(json!({
                    "date": r.get::<_, String>(0)?,
                    "nreg": r.get::<_, String>(1)?,
                    "title": r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                }))
            })?;
            rows.collect::<Result<Vec<Value>, _>>()
        });
    match rows {
        Ok(docs) => json_ok(&json!({ "docs": docs, "year": year })),
        Err(e) => json_err(500, &format!("yeardocs error: {e:#}")),
    }
}

/// Live name → nreg resolver (pacing applies).
fn resolve_handler(st: &State, q: &[(String, String)]) -> Resp {
    let Some(text) = qget(q, "text").filter(|s| !s.trim().is_empty()) else {
        return json_err(400, "text required");
    };
    match st.client.resolve_name(text) {
        Some(nreg) => json_ok(&json!({ "nreg": nreg })),
        None => json_err(404, "no unambiguous match (the server returned a candidate list)"),
    }
}

/// `GET /api/related/{nreg}` — links to other acts parsed from the live
/// document page (one paced request). Offline titles replace page anchors.
fn related_handler(method: &Method, rest: &str, st: &State) -> Resp {
    if method != &Method::Get {
        return json_err(405, "method not allowed");
    }
    let Some(nreg) = rest.strip_prefix("related/") else {
        return json_err(400, "expected /api/related/{nreg}");
    };
    let nreg = nreg.trim_end_matches('/');
    if nreg.is_empty() {
        return json_err(400, "expected /api/related/{nreg}");
    }
    match st.client.related(nreg) {
        Ok(list) => {
            // Prefer offline index titles over the page's anchor text.
            let conn = open_db(st).ok();
            let items: Vec<Value> = list
                .into_iter()
                .map(|mut d| {
                    if let Some(conn) = &conn {
                        if let Some(t) = rada_db::title_by_nreg(conn, &d.nreg) {
                            if !t.trim().is_empty() {
                                d.title = t;
                            }
                        }
                    }
                    json!({ "nreg": d.nreg, "title": d.title })
                })
                .collect();
            json_ok(&json!({ "items": items }))
        }
        Err(e) => json_err(502, &format!("related error: {e}")),
    }
}

/// `GET /api/cache/search?q=…` — offline full-text scan of the cached
/// document bodies (`docs_text`), Unicode case-insensitive, ranked; and
/// `GET /api/cache/cached?nregs=…` — the batch cache probe below.
fn cache_handler(method: &Method, rest: &str, q: &[(String, String)], st: &State) -> Resp {
    if method != &Method::Get {
        return json_err(405, "method not allowed");
    }
    if rest == "cache/cached" {
        return cache_cached_handler(q, st);
    }
    if rest != "cache/search" {
        return json_err(404, "unknown cache path");
    }
    let Some(term) = qget(q, "q").filter(|s| !s.trim().is_empty()) else {
        return json_err(400, "q required");
    };
    let limit = qget(q, "limit")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(30)
        .max(1)
        .min(200);
    let conn = match open_db(st) {
        Ok(c) => c,
        Err(e) => return json_err(500, &format!("index unavailable: {e:#}")),
    };
    match rada_db::textcache::search_bodies(&conn, &term, limit) {
        Ok(hits) => {
            let items: Vec<Value> = hits
                .into_iter()
                .map(|h| {
                    json!({
                        "nreg": h.nreg,
                        "title": rada_db::title_by_nreg(&conn, &h.nreg).unwrap_or_default(),
                        "hits": h.hits,
                        "snippet": h.snippet,
                    })
                })
                .collect();
            json_ok(&json!({ "items": items }))
        }
        Err(e) => json_err(500, &e.to_string()),
    }
}

/// Parse the batch probe's `nregs` parameter: comma-split, trimmed, empties
/// dropped, duplicates removed (first occurrence wins, order kept), capped at
/// 200 entries — extras are ignored. A missing or blank parameter yields an
/// empty list.
fn parse_nregs_param(raw: Option<&str>) -> Vec<String> {
    const MAX_NREGS: usize = 200;
    let Some(raw) = raw else { return Vec::new(); };
    let mut out: Vec<String> = Vec::new();
    for part in raw.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if out.len() >= MAX_NREGS {
            break;
        }
        if !out.iter().any(|p| p == part) {
            out.push(part.to_string());
        }
    }
    out
}

/// `GET /api/cache/cached?nregs=a,b,c` — batch form of the single-nreg probe
/// below: answers with only those nregs (in request order) that actually have
/// a cached body in `docs_text`. The search page paints its « · кеш» row
/// badges from this. Read-only.
fn cache_cached_handler(q: &[(String, String)], st: &State) -> Resp {
    let nregs = parse_nregs_param(qget(q, "nregs"));
    if nregs.is_empty() {
        return json_ok(&json!({ "cached": [] }));
    }
    let conn = match open_db(st) {
        Ok(c) => c,
        Err(e) => return json_err(500, &format!("index unavailable: {e:#}")),
    };
    let hits: Vec<String> = nregs
        .into_iter()
        .filter(|nreg| rada_db::textcache::cached(&conn, nreg).unwrap_or(false))
        .collect();
    json_ok(&json!({ "cached": hits }))
}

/// `GET /api/cached?nreg=` — whether *any* edition of the document already
/// has an offline body copy in `docs_text` (the web reader's «кешовано»
/// hint asks this to confirm the source before lighting up). Read-only.
fn cached_handler(st: &State, q: &[(String, String)]) -> Resp {
    let Some(nreg) = qget(q, "nreg") else {
        return json_err(400, "nreg required");
    };
    let conn = match open_db(st) {
        Ok(c) => c,
        Err(e) => return json_err(500, &format!("index unavailable: {e:#}")),
    };
    match rada_db::textcache::cached(&conn, nreg) {
        Ok(hit) => json_ok(&json!({ "cached": hit })),
        Err(e) => json_err(500, &e.to_string()),
    }
}

/// `doc/{nreg}/card` and `doc/{nreg}/text` — live, pacing applies.
fn doc_handler(method: &Method, rest: &str, q: &[(String, String)], st: &State) -> Resp {
    if method != &Method::Get {
        return json_err(405, "method not allowed");
    }
    let Some(tail) = rest.strip_prefix("doc/") else {
        return json_err(400, "expected /api/doc/{nreg}/card or /text");
    };
    // nreg may contain `/` (e.g. `254к/96-вр`), so split on the LAST slash.
    let Some((nreg, what)) = tail.rsplit_once('/') else {
        return json_err(400, "expected /api/doc/{nreg}/card or /text");
    };
    match what {
        "card" => match st.client.card(nreg) {
            Ok(card) => json_ok(&json!({
                "card": card,
                "status_label": StatusId(card.status).label(),
                "site_url": RadaClient::site_url(nreg),
            })),
            Err(e) => json_err(502, &format!("card error: {e}")),
        },
        "text" => {
            let edition = qget(q, "edition").filter(|s| !s.is_empty());
            let key = edition.as_deref().unwrap_or("");
            let fetched = match &edition {
                Some(ed) => st.client.show_edition_text(nreg, ed),
                None => st.client.show_text(nreg),
            };
            match fetched {
                // Live success refreshes the offline cache (GUI parity).
                Ok(t) => {
                    if let Ok(conn) = open_db(st) {
                        let _ = rada_db::textcache::put(&conn, nreg, key, &t);
                    }
                    json_ok(&json!({
                        "text": t,
                        "edition": edition,
                        "cached": false,
                    }))
                }
                // Live failure falls back to the cached body; the reader then
                // lights its «кешовано» hint off the returned flag.
                Err(e) => {
                    let hit = open_db(st)
                        .ok()
                        .and_then(|conn| rada_db::textcache::get(&conn, nreg, key).ok().flatten());
                    match hit {
                        Some(t) => json_ok(&json!({
                            "text": t,
                            "edition": edition,
                            "cached": true,
                        })),
                        None => json_err(502, &format!("text error: {e}")),
                    }
                }
            }
        }
        _ => json_err(404, &format!("unknown doc endpoint {what}")),
    }
}

/// `GET /api/dict/{kind}`.
fn dict_handler(method: &Method, rest: &str, st: &State) -> Resp {
    if method != &Method::Get {
        return json_err(405, "method not allowed");
    }
    let Some(kind) = rest.strip_prefix("dict/") else {
        return json_err(400, "expected /api/dict/{stan|typs|organs}");
    };
    if !matches!(kind, "stan" | "typs" | "organs") {
        return json_err(404, &format!("unknown dict {kind}"));
    }
    let conn = match open_db(st) {
        Ok(c) => c,
        Err(e) => return json_err(500, &format!("index unavailable: {e:#}")),
    };
    let items = all_labels(&conn, kind)
        .into_iter()
        .map(|(id, label)| json!({ "id": id, "label": label }))
        .collect::<Vec<_>>();
    json_ok(&json!({ "kind": kind, "items": items }))
}

/// `GET /api/dicts?kind=K&q=TEXT&limit=N` — the web twin of the CLI `dicts`
/// command (the desktop home «Пошук у словниках» panel): browse/search ONE
/// reference dictionary. `kind` (stan|typs|organs) is required — missing or
/// anything else is a 400 in the file's error family; `q` is an optional
/// case-insensitive substring of the label. The filter runs on the Rust side
/// through plain `to_lowercase` instead of a SQL `LIKE`/`LOWER`: SQLite folds
/// only ASCII case, so a Cyrillic needle like «міністер» would silently miss
/// every «Міністерство …» label, while `to_lowercase` folds Ukrainian letters
/// (the CLI `dicts` precedent, which shares this exact reasoning with
/// `dict --search`); skipping `LIKE` also sidesteps the `%`/`_`
/// wildcard-escaping caveat entirely. Rows arrive `ORDER BY id ASC` from
/// [`all_labels`] and the limit (default 50, clamped 1..=200, the
/// arrivals/fresh clamp idiom) is applied AFTER the filter — a SQL LIMIT
/// before the filter would drop matches sorted past the cutoff, and the
/// dictionaries hold a few hundred rows, so the full-kind read is cheap.
/// Fully offline through the index.
fn dicts_handler(st: &State, q: &[(String, String)]) -> Resp {
    let Some(kind) = qget(q, "kind") else {
        return json_err(400, "kind required (stan|typs|organs)");
    };
    if !matches!(kind, "stan" | "typs" | "organs") {
        return json_err(400, &format!("unknown kind {kind}"));
    }
    // Trim once and fold with plain `to_lowercase` (the CLI `dict_matches`
    // rule: an empty or whitespace-only needle matches every label).
    let needle = qget(q, "q").unwrap_or("").trim().to_lowercase();
    let limit = qget(q, "limit")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(50)
        .max(1)
        .min(200);
    let conn = match open_db(st) {
        Ok(c) => c,
        Err(e) => return json_err(500, &format!("index unavailable: {e:#}")),
    };
    let rows = all_labels(&conn, kind)
        .into_iter()
        .filter(|(_, label)| needle.is_empty() || label.to_lowercase().contains(&needle))
        .take(limit)
        .map(|(id, label)| json!({ "id": id, "label": label }))
        .collect::<Vec<_>>();
    json_ok(&json!({ "rows": rows, "kind": kind }))
}

/// `GET /api/annotations?nreg=`, `POST /api/annotations`,
/// `PUT /api/annotations` (body `{"id":…, "text":…}`),
/// `DELETE /api/annotations/{id}`, plus the «Обране» page routes
/// `GET /api/annotations/all`, `GET /api/annotations/export` and
/// `POST /api/annotations/import`.
fn annotations_handler(
    method: &Method,
    rest: &str,
    q: &[(String, String)],
    st: &State,
    req: &mut Request,
) -> Resp {
    // Aggregate sub-routes (each one guards its own method) must be matched
    // before the generic id/body handling below.
    match rest {
        "annotations/all" => {
            return if method == &Method::Get {
                annotations_all(st)
            } else {
                json_err(405, "method not allowed")
            };
        }
        "annotations/export" => {
            return if method == &Method::Get {
                annotations_export(st)
            } else {
                json_err(405, "method not allowed")
            };
        }
        "annotations/import" => {
            return if method == &Method::Post {
                annotations_import(st, req)
            } else {
                json_err(405, "method not allowed")
            };
        }
        _ => {}
    }
    match method {
        Method::Get => {
            let Some(nreg) = qget(q, "nreg") else {
                return json_err(400, "nreg required");
            };
            match open_db(st) {
                Ok(conn) => match rada_db::annotations::list(&conn, nreg) {
                    Ok(items) => json_ok(&json!({ "items": items })),
                    Err(e) => json_err(500, &e.to_string()),
                },
                Err(e) => json_err(500, &format!("index unavailable: {e:#}")),
            }
        }
        Method::Post => {
            let body = match read_body(req) {
                Ok(b) => b,
                Err(e) => return json_err(400, &format!("bad body: {e}")),
            };
            let v: Value = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(e) => return json_err(400, &format!("bad JSON: {e}")),
            };
            let nreg = v.get("nreg").and_then(Value::as_str).unwrap_or("").to_string();
            let kind = v.get("kind").and_then(Value::as_str).unwrap_or("bookmark").to_string();
            let text = v.get("text").and_then(Value::as_str).unwrap_or("").to_string();
            let span_start = v.get("span_start").and_then(Value::as_i64);
            let span_end = v.get("span_end").and_then(Value::as_i64);
            if nreg.is_empty() {
                return json_err(400, "nreg required");
            }
            let conn = match open_db(st) {
                Ok(c) => c,
                Err(e) => return json_err(500, &format!("index unavailable: {e:#}")),
            };
            match rada_db::annotations::insert(&conn, &nreg, &kind, &text, span_start, span_end) {
                Ok(id) => json_ok(&json!({ "id": id })),
                Err(e) => json_err(400, &e.to_string()),
            }
        }
        Method::Put => {
            let body = match read_body(req) {
                Ok(b) => b,
                Err(e) => return json_err(400, &format!("bad body: {e}")),
            };
            let v: Value = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(e) => return json_err(400, &format!("bad JSON: {e}")),
            };
            let Some(id) = v.get("id").and_then(Value::as_i64) else {
                return json_err(400, "id required");
            };
            let text = v
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            if text.is_empty() {
                return json_err(400, "text must not be empty");
            }
            let conn = match open_db(st) {
                Ok(c) => c,
                Err(e) => return json_err(500, &format!("index unavailable: {e:#}")),
            };
            match rada_db::annotations::set_text(&conn, id, &text) {
                Ok(true) => json_ok(&json!({ "id": id })),
                Ok(false) => json_err(404, "annotation not found"),
                Err(e) => json_err(400, &e.to_string()),
            }
        }
        Method::Delete => {
            let Some(idstr) = rest.strip_prefix("annotations/") else {
                return json_err(404, "expected /api/annotations/{id}");
            };
            let Some(id) = idstr.parse().ok() else {
                return json_err(400, "id must be an integer");
            };
            let conn = match open_db(st) {
                Ok(c) => c,
                Err(e) => return json_err(500, &format!("index unavailable: {e:#}")),
            };
            match rada_db::annotations::remove(&conn, id) {
                Ok(true) => Response::from_string(String::new()).with_status_code(StatusCode(204)),
                Ok(false) => json_err(404, "annotation not found"),
                Err(e) => json_err(500, &e.to_string()),
            }
        }
        _ => json_err(405, "method not allowed"),
    }
}

/// `GET /api/annotations/all` — every annotation across all documents,
/// newest first, joined with the indexed document title («Обране» page).
/// Title is `""` when the document is no longer in the index.
fn annotations_all(st: &State) -> Resp {
    let conn = match open_db(st) {
        Ok(c) => c,
        Err(e) => return json_err(500, &format!("index unavailable: {e:#}")),
    };
    match rada_db::annotations::all(&conn) {
        Ok(items) => {
            // `all()` returns rows ordered by id; the UI wants newest first.
            let items: Vec<Value> = items
                .into_iter()
                .rev()
                .map(|a| {
                    json!({
                        "id": a.id,
                        "nreg": a.nreg,
                        "title": rada_db::title_by_nreg(&conn, &a.nreg).unwrap_or_default(),
                        "kind": a.kind,
                        "text": a.text,
                        "created_at": a.created_at,
                        // Context snippet from the offline text cache (None
                        // until the document text was fetched at least once).
                        "preview": if a.text.trim().is_empty() {
                            rada_db::textcache::preview(&conn, &a.nreg, "", 200)
                        } else {
                            None
                        },
                    })
                })
                .collect();
            json_ok(&json!({ "items": items }))
        }
        Err(e) => json_err(500, &e.to_string()),
    }
}

/// `GET /api/annotations/export` — the portable JSON backup
/// (`annotations::export_json`) served as a file download.
fn annotations_export(st: &State) -> Resp {
    let conn = match open_db(st) {
        Ok(c) => c,
        Err(e) => return json_err(500, &format!("index unavailable: {e:#}")),
    };
    match rada_db::annotations::export_json(&conn) {
        Ok(body) => {
            let stamp = rada_db::build::now_stamp().replace(' ', "_").replace(':', "");
            let filename = format!("rada-annotations-{stamp}.json");
            Response::from_string(body)
                .with_header(h("Content-Type", "application/json; charset=utf-8"))
                .with_header(h(
                    "Content-Disposition",
                    &format!("attachment; filename=\"{filename}\""),
                ))
        }
        Err(e) => json_err(500, &e.to_string()),
    }
}

/// `POST /api/annotations/import` — body is the export JSON (a plain array
/// or `{"annotations": […]}`); malformed items are skipped. Returns the
/// number of imported annotations.
fn annotations_import(st: &State, req: &mut Request) -> Resp {
    let body = match read_body(req) {
        Ok(b) => b,
        Err(e) => return json_err(400, &format!("bad body: {e}")),
    };
    let conn = match open_db(st) {
        Ok(c) => c,
        Err(e) => return json_err(500, &format!("index unavailable: {e:#}")),
    };
    match rada_db::annotations::import_json(&conn, &body) {
        Ok(n) => json_ok(&json!({ "imported": n })),
        Err(e) => json_err(400, &e.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Background index-update jobs («Дані»: live sync + snapshot fetch)
// ---------------------------------------------------------------------------

/// `GET /api/jobs`, `POST /api/jobs/sync`, `POST /api/jobs/snapshot`,
/// `POST /api/jobs/backup`, `POST /api/jobs/restore`, `POST /api/jobs/init`.
fn jobs_handler(method: &Method, rest: &str, st: &Arc<State>, req: &mut Request) -> Resp {
    match rest {
        "jobs" => {
            if method == &Method::Get {
                jobs_list(st)
            } else {
                json_err(405, "method not allowed")
            }
        }
        "jobs/sync" => {
            if method == &Method::Post {
                start_job(st, "sync")
            } else {
                json_err(405, "method not allowed")
            }
        }
        "jobs/snapshot" => {
            if method == &Method::Post {
                start_job(st, "snapshot")
            } else {
                json_err(405, "method not allowed")
            }
        }
        "jobs/backup" => {
            if method == &Method::Post {
                start_job(st, "backup")
            } else {
                json_err(405, "method not allowed")
            }
        }
        "jobs/restore" => {
            if method == &Method::Post {
                start_restore_job(st, req)
            } else {
                json_err(405, "method not allowed")
            }
        }
        "jobs/init" => {
            if method == &Method::Post {
                start_job(st, "init")
            } else {
                json_err(405, "method not allowed")
            }
        }
        _ => json_err(404, &format!("unknown api path /api/{rest}")),
    }
}

/// Registry view for the UI: every known job, oldest update first.
fn jobs_list(st: &State) -> Resp {
    let mut jobs: Vec<JobRecord> = st.jobs_lock().values().cloned().collect();
    jobs.sort_by_key(|r| r.updated);
    let items: Vec<Value> = jobs
        .iter()
        .map(|r| {
            json!({
                "id": r.id,
                "kind": r.kind,
                "state": r.state,
                "line": r.line,
                "result": r.result,
                "updated": r.updated,
            })
        })
        .collect();
    json_ok(&json!({ "jobs": items }))
}

/// Register a new `kind` job unless one is still running; returns its id.
/// Tests use this plain per-kind form; the server itself claims through
/// [`start_job_with`] with its cross-kind exclusions ([`claim_job_guarded`]).
#[cfg(test)]
fn claim_job(st: &State, kind: &str) -> Option<String> {
    claim_job_guarded(st, kind, &[])
}

/// [`claim_job`] with extra kinds: refuses while any running job's kind is
/// `kind` itself or one of `also_block`.
fn claim_job_guarded(st: &State, kind: &str, also_block: &[&str]) -> Option<String> {
    let mut jobs = st.jobs_lock();
    if jobs.values().any(|r| {
        r.state == "running" && (r.kind == kind || also_block.contains(&r.kind.as_str()))
    }) {
        return None;
    }
    let n = st.job_seq.fetch_add(1, Ordering::SeqCst);
    let id = format!("{kind}-{n}");
    jobs.insert(
        id.clone(),
        JobRecord {
            id: id.clone(),
            kind: kind.to_string(),
            state: "running".to_string(),
            line: "підготовка…".to_string(),
            result: None,
            updated: n,
        },
    );
    Some(id)
}

/// Background job body: `fn(state, job id)`. Real jobs block on paced live
/// requests and SQLite; tests inject a fake runner.
type JobRunner = fn(Arc<State>, String);

/// Start a background job. Blocking work (paced live requests, SQLite)
/// runs on a plain OS thread — this server has no async runtime.
fn start_job(st: &Arc<State>, kind: &'static str) -> Resp {
    let runner: JobRunner = match kind {
        "sync" => run_sync_job,
        "snapshot" => run_snapshot_job,
        "backup" => run_backup_job,
        "init" => run_init_job,
        _ => return json_err(400, &format!("unknown job kind {kind}")),
    };
    start_job_with(st, kind, runner)
}

fn start_job_with<F>(st: &Arc<State>, kind: &str, runner: F) -> Resp
where
    F: Fn(Arc<State>, String) + Send + 'static,
{
    // `claim_job` guards one job per kind; the jobs that rebuild or replace
    // the served db must additionally never overlap each other.
    let also_block: &[&str] = match kind {
        "init" => &["snapshot", "backup", "restore"],
        "snapshot" | "backup" | "restore" => &["init"],
        _ => &[],
    };
    let Some(id) = claim_job_guarded(st, kind, also_block) else {
        let article = if kind == "init" { "an" } else { "a" };
        return json_err(409, &format!("{article} {kind} job is already running"));
    };
    let st2 = st.clone();
    let thread_id = id.clone();
    std::thread::spawn(move || runner(st2, thread_id));
    json_ok(&json!({ "id": id, "kind": kind, "state": "running" }))
}

/// Live sync, mirroring the CLI `sync`: dictionaries + updated-docs feed,
/// paced with the server's configured pause (same pattern as `cmd_sync`).
fn run_sync_job(st: Arc<State>, id: String) {
    st.job_set(&id, "running", "відкриваю індекс …", None);
    let c = RadaClient::with_pacing(Duration::from_secs(st.pace_secs));
    let mut conn = match db_open(&st.db_path) {
        Ok(conn) => conn,
        Err(e) => {
            st.job_set(&id, "error", "індекс недоступний", Some(&format!("{e:#}")));
            return;
        }
    };

    st.job_set(&id, "running", "оновлюю словники …", None);
    let failed_dicts = match rada_db::sync::refresh_dicts(&mut conn, |kind| Ok(c.dictionary(kind)?)) {
        Ok(failed) => failed,
        Err(e) => {
            st.job_set(&id, "error", "словники не вдалося оновити", Some(&format!("{e:#}")));
            return;
        }
    };

    st.job_set(&id, "running", "отримую стрічку оновлених документів …", None);
    let updates = match c.updated_docs() {
        Ok(u) => u,
        Err(e) => {
            st.job_set(&id, "error", "стрічка оновлень недоступна", Some(&e.to_string()));
            return;
        }
    };
    let total = updates.len();
    st.job_set(&id, "running", &format!("{total} оновлених документів …"), None);

    // Feed rows carry nreg + title only; refresh metadata via the JSON card
    // (exactly the CLI card→DocCard mapping). One progress line per card.
    let mut done = 0usize;
    let (applied, failed) = match rada_db::sync::apply_updates(&mut conn, &updates, |nreg| {
        done += 1;
        st.job_set(&id, "running", &format!("картка {done}/{total}: {nreg}"), None);
        match c.card(nreg) {
            Ok(j) => Ok(rada_core::models::DocCard {
                dokid: 0, // unknown from card; keep existing via conflict path
                nreg: j.nreg.clone(),
                nazva: j.nazva.clone(),
                status: j.status,
                types: j.types.clone(),
                organs: format!(
                    "{}:{}:",
                    j.org,
                    j.orgdat.map(|d| d.to_string()).unwrap_or_default()
                ),
                minjust: j.minjust.clone().unwrap_or_default(),
                npix: 0,
                poddat: j.poddat.map(|d| d.to_string()).unwrap_or_default(),
            }),
            Err(e) => Err(anyhow::Error::from(e)),
        }
    }) {
        Ok(pair) => pair,
        Err(e) => {
            st.job_set(&id, "error", "не вдалося застосувати оновлення", Some(&format!("{e:#}")));
            return;
        }
    };
    let mut result = format!("застосовано {applied}, помилок {failed} (з {total} оновлених)");
    if !failed_dicts.is_empty() {
        result.push_str(&format!("; словники не завантажено: {}", failed_dicts.join(", ")));
    }
    st.job_set(&id, "done", "готово", Some(&result));
}

/// Snapshot fetch, mirroring the CLI `snapshot fetch` / the GUI worker:
/// annotations are backed up first, the published `.sqlite.gz` is streamed
/// to `<db>.sqlite.gz.tmp`, restored onto the db path, annotations re-added.
fn run_snapshot_job(st: Arc<State>, id: String) {
    let db = st.db_path.clone();
    st.job_set(&id, "running", "зберігаю анотації …", None);
    let keep: Vec<rada_db::annotations::Annotation> = std::fs::metadata(&db)
        .ok()
        .and_then(|_| db_open(&db).ok())
        .map(|conn| rada_db::annotations::all(&conn).unwrap_or_default())
        .unwrap_or_default();
    let n = keep.len();

    let gh = rada_core::GitHub::default();
    let outcome = (|| -> Result<String> {
        st.job_set(&id, "running", "шукаю останній снапшот на GitHub …", None);
        let url = gh.latest_snapshot_url("shevchukdp", "rada_client", None)?;
        let gz = db.with_extension("sqlite.gz.tmp");
        std::fs::remove_file(&gz).ok();
        st.job_set(&id, "running", "завантажую снапшот …", None);
        let mut last_mb: u64 = 0;
        let size = gh.download_with_progress(&url, &gz, |done| {
            let mb = done / (1 << 20);
            if mb > last_mb {
                last_mb = mb;
                st.job_set(&id, "running", &format!("завантажено {mb} МБ …"), None);
            }
        })?;
        st.job_set(&id, "running", "розгортаю базу …", None);
        let restored = rada_db::snapshot::restore(&gz, &db)?;
        let _ = std::fs::remove_file(&gz);
        if !keep.is_empty() {
            st.job_set(&id, "running", &format!("повертаю {n} анотацій …"), None);
            let conn = db_open(&restored)?;
            for a in &keep {
                let _ = rada_db::annotations::insert(
                    &conn,
                    &a.nreg,
                    &a.kind,
                    &a.text,
                    a.span_start,
                    a.span_end,
                );
            }
        }
        Ok(format!(
            "індекс оновлено з снапшота ({:.1} МБ, анотацій повернуто: {n})",
            size as f64 / 1_048_576.0
        ))
    })();
    match outcome {
        Ok(msg) => st.job_set(&id, "done", "готово", Some(&msg)),
        Err(e) => st.job_set(&id, "error", "снапшот не вдалося отримати", Some(&format!("{e:#}"))),
    }
}

/// Full index rebuild from the official `doc.txt` dump, mirroring the CLI
/// `init`: paced download via `full_dump()` (no progress callback exists —
/// the client blocks, so progress lines cover the download / import phases)
/// then `rada_db::build::import_dump` into the served db. The import runs
/// in one upsert transaction keyed on `nreg`, so a mid-build failure leaves
/// the previous index untouched (no file swap needed), and the per-request
/// `open_db` handlers keep serving the committed state throughout.
fn run_init_job(st: Arc<State>, id: String) {
    let outcome = (|| -> Result<usize> {
        st.job_set(
            &id,
            "running",
            "завантажую офіційний дамп doc.txt (≈51 МБ) …",
            None,
        );
        let c = RadaClient::with_pacing(Duration::from_secs(st.pace_secs));
        let raw = c.full_dump().map_err(|e| anyhow!("дамп doc.txt недоступний: {e}"))?;
        st.job_set(
            &id,
            "running",
            &format!("дамп отримано ({} МБ), імпортую …", raw.len() / (1 << 20)),
            None,
        );
        let mut conn = db_open(&st.db_path)?;
        let n = rada_db::build::import_dump(&mut conn, &raw)?;
        let _ = conn.execute_batch("ANALYZE;");
        Ok(n)
    })();
    match outcome {
        Ok(n) => st.job_set(
            &id,
            "done",
            "готово",
            Some(&format!("індекс побудовано з дампа: {n} документів")),
        ),
        Err(e) => st.job_set(
            &id,
            "error",
            "побудова індексу не вдалася",
            Some(&format!("{e:#}")),
        ),
    }
}

// ---------------------------------------------------------------------------
// Backup / restore jobs («Дані»: gzip snapshots kept next to the index)
// ---------------------------------------------------------------------------

/// Directory for `rada-backup-*.sqlite.gz` files: `backups/` next to the db.
fn backups_dir(st: &State) -> PathBuf {
    st.db_path
        .parent()
        .map(|p| p.join("backups"))
        .unwrap_or_else(|| PathBuf::from("backups"))
}

/// Validate a `{"file": …}` restore target: a plain `*.sqlite.gz` basename —
/// no path separators, traversal or drive letters. `None` ⇒ caller 400s.
fn validate_backup_name(file: &str) -> Option<String> {
    let name = file.trim();
    if name.is_empty()
        || !name.ends_with(".sqlite.gz")
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || name.contains(':')
    {
        return None;
    }
    Some(name.to_string())
}

/// `POST /api/jobs/backup` job body: gzip the current index into
/// `backups/rada-backup-<stamp>.sqlite.gz` — the same `snapshot::create`
/// path the GUI «Резервна копія» uses.
fn run_backup_job(st: Arc<State>, id: String) {
    st.job_set(&id, "running", "створюю резервну копію …", None);
    let stamp = rada_db::build::now_stamp().replace(' ', "_").replace(':', "");
    let name = format!("rada-backup-{stamp}.sqlite.gz");
    let out = backups_dir(&st).join(&name);
    match rada_db::snapshot::create(&st.db_path, &out) {
        Ok(size) => st.job_set(
            &id,
            "done",
            "готово",
            Some(&format!(
                "резервну копію збережено: {name} ({:.1} МБ)",
                size as f64 / 1_048_576.0
            )),
        ),
        Err(e) => {
            let _ = std::fs::remove_file(&out);
            st.job_set(&id, "error", "резервна копія не вдалася", Some(&format!("{e:#}")));
        }
    }
}

/// `POST /api/jobs/restore` with body `{"file": "<basename>.sqlite.gz"}`.
/// Everything is validated *before* the single-flight guard is claimed, so
/// bad requests never occupy the registry.
fn start_restore_job(st: &Arc<State>, req: &mut Request) -> Resp {
    let body = match read_body(req) {
        Ok(b) => b,
        Err(e) => return json_err(400, &format!("bad body: {e}")),
    };
    let v: Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => return json_err(400, &format!("bad JSON: {e}")),
    };
    let Some(raw) = v.get("file").and_then(Value::as_str) else {
        return json_err(400, "file required");
    };
    let Some(name) = validate_backup_name(raw) else {
        return json_err(
            400,
            "file must be a *.sqlite.gz basename from the backups directory",
        );
    };
    if !backups_dir(st).join(&name).is_file() {
        return json_err(400, &format!("backup {name} not found"));
    }
    start_job_with(st, "restore", move |st, id| {
        run_restore_job(&st, &id, &name);
    })
}

/// Restore a validated backup over the db path, mirroring the GUI
/// `run_restore_local` order of operations (annotations out → restore → in).
/// The gzip is unpacked to a temp file first — `snapshot::restore` runs its
/// integrity check there — and the previous db is only parked + swapped once
/// the new file is verified, so a failed restore never loses a usable db.
fn run_restore_job(st: &Arc<State>, id: &str, name: &str) {
    let db = st.db_path.clone();
    let src = backups_dir(st).join(name);
    st.job_set(id, "running", "зберігаю анотації …", None);
    let keep: Vec<rada_db::annotations::Annotation> = std::fs::metadata(&db)
        .ok()
        .and_then(|_| db_open(&db).ok())
        .map(|conn| rada_db::annotations::all(&conn).unwrap_or_default())
        .unwrap_or_default();
    let n = keep.len();
    let outcome = (|| -> Result<String> {
        st.job_set(id, "running", "розгортаю резервну копію …", None);
        let tmp = db.with_extension("sqlite.restore-tmp");
        std::fs::remove_file(&tmp).ok();
        let restored = rada_db::snapshot::restore(&src, &tmp)?;
        // Swap: park the old db (with its WAL siblings), move the verified
        // file in, roll the park back if that last step ever fails.
        let old = db.with_extension("sqlite.restore-old");
        std::fs::remove_file(&old).ok();
        let had_db = std::fs::metadata(&db).is_ok();
        if had_db {
            std::fs::rename(&db, &old).with_context(|| format!("park {}", db.display()))?;
            std::fs::remove_file(db.with_extension("sqlite-wal")).ok();
            std::fs::remove_file(db.with_extension("sqlite-shm")).ok();
        }
        if let Err(e) = std::fs::rename(&restored, &db) {
            if had_db {
                let _ = std::fs::rename(&old, &db);
            }
            return Err(e).with_context(|| format!("swap {}", db.display()));
        }
        if had_db {
            let _ = std::fs::remove_file(&old);
        }
        if !keep.is_empty() {
            st.job_set(id, "running", &format!("повертаю {n} анотацій …"), None);
            let conn = db_open(&db)?;
            for a in &keep {
                let _ = rada_db::annotations::insert(
                    &conn,
                    &a.nreg,
                    &a.kind,
                    &a.text,
                    a.span_start,
                    a.span_end,
                );
            }
        }
        Ok(format!("індекс відновлено з {name} (анотацій повернуто: {n})"))
    })();
    match outcome {
        Ok(msg) => st.job_set(id, "done", "готово", Some(&msg)),
        Err(e) => st.job_set(id, "error", "відновлення не вдалося", Some(&format!("{e:#}"))),
    }
}

/// `GET /api/backups` — the `*.sqlite.gz` files in the backups directory,
/// newest first, for the «Дані» restore picker.
fn backups_list(st: &State) -> Resp {
    let dir = backups_dir(st);
    let mut entries: Vec<(u64, String, f64)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for ent in rd.flatten() {
            let path = ent.path();
            let Some(name) = path.file_name().and_then(|s| s.to_str()).map(str::to_string) else {
                continue;
            };
            if !name.ends_with(".sqlite.gz") {
                continue;
            }
            let Ok(md) = std::fs::metadata(&path) else {
                continue;
            };
            if !md.is_file() {
                continue;
            }
            let created = md
                .created()
                .or_else(|_| md.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            entries.push((created, name, md.len() as f64 / 1_048_576.0));
        }
    }
    // Newest first; name breaks exact-timestamp ties deterministically.
    entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let backups: Vec<Value> = entries
        .into_iter()
        .map(|(created, name, mb)| {
            json!({
                "name": name,
                "size_mb": (mb * 10.0).round() / 10.0,
                "created": created,
            })
        })
        .collect();
    json_ok(&json!({ "backups": backups }))
}

// ---------------------------------------------------------------------------
// Export (txt / md / html), single + bulk
// ---------------------------------------------------------------------------

/// `GET /api/export/{nreg}?fmt=&edition=` (live, pacing applies) and
/// `POST /api/export/bulk` with a JSON body `{"nregs":[...], "fmt":"md"}`.
fn export_handler(
    method: &Method,
    rest: &str,
    q: &[(String, String)],
    st: &State,
    req: &mut Request,
) -> Resp {
    let Some(tail) = rest.strip_prefix("export/") else {
        return json_err(400, "expected /api/export/{nreg} or /api/export/bulk");
    };
    if method == &Method::Get && tail != "bulk" {
        let print = qget(q, "print").is_some();
        let fmt = if print { "html".to_string() } else { qget(q, "fmt").unwrap_or("txt").to_string() };
        let edition = qget(q, "edition").filter(|s| !s.is_empty()).map(str::to_string);
        match build_export(st, tail, &fmt, edition.as_deref()) {
            Ok((body, ext, filename)) => {
                if print {
                    // Inline print view: same document, auto-opens the
                    // browser print dialog on load. The route is already
                    // live (card + text), so one more paced request adds
                    // the «Пов'язані документи» section — best-effort: a
                    // failed or empty fetch simply omits it, so printing
                    // never depends on the related endpoint.
                    let related = st
                        .client
                        .related(tail)
                        .ok()
                        .filter(|l| !l.is_empty());
                    let conn = open_db(st).ok();
                    let body = print_body_with_related(&body, related, conn.as_ref());
                    let theme = qget(q, "theme").unwrap_or("");
                    Response::from_data(print_wrap(&body, theme).into_bytes())
                        .with_chunked_threshold(usize::MAX)
                        .with_header(h("Content-Type", "text/html; charset=utf-8"))
                } else {
                    file_response(&body, &ext, &filename)
                }
            }
            Err(e) => json_err(502, &e.to_string()),
        }
    } else if method == &Method::Post && tail == "bulk" {
        let body = match read_body(req) {
            Ok(b) => b,
            Err(e) => return json_err(400, &format!("bad body: {e}")),
        };
        let v: Value = match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(e) => return json_err(400, &format!("bad JSON: {e}")),
        };
        let nregs: Vec<String> = v
            .get("nregs")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default();
        if nregs.is_empty() {
            return json_err(400, "nregs must be a non-empty array");
        }
        let fmt = v.get("fmt").and_then(Value::as_str).unwrap_or("txt").to_string();
        let edition = v
            .get("edition")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        // Keep bulk runs bounded: ~5 s per document per live request.
        let nregs: Vec<String> = nregs.into_iter().take(50).collect();
        match build_bulk(st, &nregs, &fmt, edition.as_deref()) {
            Ok((body, ext, filename)) => file_response(&body, &ext, &filename),
            Err(e) => json_err(502, &e.to_string()),
        }
    } else {
        json_err(405, "method not allowed")
    }
}

/// Live card + text, rendered in `fmt` (txt/md/html).
fn build_export(st: &State, nreg: &str, fmt: &str, edition: Option<&str>) -> Result<(String, String, String)> {
    let card = st
        .client
        .card(nreg)
        .with_context(|| format!("card {nreg}"))?;
    let text = fetch_text(st, nreg, edition)?;
    let (body, ext) = render_export(nreg, &card, &text, fmt);
    let filename = export_filename(nreg, &ext);
    Ok((body, ext, filename))
}

/// Combined file for up to 50 documents.
fn build_bulk(
    st: &State,
    nregs: &[String],
    fmt: &str,
    edition: Option<&str>,
) -> Result<(String, String, String)> {
    let mut out = String::new();
    for (i, nreg) in nregs.iter().enumerate() {
        let card = st
            .client
            .card(nreg)
            .with_context(|| format!("card {nreg}"))?;
        let text = fetch_text(st, nreg, edition)?;
        let (body, _) = render_export(nreg, &card, &text, fmt);
        if i > 0 {
            out.push_str("\n\n------------------------------------------------------------\n\n");
        }
        out.push_str(&body);
    }
    let ext = render_export("", &LawCardJson::default(), "", fmt).1;
    let stamp = rada_db::build::now_stamp().replace(' ', "_").replace(':', "");
    let filename = format!("rada-bulk-{stamp}-{ext}");
    Ok((out, ext, filename))
}

fn fetch_text(st: &State, nreg: &str, edition: Option<&str>) -> Result<String> {
    match edition {
        Some(ed) => st
            .client
            .show_edition_text(nreg, ed)
            .map_err(|e| anyhow!("text {nreg}/ed{ed}: {e}")),
        None => st.client.show_text(nreg).map_err(|e| anyhow!("text {nreg}: {e}")),
    }
}

/// Render the export body. `fmt` ∈ `txt` | `md` | `html` (default txt).
pub fn render_export(nreg: &str, card: &LawCardJson, text: &str, fmt: &str) -> (String, String) {
    match fmt {
        "md" => (md_export(nreg, card, text), "md".to_string()),
        "html" => (html_export(card, text), "html".to_string()),
        _ => (text.to_string(), "txt".to_string()),
    }
}

fn md_export(nreg: &str, card: &LawCardJson, text: &str) -> String {
    let mut s = String::with_capacity(text.len() + 1024);
    s.push_str(&format!("# {}\n\n", card.nazva));
    s.push_str(&format!("- **Номер (nreg)**: `{nreg}`\n"));
    s.push_str(&format!("- **Статус**: {}\n", StatusId(card.status).label()));
    if let Some(d) = card.orgdat {
        s.push_str(&format!("- **Прийнято**: {}\n", fmt_yyyymmdd(d)));
    }
    if let Some(n) = card.orgnum.as_deref().filter(|s| !s.is_empty()) {
        s.push_str(&format!("- **Реєстраційний номер**: {n}\n"));
    }
    if let Some(d) = card.poddat {
        s.push_str(&format!("- **Редакція (поточна)**: {}\n", fmt_yyyymmdd(d)));
    }
    s.push_str("\n---\n\n");
    s.push_str(text);
    s
}

fn html_export(card: &LawCardJson, text: &str) -> String {
    let nazva = esc_html(&card.nazva);
    let status = StatusId(card.status).label();
    let poddat = card
        .poddat
        .map(fmt_yyyymmdd)
        .unwrap_or_else(|| "поточна".to_string());
    format!(
        "<!doctype html>\n<html lang=\"uk\"><head><meta charset=\"utf-8\">\n\
         <title>{nazva}</title>\n\
         <style>body{{font-family:system-ui,sans-serif;max-width:72em;margin:2em auto;padding:0 1em}}\
         .text{{white-space:pre-wrap;font-family:inherit;line-height:1.5}}meta{{color:#555}}</style>\n\
         </head>\n<body>\n<h1>{nazva}</h1>\n\
         <p><meta>Статус: {status} · Редакція: {poddat}</p>\n\
         <pre class=\"text\">{text}</pre>\n</body></html>\n",
        text = esc_html(text)
    )
}

fn esc_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Print accent color of a web theme (per-theme print CSS): headings and
/// rules keep the on-screen identity of the picked theme, while the paper
/// stays light (white background, black text) so dark themes print cheaply.
/// Unknown/missing ids fall back to the classical Windows blue.
fn print_accent(id: &str) -> &'static str {
    match id {
        "classical" => "#0078D4",
        "white" => "#1F6FEB",
        "black" => "#8A6D00",
        "invert" => "#7A00CC",
        "matrix" => "#0B7A30",
        "opencode" => "#C25E00",
        "nord" => "#4A7C93",
        "dracula" => "#7A4FCC",
        "solarized-dark" | "solarized-light" => "#268BD2",
        "gruvbox-dark" => "#B5760A",
        "monokai" => "#C0155A",
        "one-dark" => "#B08A3E",
        "tokyo-night" => "#9A7A2E",
        "catppuccin" => "#C2643A",
        "github-dark" => "#9A6A0E",
        _ => "#0078D4",
    }
}

/// Turn a rendered html export into a print view: sensible print margins,
/// the theme's accent color and the browser print dialog on load. Falls
/// back to the input when the document has no `</body>`.
fn print_wrap(doc: &str, theme: &str) -> String {
    let accent = print_accent(theme);
    let inject = format!(
        "<style media=\"print\">@page{{margin:18mm 16mm}}\
         body{{margin:0;max-width:none;background:#FFFFFF;color:#000000}}\
         h1,h2{{color:{accent};border-bottom:1px solid {accent};padding-bottom:2px}}\
         hr{{border:none;border-top:1px solid {accent}}}\
         a{{color:#000000;text-decoration:none}}</style>\n\
         <script>window.addEventListener('load',function(){{\
         setTimeout(function(){{window.print();}},250);}});</script>\n</body></html>"
    );
    match doc.rfind("</body>") {
        Some(i) => format!("{}{}", &doc[..i], inject),
        None => doc.to_string(),
    }
}

/// Append the «Пов'язані документи» section to a rendered print document,
/// before `</body>` (same insertion point as [`print_wrap`]). `related` is
/// `None` when the live fetch failed — the document is then returned
/// unchanged, keeping the print view reliable when the endpoint is down.
/// Offline index titles replace the page anchors (same pattern as
/// `related_handler`); display is capped at 25 entries to keep the print
/// compact, and every string goes through [`esc_html`].
fn print_body_with_related(
    body: &str,
    related: Option<Vec<RelatedDoc>>,
    conn: Option<&Connection>,
) -> String {
    let Some(list) = related.filter(|l| !l.is_empty()) else {
        return body.to_string();
    };
    let mut items = String::new();
    for mut d in list.into_iter().take(25) {
        if let Some(conn) = conn {
            if let Some(t) = rada_db::title_by_nreg(conn, &d.nreg) {
                if !t.trim().is_empty() {
                    d.title = t;
                }
            }
        }
        items.push_str(&format!(
            "<li>{} ({})</li>\n",
            esc_html(d.title.trim()),
            esc_html(&d.nreg),
        ));
    }
    let section = format!("<h2>Пов'язані документи</h2>\n<ul>\n{items}</ul>\n");
    match body.rfind("</body>") {
        Some(i) => format!("{}{}\n{}", &body[..i], section, &body[i..]),
        None => format!("{body}{section}"),
    }
}

/// `YYYYMMDD` → `DD.MM.YYYY` (passthrough when not 8 digits).
fn fmt_yyyymmdd(d: i64) -> String {
    let s = d.to_string();
    if s.len() == 8 && s.chars().all(|c| c.is_ascii_digit()) {
        format!("{}.{}.{}", &s[6..8], &s[4..6], &s[0..4])
    } else {
        s
    }
}

/// Filename-safe document name: keep `[A-Za-z0-9._-]`, everything else → `_`.
pub fn export_filename(nreg: &str, ext: &str) -> String {
    let safe: String = nreg
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("rada-{safe}.{ext}")
}

// ---------------------------------------------------------------------------
// HTTP helpers
// ---------------------------------------------------------------------------

fn read_body(req: &mut Request) -> Result<String> {
    let mut s = String::new();
    req.as_reader().read_to_string(&mut s)?;
    if s.len() > 2_000_000 {
        return Err(anyhow!("request body too large"));
    }
    Ok(s)
}

/// tiny_http 0.12: `Header::from_bytes` returns a Result; all values here
/// are ASCII (filenames are sanitized to `[A-Za-z0-9._-]`).
fn h(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("ascii header value")
}

fn json_ok(v: &Value) -> Resp {
    Response::from_string(v.to_string())
        .with_header(h("Content-Type", "application/json; charset=utf-8"))
}

fn json_err(status: u16, msg: &str) -> Resp {
    Response::from_string(json!({ "error": msg }).to_string())
        .with_status_code(StatusCode(status))
        .with_header(h("Content-Type", "application/json; charset=utf-8"))
}

fn html_response() -> Resp {
    // Serve the page with a Content-Length instead of chunked encoding:
    // tiny_http chunks any body ≥ its 32 KiB threshold by default, and a
    // chunk boundary can cut a multibyte UTF-8 character in half, which
    // breaks naive raw-socket readers (and the loopback smoke test).
    Response::from_data(ui::INDEX_HTML.as_bytes().to_vec())
        .with_chunked_threshold(usize::MAX)
        .with_header(h("Content-Type", "text/html; charset=utf-8"))
}

fn file_response(body: &str, ext: &str, filename: &str) -> Resp {
    let ctype = if ext == "html" {
        "text/html; charset=utf-8"
    } else {
        "text/plain; charset=utf-8"
    };
    Response::from_string(body.to_string())
        .with_header(h("Content-Type", ctype))
        .with_header(h(
            "Content-Disposition",
            &format!("attachment; filename=\"{filename}\""),
        ))
}

/// Percent-decode; `plus_as_space` treats `+` as a literal space (query
/// strings from the browser's `URLSearchParams`).
fn decode_component(s: &str, plus_as_space: bool) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' if plus_as_space => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                match u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    Ok(h) => {
                        out.push(h);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_query(s: &str) -> Vec<(String, String)> {
    s.split('&')
        .filter(|p| !p.is_empty())
        .map(|pair| {
            let (k, v) = pair.split_once('=').map_or_else(
                || (pair.to_string(), String::new()),
                |(k, v)| (k.to_string(), v.to_string()),
            );
            (
                decode_component(&k, true),
                decode_component(&v, true),
            )
        })
        .collect()
}

fn qget<'a>(q: &'a [(String, String)], key: &str) -> Option<&'a str> {
    q.iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rada_core::models::StatusId;
    use tiny_http::TestRequest;

    #[test]
    fn decodes_cyrillic_and_slash() {
        assert_eq!(
            decode_component("254%D0%BA/96-%D0%B2%D1%80", false),
            "254к/96-вр"
        );
        // %2F decodes to a literal slash (JS encodes nreg per segment, so
        // both shapes round-trip).
        assert_eq!(decode_component("%2F", false), "/");
        assert_eq!(decode_component("a+b", true), "a b");
        assert_eq!(decode_component("a+b", false), "a+b");
        assert_eq!(decode_component("a%41", false), "aA");
    }

    #[test]
    fn parse_query_splits_and_decodes() {
        let q = parse_query("q=%D0%BA%D0%BE%D0%B4&limit=20&status=");
        assert_eq!(q.len(), 3);
        assert_eq!(q[0].1, "код");
        assert_eq!(q[1].1, "20");
        assert_eq!(q[2].1, "");
    }

    #[test]
    fn export_filename_sanitizes() {
        assert_eq!(export_filename("1178-2022-п", "md"), "rada-1178-2022-_.md");
        assert_eq!(export_filename("254к/96-вр", "txt"), "rada-254__96-__.txt");
        assert_eq!(export_filename("2041-15", "html"), "rada-2041-15.html");
    }

    #[test]
    fn render_export_txt_passthrough() {
        let card = LawCardJson::default();
        let (body, ext) = render_export("1-1", &card, "текст", "txt");
        assert_eq!(body, "текст");
        assert_eq!(ext, "txt");
    }

    #[test]
    fn render_export_md_has_metadata() {
        let card = LawCardJson {
            nreg: "1-1".into(),
            nazva: "Тестовий закон".into(),
            status: 5,
            orgdat: Some(20261001),
            poddat: Some(20261015),
            ..Default::default()
        };
        let (body, ext) = render_export("1-1", &card, "стаття 1", "md");
        assert_eq!(ext, "md");
        assert!(body.starts_with("# Тестовий закон\n"));
        assert!(body.contains("**Статус**: Чинний"));
        assert!(body.contains("стаття 1"));
        assert!(body.contains("15.10.2026"));
        // status label is the dictionary label for 5
        assert_eq!(StatusId(5).label(), "Чинний");
    }

    #[test]
    fn render_export_html_escapes() {
        let card = LawCardJson {
            nazva: "<b>Закон</b>".into(),
            ..Default::default()
        };
        let (body, ext) = render_export("1-1", &card, "a < b & c > d", "html");
        assert_eq!(ext, "html");
        assert!(body.contains("&lt;b&gt;Закон&lt;/b&gt;"));
        assert!(body.contains("a &lt; b &amp; c &gt; d"));
        assert!(body.contains("<!doctype html>"));
    }

    #[test]
    fn ui_page_has_parity_features() {
        // Gold highlight rendering, find-in-text bar and copy-identifier
        // buttons must be present in the embedded page (GUI parity).
        assert!(ui::INDEX_HTML.contains("<mark class=\"hl\">"));
        assert!(ui::INDEX_HTML.contains("id=\"find-input\""));
        assert!(ui::INDEX_HTML.contains("id=\"find-count\""));
        assert!(ui::INDEX_HTML.contains("id=\"find-next\""));
        assert!(ui::INDEX_HTML.contains("id=\"btn-copy-nreg\""));
        assert!(ui::INDEX_HTML.contains("execCommand"));
        // Recent-documents row (localStorage `rada-recent`) and the Ctrl+F
        // shortcut that redirects browser find into the reader's find bar.
        assert!(ui::INDEX_HTML.contains("id=\"rada-recent\""));
        assert!(ui::INDEX_HTML.contains("// Ctrl+F / Cmd+F opens the document find bar instead of the browser find."));
        // Web-parity gaps closed: live nreg resolver (search tab), retry
        // buttons for failed card/text loads, inline annotation edit, and
        // the «Видавник» org filter (the server accepts `org=`).
        assert!(ui::INDEX_HTML.contains("id=\"btn-resolve\""));
        assert!(ui::INDEX_HTML.contains("id=\"retry-card\""));
        assert!(ui::INDEX_HTML.contains("змінити"));
        assert!(ui::INDEX_HTML.contains("id=\"f-org\""));
        // Theme system (the GUI's 17-theme color parity): CSS-variable
        // palettes, the «Тема» picker, and localStorage persistence.
        assert!(ui::INDEX_HTML.contains("id=\"theme-select\""));
        assert!(ui::INDEX_HTML.contains("solarized-light"));
        assert!(ui::INDEX_HTML.contains("rada-theme"));
        // Structured document-text rendering: the act text is split into
        // classified per-line blocks (document title, article headings,
        // sections, numbered paragraphs, amendment metadata).
        assert!(ui::INDEX_HTML.contains("p-doctitle"));
        assert!(ui::INDEX_HTML.contains("p-subtitle"));
        assert!(ui::INDEX_HTML.contains("p-section"));
        assert!(ui::INDEX_HTML.contains("p-article"));
        assert!(ui::INDEX_HTML.contains("p-num"));
        assert!(ui::INDEX_HTML.contains("classifyLine"));
        // Print: the reader's «Друк» button opens the auto-print view.
        assert!(ui::INDEX_HTML.contains("id=\"btn-print\""));
        assert!(ui::INDEX_HTML.contains("?print=1"));
        // Plain-text mode: the «Простий» toggle and its flattened styles.
        assert!(ui::INDEX_HTML.contains("id=\"btn-raw\""));
        assert!(ui::INDEX_HTML.contains("#doc-text.raw"));
        assert!(ui::INDEX_HTML.contains("rada-raw"));
    }

    // ---- background data jobs («Дані») ---------------------------------------

    fn test_state() -> Arc<State> {
        let dir = std::env::temp_dir().join("rada-web-jobs-test");
        Arc::new(State {
            db_path: dir.join("test.sqlite"),
            // Long pacing so nothing can hammer the API even by accident;
            // these tests never start a real (network) job.
            client: RadaClient::with_pacing(Duration::from_secs(3600)),
            pace_secs: 3600,
            jobs: Mutex::new(HashMap::new()),
            job_seq: AtomicU64::new(0),
        })
    }

    /// Drive `route` with a synthetic request and read the JSON body back.
    fn call(st: &Arc<State>, method: Method, path: &str, body: &'static str) -> (u16, Value) {
        let mut req: Request = TestRequest::new()
            .with_method(method)
            .with_path(path)
            .with_body(body)
            .into();
        let method = req.method().clone();
        let resp = route(&method, path, &[], &mut req, st).expect("route");
        let code = resp.status_code().0;
        let bytes = resp.into_reader().into_inner();
        let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (code, v)
    }

    #[test]
    fn job_record_serializes_expected_shape() {
        let rec = JobRecord {
            id: "sync-3".into(),
            kind: "sync".into(),
            state: "running".into(),
            line: "оновлюю словники …".into(),
            result: None,
            updated: 3,
        };
        let v = serde_json::to_value(&rec).unwrap();
        assert_eq!(v["id"], "sync-3");
        assert_eq!(v["kind"], "sync");
        assert_eq!(v["state"], "running");
        assert_eq!(v["line"], "оновлюю словники …");
        assert!(v["result"].is_null());
        assert_eq!(v["updated"], 3);
    }

    #[test]
    fn job_registry_claims_one_per_kind() {
        let st = test_state();
        let id = claim_job(&st, "sync").expect("first sync claim");
        assert!(id.starts_with("sync-"));
        assert!(claim_job(&st, "sync").is_none(), "second concurrent sync rejected");
        let snap = claim_job(&st, "snapshot").expect("snapshot is an independent kind");
        // Finishing a job frees its kind (new id, old record kept for the UI).
        st.job_set(&id, "done", "готово", Some("ok"));
        let again = claim_job(&st, "sync").expect("done job frees the kind");
        assert_ne!(again, id);
        st.job_set(&snap, "error", "помилка", Some("boom"));
        assert!(claim_job(&st, "snapshot").is_some());
        let jobs = st.jobs_lock();
        assert_eq!(jobs.len(), 4);
        assert_eq!(jobs[&id].result.as_deref(), Some("ok"));
    }

    #[test]
    fn jobs_routes_registered() {
        let st = test_state();
        // GET /api/jobs — empty registry, list shape.
        let (code, v) = call(&st, Method::Get, "/api/jobs", "");
        assert_eq!(code, 200);
        assert_eq!(v["jobs"].as_array().unwrap().len(), 0);
        // A running job of the kind ⇒ POST answers 409 (JSON error shape),
        // no second job is started.
        let id = claim_job(&st, "sync").unwrap();
        let (code, v) = call(&st, Method::Post, "/api/jobs/sync", "");
        assert_eq!(code, 409);
        assert!(v["error"].as_str().unwrap().contains("sync"));
        // Both kinds are guarded while running (409, JSON error shape).
        let _snap = claim_job(&st, "snapshot").unwrap();
        let (code, v) = call(&st, Method::Post, "/api/jobs/snapshot", "");
        assert_eq!(code, 409);
        assert!(v["error"].as_str().unwrap().contains("snapshot"));
        // The registry lists both running jobs with the UI-visible fields.
        let (code, v) = call(&st, Method::Get, "/api/jobs", "");
        assert_eq!(code, 200);
        let jobs = v["jobs"].as_array().unwrap();
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0]["id"], id);
        assert_eq!(jobs[0]["kind"], "sync");
        assert_eq!(jobs[0]["state"], "running");
        assert!(jobs[0]["line"].is_string());
        assert!(jobs[0]["result"].is_null());
        assert_eq!(jobs[1]["kind"], "snapshot");
        // Method / path mistakes match the existing error style.
        assert_eq!(call(&st, Method::Get, "/api/jobs/sync", "").0, 405);
        assert_eq!(call(&st, Method::Post, "/api/jobs", "").0, 405);
        let (code, v) = call(&st, Method::Get, "/api/jobs/nope", "");
        assert_eq!(code, 404);
        assert!(v["error"].as_str().unwrap().contains("/api/jobs/nope"));
    }

    /// `POST /api/jobs/{kind}` success shape via the shared start path, with
    /// a fake runner so no live API / GitHub traffic happens in tests.
    #[test]
    fn start_job_endpoint_shape() {
        fn fake_runner(_st: Arc<State>, _id: String) {}
        let st = test_state();
        let resp = start_job_with(&st, "sync", fake_runner);
        assert_eq!(resp.status_code().0, 200);
        let v: Value = serde_json::from_slice(&resp.into_reader().into_inner()).unwrap();
        assert_eq!(v["kind"], "sync");
        assert_eq!(v["state"], "running");
        let id = v["id"].as_str().unwrap().to_string();
        assert!(id.starts_with("sync-"));
        // The registry now carries the running job (the fake runner leaves
        // it `running`, which is also what the second POST below sees).
        assert_eq!(st.jobs_lock()[&id].state, "running");
        assert!(claim_job(&st, "sync").is_none());
    }

    /// End-to-end smoke over a real loopback socket: the whole
    /// `Server → handle_request → route` path must answer the new endpoints
    /// and serve the page with the «Дані» area.
    #[test]
    fn serve_smoke_jobs_and_page() {
        use std::io::{Read, Write};
        let server = Server::http("127.0.0.1:0").expect("bind ephemeral port");
        let port = server
            .server_addr()
            .to_ip()
            .expect("plain IP listener")
            .port();
        let st = test_state();
        let worker = std::thread::spawn(move || {
            for req in server.incoming_requests().take(2) {
                if handle_request(req, &st).is_err() {
                    break;
                }
            }
        });
        let get = |path: &str| -> String {
            let mut s = String::new();
            if let Ok(mut sock) = std::net::TcpStream::connect(("127.0.0.1", port)) {
                let _ = sock.write_all(
                    format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
                        .as_bytes(),
                );
                let _ = sock.read_to_string(&mut s);
            }
            s
        };
        let body = get("/api/jobs");
        assert!(body.starts_with("HTTP/1.1 200 OK"), "jobs list: {body}");
        assert!(body.contains("\"jobs\":[]"), "jobs list: {body}");
        let page = get("/");
        assert!(page.starts_with("HTTP/1.1 200 OK"), "index page");
        assert!(page.contains("id=\"btn-jobs-sync\""), "index page has the jobs buttons");
        worker.join().expect("worker");
    }

    #[test]
    fn ui_page_has_data_jobs_area() {
        // «Дані» area: both job buttons, the status line, and the polling
        // client against the new endpoints must be present in the page.
        assert!(ui::INDEX_HTML.contains("id=\"btn-jobs-sync\""));
        assert!(ui::INDEX_HTML.contains("id=\"btn-jobs-snapshot\""));
        assert!(ui::INDEX_HTML.contains("id=\"jobs-status\""));
        assert!(ui::INDEX_HTML.contains("id=\"jobs-list\""));
        assert!(ui::INDEX_HTML.contains("Оновити наживо"));
        assert!(ui::INDEX_HTML.contains("Отримати снапшот"));
        assert!(ui::INDEX_HTML.contains("\"/api/jobs/\""));
        assert!(ui::INDEX_HTML.contains("/api/jobs\""));
        assert!(ui::INDEX_HTML.contains("startDataJob"));
        assert!(ui::INDEX_HTML.contains("refreshJobs"));
        // «Резервна копія» sub-area (GUI backup/restore parity): the create
        // button, the backup picker + refresh, the restore button, its own
        // status line, and the client against the new endpoints.
        assert!(ui::INDEX_HTML.contains("id=\"btn-backup-create\""));
        assert!(ui::INDEX_HTML.contains("id=\"backup-list\""));
        assert!(ui::INDEX_HTML.contains("id=\"btn-backup-refresh\""));
        assert!(ui::INDEX_HTML.contains("id=\"btn-backup-restore\""));
        assert!(ui::INDEX_HTML.contains("id=\"backup-status\""));
        assert!(ui::INDEX_HTML.contains("Зберегти резервну копію"));
        assert!(ui::INDEX_HTML.contains("Відновити з файлу"));
        assert!(ui::INDEX_HTML.contains("/api/backups"));
        assert!(ui::INDEX_HTML.contains("loadBackups"));
        // «Повна побудова індексу» sub-area: the rebuild button, its
        // Ukrainian labels, and the client against the new endpoint.
        assert!(ui::INDEX_HTML.contains("id=\"btn-jobs-init\""));
        assert!(ui::INDEX_HTML.contains("Повна побудова індексу"));
        assert!(ui::INDEX_HTML.contains("Побудувати з офіційного дампу"));
        assert!(ui::INDEX_HTML.contains("/api/jobs/init"));
    }

    /// The «Дані» job row surfaces the record's live progress `line` while
    /// the job runs (muted `hint` span, escHtml-escaped), finished jobs keep
    /// showing `result`, and an empty line renders nothing at all.
    #[test]
    fn ui_page_has_job_progress_line() {
        let start = ui::INDEX_HTML
            .find("function renderJobRow(j) {")
            .expect("job row renderer present");
        let end = start
            + ui::INDEX_HTML[start..]
                .find("async function refreshJobs")
                .expect("renderer defined before refreshJobs");
        let body = &ui::INDEX_HTML[start..end];
        assert!(
            body.contains("j.state === \"running\""),
            "live line is shown only while the job runs"
        );
        assert!(body.contains("escHtml(j.line)"), "server line is escHtml-escaped");
        assert!(
            body.contains("class=\"hint job-line\""),
            "line reuses the muted hint styling"
        );
        assert!(body.contains("if (j.line)"), "empty line renders nothing");
        assert!(body.contains("escHtml(j.kind)"), "kind is escaped");
        assert!(body.contains("escHtml(j.state)"), "state is escaped");
        assert!(body.contains("escHtml(j.result)"), "result is escaped");
        // Finished jobs still show the outcome message (existing behavior).
        assert!(body.contains("else if (j.result)"), "done/error rows show result");
    }

    /// The existing jobs poll feeds the line repaint: `pollJobs` keeps its
    /// single 2 s timer, each tick runs `refreshJobs`, which pulls the
    /// `/api/jobs` payload and hands every record (with its `.line`) to
    /// `renderJobRow`. No new interval is introduced anywhere in the area.
    #[test]
    fn ui_page_has_job_progress_polled() {
        let poll_at = ui::INDEX_HTML
            .find("function pollJobs() {")
            .expect("poll loop present");
        let poll_body = &ui::INDEX_HTML[poll_at..poll_at + 400];
        assert!(poll_body.contains("setInterval("), "poll keeps its timer");
        assert!(poll_body.contains("refreshJobs()"), "each tick refreshes jobs");
        assert!(poll_body.contains("2000"), "cadence unchanged (2 s)");

        let start = ui::INDEX_HTML
            .find("async function refreshJobs() {")
            .expect("refresh present");
        let end = start
            + ui::INDEX_HTML[start..]
                .find("function onJobFinished")
                .expect("refresh ends at onJobFinished");
        let body = &ui::INDEX_HTML[start..end];
        assert!(body.contains("\"/api/jobs\""), "poll reads the job registry");
        assert!(body.contains("renderJobRow(j)"), "payload records reach the renderer");
        assert!(!body.contains("setInterval("), "refresh itself schedules nothing");

        // The whole jobs area owns exactly one timer (the pre-existing poll).
        let area_at = ui::INDEX_HTML
            .find("// ---- background data jobs")
            .expect("jobs area marker");
        let area_end = ui::INDEX_HTML[area_at..]
            .find("// ---- backup / restore")
            .expect("backup area marker")
            + area_at;
        assert_eq!(
            ui::INDEX_HTML[area_at..area_end].matches("setInterval(").count(),
            1,
            "no new intervals: the line repaint reuses the existing jobs poll"
        );
    }

    // ---- full-dump index build («Повна побудова індексу») --------------------

    /// `POST /api/jobs/init` shape through the shared start path. The real
    /// runner would download the ~51 MB official dump, so tests only ever
    /// inject a fake one (exactly like the snapshot start test does).
    #[test]
    fn init_job_endpoint_shape() {
        fn fake_runner(_st: Arc<State>, _id: String) {}
        let st = test_state();
        // A running init answers 409 with the JSON error shape (the route
        // is registered: not 404), and GET is a 405.
        let running = claim_job(&st, "init").expect("first init claim");
        let (code, v) = call(&st, Method::Post, "/api/jobs/init", "");
        assert_eq!(code, 409);
        assert!(v["error"].as_str().unwrap().contains("init"));
        assert_eq!(call(&st, Method::Get, "/api/jobs/init", "").0, 405);
        // Finish it, then check the POST success shape via the fake runner.
        st.job_set(&running, "done", "готово", Some("ok"));
        let resp = start_job_with(&st, "init", fake_runner);
        assert_eq!(resp.status_code().0, 200);
        let v: Value = serde_json::from_slice(&resp.into_reader().into_inner()).unwrap();
        assert_eq!(v["kind"], "init");
        assert_eq!(v["state"], "running");
        let id = v["id"].as_str().unwrap().to_string();
        assert!(id.starts_with("init-"));
        assert_eq!(st.jobs_lock()[&id].state, "running");
    }

    /// The init job is mutually exclusive with every db-replacing job
    /// (snapshot / backup / restore), in both directions.
    #[test]
    fn init_job_single_flight() {
        fn fake_runner(_st: Arc<State>, _id: String) {}
        let st = test_state();
        // A running snapshot blocks init (409, JSON error shape).
        let snap = claim_job(&st, "snapshot").expect("first snapshot claim");
        let (code, v) = call(&st, Method::Post, "/api/jobs/init", "");
        assert_eq!(code, 409);
        assert!(v["error"].as_str().unwrap().contains("init"));
        st.job_set(&snap, "done", "готово", Some("ok"));
        // A running init blocks the other db-replacing kinds.
        let init = claim_job(&st, "init").expect("init claim once snapshot is done");
        let (code, v) = call(&st, Method::Post, "/api/jobs/snapshot", "");
        assert_eq!(code, 409);
        assert!(v["error"].as_str().unwrap().contains("snapshot"));
        assert_eq!(call(&st, Method::Post, "/api/jobs/backup", "").0, 409);
        let (code, v) = call(&st, Method::Post, "/api/jobs/restore", r#"{"file":"x.sqlite.gz"}"#);
        // Restore validates first, so an unknown file still answers 400 …
        assert_eq!(code, 400);
        assert!(v["error"].is_string());
        // … and an independent kind (sync) is untouched by the init guard.
        let resp = start_job_with(&st, "sync", fake_runner);
        assert_eq!(resp.status_code().0, 200);
        drop(resp);
        st.job_set(&init, "done", "готово", Some("ok"));
        // A finished init frees the db-replacing kinds again.
        let resp = start_job_with(&st, "snapshot", fake_runner);
        assert_eq!(resp.status_code().0, 200);
        let v: Value = serde_json::from_slice(&resp.into_reader().into_inner()).unwrap();
        assert_eq!(v["kind"], "snapshot");
    }

    // ---- backup / restore jobs («Резервна копія») ----------------------------

    /// Fresh indexed db for one backup/restore test: migrated schema + one
    /// titled document, unique dir per tag so parallel tests never collide.
    fn backups_state(tag: &str) -> Arc<State> {
        let dir = std::env::temp_dir().join(format!("rada-web-backup-test-{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create test dir");
        let st = Arc::new(State {
            db_path: dir.join("test.sqlite"),
            client: RadaClient::with_pacing(Duration::from_secs(3600)),
            pace_secs: 3600,
            jobs: Mutex::new(HashMap::new()),
            job_seq: AtomicU64::new(0),
        });
        let conn = db_open(&st.db_path).expect("open test index");
        conn.execute(
            "INSERT INTO docs(nreg, nazva) VALUES('2341-14', 'Про тестовий закон')",
            [],
        )
        .expect("seed doc");
        drop(conn);
        st
    }

    /// Basenames of `*.sqlite.gz` files currently in the state's backups dir.
    fn backup_files(st: &State) -> Vec<String> {
        std::fs::read_dir(backups_dir(st))
            .expect("backups dir exists")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".sqlite.gz"))
            .collect()
    }

    #[test]
    fn backup_job_creates_file() {
        let st = backups_state("create");
        // While one backup runs the POST answers 409 (JSON error shape) —
        // the shared claim_job guard covers the new kinds too.
        let running = claim_job(&st, "backup").expect("first backup claim");
        let (code, v) = call(&st, Method::Post, "/api/jobs/backup", "");
        assert_eq!(code, 409);
        assert!(v["error"].as_str().unwrap().contains("backup"));
        // Finish it, then run the real (pure-local) job body synchronously.
        st.job_set(&running, "done", "готово", Some("ok"));
        let id = claim_job(&st, "backup").unwrap();
        run_backup_job(st.clone(), id.clone());
        // Copy the record out: the registry lock must be free again before
        // the `start_job_with` shape check below claims it once more.
        let (state, msg) = {
            let jobs = st.jobs_lock();
            (jobs[&id].state.clone(), jobs[&id].result.clone())
        };
        assert_eq!(state, "done", "result: {msg:?}");
        let msg = msg.unwrap();
        let gz = backup_files(&st);
        assert_eq!(gz.len(), 1, "exactly one backup written: {gz:?}");
        assert!(gz[0].starts_with("rada-backup-") && gz[0].ends_with(".sqlite.gz"));
        // The message names the file and reports its size.
        assert!(msg.contains(&gz[0]), "message {msg:?} must name {}", gz[0]);
        assert!(msg.contains("МБ"), "message {msg:?} must report the size");
        // POST success shape via the shared start path (fake runner so no
        // second file / thread races the assertions above).
        fn fake_runner(_st: Arc<State>, _id: String) {}
        let resp = start_job_with(&st, "backup", fake_runner);
        assert_eq!(resp.status_code().0, 200);
        let v: Value = serde_json::from_slice(&resp.into_reader().into_inner()).unwrap();
        assert_eq!(v["kind"], "backup");
        assert_eq!(v["state"], "running");
        assert!(v["id"].as_str().unwrap().starts_with("backup-"));
    }

    #[test]
    fn restore_job_validates_input() {
        let st = backups_state("validate");
        std::fs::create_dir_all(backups_dir(&st)).unwrap();
        std::fs::write(backups_dir(&st).join("good.sqlite.gz"), b"gz").unwrap();
        // Traversal, separators, missing files, wrong extensions and junk
        // bodies all answer 400 with the shared JSON error shape.
        for body in [
            r#"{"file":"../x.sqlite.gz"}"#,
            r#"{"file":"..\\..\\secret.sqlite.gz"}"#,
            r#"{"file":"sub/dir.sqlite.gz"}"#,
            r#"{"file":"missing.sqlite.gz"}"#,
            r#"{"file":"notes.txt"}"#,
            r#"{"file":"good.tar.gz"}"#,
            r#"{}"#,
            "не json",
        ] {
            let (code, v) = call(&st, Method::Post, "/api/jobs/restore", body);
            assert_eq!(code, 400, "body {body}");
            assert!(v["error"].is_string(), "body {body}");
        }
        // A running restore occupies the single-flight guard (409), while
        // validation still rejects bad input with 400 in parallel.
        let id = claim_job(&st, "restore").unwrap();
        let (code, v) = call(&st, Method::Post, "/api/jobs/restore", r#"{"file":"good.sqlite.gz"}"#);
        assert_eq!(code, 409);
        assert!(v["error"].as_str().unwrap().contains("restore"));
        assert_eq!(
            call(&st, Method::Post, "/api/jobs/restore", r#"{"file":"../x.sqlite.gz"}"#).0,
            400
        );
        st.job_set(&id, "done", "готово", Some("ok"));
        // Method / path mistakes match the existing error style.
        assert_eq!(call(&st, Method::Get, "/api/jobs/restore", "").0, 405);
        assert_eq!(call(&st, Method::Get, "/api/jobs/backup", "").0, 405);
        assert_eq!(call(&st, Method::Post, "/api/backups", "").0, 405);
        let (code, v) = call(&st, Method::Get, "/api/jobs/nope", "");
        assert_eq!(code, 404);
        assert!(v["error"].as_str().unwrap().contains("/api/jobs/nope"));
    }

    /// Full local round trip: back the seeded db up, change the live db,
    /// restore, and check the swap + annotation re-attach (GUI semantics).
    #[test]
    fn restore_job_round_trip() {
        let st = backups_state("roundtrip");
        let id = claim_job(&st, "backup").unwrap();
        run_backup_job(st.clone(), id.clone());
        assert_eq!(st.jobs_lock()[&id].state, "done");
        let name = backup_files(&st).into_iter().next().expect("one backup");
        // Post-backup changes: an annotation and a doc the backup cannot know.
        let conn = db_open(&st.db_path).unwrap();
        rada_db::annotations::insert(&conn, "2341-14", "comment", "треба зберегти", None, None)
            .unwrap();
        conn.execute("INSERT INTO docs(nreg, nazva) VALUES('9999-99', 'Зайвий')", [])
            .unwrap();
        drop(conn);
        let rid = claim_job(&st, "restore").unwrap();
        run_restore_job(&st, &rid, &name);
        let (state, msg) = {
            let jobs = st.jobs_lock();
            (jobs[&rid].state.clone(), jobs[&rid].result.clone())
        };
        assert_eq!(state, "done", "result: {msg:?}");
        let msg = msg.unwrap();
        assert!(msg.contains(&name), "message {msg:?} must name the file");
        assert!(msg.contains("анотацій повернуто: 1"), "message {msg:?}");
        let conn = db_open(&st.db_path).unwrap();
        let docs: i64 = conn
            .query_row("SELECT COUNT(*) FROM docs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(docs, 1, "post-backup doc is gone (db replaced by the backup)");
        let anns = rada_db::annotations::all(&conn).unwrap();
        assert_eq!(anns.len(), 1, "annotations re-attached after the swap");
        assert_eq!(anns[0].text, "треба зберегти");
    }

    #[test]
    fn backups_listing_endpoint() {
        let st = backups_state("list");
        // Missing directory behaves like an empty one.
        let (code, v) = call(&st, Method::Get, "/api/backups", "");
        assert_eq!(code, 200);
        assert_eq!(v["backups"].as_array().unwrap().len(), 0);
        // Only *.sqlite.gz *files* are listed, with the UI's fields.
        let dir = backups_dir(&st);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("x.sqlite.gz"), b"gz-bytes").unwrap();
        std::fs::write(dir.join("y.txt"), b"txt").unwrap();
        std::fs::write(dir.join("z.sqlite"), b"db").unwrap();
        std::fs::create_dir_all(dir.join("dir.sqlite.gz")).unwrap();
        let (code, v) = call(&st, Method::Get, "/api/backups", "");
        assert_eq!(code, 200);
        let items = v["backups"].as_array().unwrap();
        assert_eq!(items.len(), 1, "only x.sqlite.gz listed: {items:?}");
        assert_eq!(items[0]["name"], "x.sqlite.gz");
        assert!(items[0]["size_mb"].is_number());
        assert!(items[0]["created"].is_number());
        assert_eq!(items[0].as_object().unwrap().len(), 3);
    }

    // ---- «Обране» (favorites): /api/annotations/all|export|import ----------

    /// Routing + validation of GET /api/related/{nreg} without touching the
    /// paced network client (the happy path is covered by the core extractor
    /// tests + the ignored live test).
    #[test]
    fn related_route_shape() {
        let st = favorites_state("related");
        let (code, _) = call(&st, Method::Post, "/api/related/2341-14", "");
        assert_eq!(code, 405);
        let (code, v) = call(&st, Method::Get, "/api/related/", "");
        assert_eq!(code, 400, "{v}");
        let (code, v) = call(&st, Method::Get, "/api/related", "");
        assert_eq!(code, 400, "{v}");
    }

    /// Offline full-text cache search endpoint: shape, validation, ranking
    /// join with the index title. No network (docs_text is seeded directly).
    #[test]
    fn cache_search_endpoint() {
        let st = favorites_state("cache");
        {
            let conn = db_open(&st.db_path).unwrap();
            rada_db::textcache::put(
                &conn,
                "2341-14",
                "",
                "Стаття 1. КРИМІНАЛЬНА відповідальність. КРИМІНАЛЬНИЙ кодекс.",
            )
            .unwrap();
            rada_db::textcache::put(&conn, "9-9", "", "текст без ключового слова")
                .unwrap();
        }
        // Query-aware call (the plain `call` helper sends no params).
        let get = |path: &str, query: &str| -> (u16, Value) {
            let mut req: Request = TestRequest::new()
                .with_method(Method::Get)
                .with_path(path)
                .into();
            let resp = route(&Method::Get, path, &parse_query(query), &mut req, &st).unwrap();
            let code = resp.status_code().0;
            let bytes = resp.into_reader().into_inner();
            (code, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
        };
        let (code, _) = call(&st, Method::Post, "/api/cache/search", "");
        assert_eq!(code, 405);
        let (code, v) = get("/api/cache/search", "");
        assert_eq!(code, 400, "{v}");
        let (code, v) = get("/api/cache/nope", "q=x");
        assert_eq!(code, 404, "{v}");
        let (code, v) = get("/api/cache/search", "q=%D0%BA%D1%80%D0%B8%D0%BC%D1%96%D0%BD%D0%B0%D0%BB%D1%8C%D0%BD");
        assert_eq!(code, 200, "{v}");
        let items = v["items"].as_array().unwrap();
        assert_eq!(items.len(), 1, "only the matching doc: {items:?}");
        assert_eq!(items[0]["nreg"], "2341-14");
        assert_eq!(items[0]["title"], "Про тестовий закон", "index title join");
        assert_eq!(items[0]["hits"], 2, "two folded occurrences");
        assert!(items[0]["snippet"]
            .as_str()
            .unwrap()
            .contains("КРИМІНАЛЬНА"));
    }

    /// Fresh indexed db for one favorites test: migrated schema + one titled
    /// document (`2341-14`), so title joins are observable. Unique dir per
    /// tag keeps parallel tests from colliding.
    fn favorites_state(tag: &str) -> Arc<State> {
        let dir = std::env::temp_dir().join(format!("rada-web-fav-test-{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        let st = Arc::new(State {
            db_path: dir.join("test.sqlite"),
            client: RadaClient::with_pacing(Duration::from_secs(3600)),
            pace_secs: 3600,
            jobs: Mutex::new(HashMap::new()),
            job_seq: AtomicU64::new(0),
        });
        let conn = db_open(&st.db_path).expect("open test index");
        conn.execute(
            "INSERT INTO docs(nreg, nazva) VALUES('2341-14', 'Про тестовий закон')",
            [],
        )
        .expect("seed doc");
        drop(conn);
        st
    }

    #[test]
    fn annotations_all_bookmark_preview_from_cache() {
        let st = favorites_state("prev");
        let conn = db_open(&st.db_path).unwrap();
        rada_db::annotations::insert(&conn, "2341-14", "bookmark", "", None, None).unwrap();
        let long = format!("Початок документа. {}", "Довгий текст статті. ".repeat(40));
        rada_db::textcache::put(&conn, "2341-14", "", &long).unwrap();
        rada_db::annotations::insert(&conn, "2341-14", "comment", "є текст", None, None).unwrap();
        drop(conn);
        let (code, v) = call(&st, Method::Get, "/api/annotations/all", "");
        assert_eq!(code, 200);
        let items = v["items"].as_array().unwrap();
        // [0] = the comment (newest): preview suppressed when text exists.
        assert_eq!(items[0]["kind"], "comment");
        assert!(items[0]["preview"].is_null(), "{}", items[0]);
        // [1] = the bare bookmark: cached opening as the preview.
        assert_eq!(items[1]["kind"], "bookmark");
        let p = items[1]["preview"].as_str().unwrap();
        assert!(p.starts_with("Початок документа."), "{p}");
        assert!(p.ends_with('…'), "long body gets an ellipsis: {p}");
    }

    #[test]
    fn annotations_all_route_shape() {
        let st = favorites_state("all");
        let conn = db_open(&st.db_path).unwrap();
        let c1 = rada_db::annotations::insert(&conn, "2341-14", "comment", "коментар", None, None).unwrap();
        let b1 = rada_db::annotations::insert(&conn, "2341-14", "bookmark", "", None, None).unwrap();
        let h1 = rada_db::annotations::insert(&conn, "0000-00", "highlight", "виділення", Some(1), Some(9)).unwrap();
        drop(conn);
        let (code, v) = call(&st, Method::Get, "/api/annotations/all", "");
        assert_eq!(code, 200);
        let items = v["items"].as_array().unwrap();
        assert_eq!(items.len(), 3);
        // Newest first (ids descend).
        assert_eq!(items[0]["id"], h1);
        assert_eq!(items[1]["id"], b1);
        assert_eq!(items[2]["id"], c1);
        // Exact JSON shape of one item.
        for key in ["id", "nreg", "title", "kind", "text", "created_at", "preview"] {
            assert!(items[0].get(key).is_some(), "missing key {key}");
        }
        assert_eq!(items[0].as_object().unwrap().len(), 7);
        assert_eq!(items[0]["nreg"], "0000-00");
        assert_eq!(items[0]["kind"], "highlight");
        assert_eq!(items[0]["text"], "виділення");
        // Title from the docs lookup; empty string for a missing document.
        assert_eq!(items[0]["title"], "");
        assert_eq!(items[2]["title"], "Про тестовий закон");
        assert!(items[1]["created_at"].as_str().unwrap().len() >= 19);
        // Wrong method matches the shared error style.
        let (code, v) = call(&st, Method::Post, "/api/annotations/all", "");
        assert_eq!(code, 405);
        assert!(v["error"].as_str().unwrap().contains("method"));
    }

    #[test]
    fn print_wrap_injects_dialog_and_survives_without_body() {
        let doc = "<!doctype html><html><head></head><body><pre>x</pre></body></html>";
        let w = print_wrap(doc, "");
        assert!(w.contains("window.print()"), "{w}");
        assert!(w.contains("@page"), "{w}");
        assert!(w.starts_with("<!doctype html><html><head></head><body><pre>x</pre>"));
        assert!(w.ends_with("</body></html>"), "{w}");
        // No </body>: returned unchanged.
        assert_eq!(print_wrap("<p>q</p>", ""), "<p>q</p>");
    }

    #[test]
    fn print_accent_follows_theme_and_print_wrap_uses_it() {
        // Known themes get their own accent, unknown ones the classical blue.
        assert_eq!(print_accent("matrix"), "#0B7A30");
        assert_eq!(print_accent("white"), "#1F6FEB");
        assert_eq!(print_accent("classical"), "#0078D4");
        assert_eq!(print_accent("nonsense"), "#0078D4");
        assert_eq!(print_accent(""), "#0078D4");
        let doc = "<html><body><h1>т</h1></body></html>";
        let w = print_wrap(doc, "matrix");
        assert!(w.contains("#0B7A30"), "{w}");
        assert!(w.contains("background:#FFFFFF"), "paper stays light: {w}");
    }

    fn related_docs(pairs: &[(&str, &str)]) -> Vec<RelatedDoc> {
        pairs
            .iter()
            .map(|(nreg, title)| RelatedDoc {
                nreg: nreg.to_string(),
                title: title.to_string(),
            })
            .collect()
    }

    /// Degraded / offline path: no related list (live fetch failed) or an
    /// empty one ⇒ the print document is returned unchanged — no section,
    /// no failure. The happy path is covered by the core extractor tests;
    /// the paced (3600 s) test client is never called here.
    #[test]
    fn print_view_omits_related_section_when_unavailable() {
        let doc = "<!doctype html><html><body><pre>x</pre></body></html>";
        assert_eq!(print_body_with_related(doc, None, None), doc);
        assert_eq!(print_body_with_related(doc, Some(vec![]), None), doc);
        assert!(!print_body_with_related(doc, None, None).contains("Пов'язані"));
        // A non-empty list does add the section before `</body>`.
        let s = print_body_with_related(doc, Some(related_docs(&[("1-1", "Закон")])), None);
        assert!(s.contains("<h2>Пов'язані документи</h2>"), "{s}");
        assert!(s.contains("<li>Закон (1-1)</li>"), "{s}");
        assert!(
            s.find("Пов'язані документи").unwrap() < s.find("</body>").unwrap(),
            "section must sit inside the body: {s}"
        );
        assert!(s.ends_with("</body></html>"), "{s}");
    }

    /// Titles and nregs are HTML-escaped (the existing print view escapes
    /// via `esc_html`; the section mirrors it), and the list is capped at
    /// 25 entries to keep the print compact.
    #[test]
    fn print_related_section_escapes_and_caps() {
        let s = print_body_with_related(
            "<body></body>",
            Some(related_docs(&[("254к/96-вр", "<b>Закон</b> & <i>про все</i>")])),
            None,
        );
        assert!(
            s.contains("&lt;b&gt;Закон&lt;/b&gt; &amp; &lt;i&gt;про все&lt;/i&gt; (254к/96-вр)"),
            "{s}"
        );
        assert!(!s.contains("<b>Закон</b>"), "raw anchor markup must not leak: {s}");
        let many: Vec<RelatedDoc> = (0..40)
            .map(|i| RelatedDoc {
                nreg: format!("{i}-2026"),
                title: format!("Закон {i}"),
            })
            .collect();
        let s = print_body_with_related("<body></body>", Some(many), None);
        assert_eq!(s.matches("<li>").count(), 25, "capped at 25 entries: {s}");
        assert!(s.contains("Закон 24 (24-2026)"), "{s}");
        assert!(!s.contains("Закон 25"), "entries past the cap are dropped: {s}");
        // No </body>: the section is appended (same fallback shape as print_wrap).
        let s = print_body_with_related("<p>q</p>", Some(related_docs(&[("1-1", "T")])), None);
        assert!(s.starts_with("<p>q</p><h2>Пов'язані документи</h2>"), "{s}");
    }

    /// Offline index titles replace the page anchors (the same
    /// `title_by_nreg` join as `related_handler`); documents missing from
    /// the index keep the page title.
    #[test]
    fn print_related_section_prefers_index_title() {
        let st = favorites_state("print-related");
        let conn = db_open(&st.db_path).unwrap();
        let s = print_body_with_related(
            "<body></body>",
            Some(related_docs(&[("2341-14", "якір із сторінки")])),
            Some(&conn),
        );
        assert!(s.contains("<li>Про тестовий закон (2341-14)</li>"), "{s}");
        let s = print_body_with_related(
            "<body></body>",
            Some(related_docs(&[("9999-99", "З назви сторінки")])),
            Some(&conn),
        );
        assert!(s.contains("<li>З назви сторінки (9999-99)</li>"), "{s}");
    }

    #[test]
    fn status_reports_version() {        let st = favorites_state("ver");
        let (code, v) = call(&st, Method::Get, "/api/status", "");
        assert_eq!(code, 200);
        assert_eq!(
            v["version"].as_str().unwrap(),
            env!("CARGO_PKG_VERSION"),
            "/api/status must expose the crate version for the footer"
        );
    }

    /// Offline text-cache stats on /api/status for the footer: an empty
    /// cache reports 0/0, a seeded `docs_text` row reports its row count
    /// and body length (ASCII body ⇒ SQLite LENGTH equals the byte count).
    /// No network.
    #[test]
    fn status_reports_cache_rows_and_bytes() {
        let st = favorites_state("cache-stats");
        let (code, v) = call(&st, Method::Get, "/api/status", "");
        assert_eq!(code, 200);
        assert_eq!(v["cache_rows"], 0, "empty cache: {v}");
        assert_eq!(v["cache_bytes"], 0, "empty cache: {v}");
        let body = "body text for cache stats";
        {
            let conn = db_open(&st.db_path).unwrap();
            rada_db::textcache::put(&conn, "2341-14", "", body).unwrap();
        }
        let (code, v) = call(&st, Method::Get, "/api/status", "");
        assert_eq!(code, 200);
        assert_eq!(v["cache_rows"], 1, "{v}");
        assert_eq!(v["cache_bytes"], body.len() as i64, "{v}");
    }

    #[test]
    fn ui_page_has_cache_foot() {
        // Footer cache segment: the wiring to the new /api/status keys and
        // the exact Ukrainian label must be present in the embedded page.
        assert!(ui::INDEX_HTML.contains("cache_rows"), "footer reads cache_rows");
        assert!(ui::INDEX_HTML.contains("cache_bytes"), "footer reads cache_bytes");
        assert!(ui::INDEX_HTML.contains("кеш текстів"), "footer cache label");
        assert!(ui::INDEX_HTML.contains("1048576"), "bytes are formatted as MB");
    }

    #[test]
    fn annotations_export_route_downloads_json() {
        let st = favorites_state("export");
        let conn = db_open(&st.db_path).unwrap();
        rada_db::annotations::insert(&conn, "2341-14", "comment", "привіт", None, None).unwrap();
        drop(conn);
        let mut req: Request = TestRequest::new()
            .with_method(Method::Get)
            .with_path("/api/annotations/export")
            .into();
        let resp = route(&Method::Get, "/api/annotations/export", &[], &mut req, &st).expect("route");
        assert_eq!(resp.status_code().0, 200);
        let hdr = |name: &str| -> String {
            resp.headers()
                .iter()
                .find(|x| x.field.as_str().as_str().eq_ignore_ascii_case(name))
                .map(|x| x.value.to_string())
                .unwrap_or_default()
        };
        let ct = hdr("content-type");
        assert!(ct.contains("application/json"), "content-type: {ct}");
        let cd = hdr("content-disposition");
        assert!(cd.contains("attachment"), "disposition: {cd}");
        assert!(cd.contains("rada-annotations-"), "disposition: {cd}");
        let body = resp.into_reader().into_inner();
        let v: Value = serde_json::from_slice(&body).unwrap();
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["nreg"], "2341-14");
        assert_eq!(arr[0]["kind"], "comment");
        assert_eq!(arr[0]["text"], "привіт");
        // Wrong method → 405 JSON error.
        assert_eq!(call(&st, Method::Post, "/api/annotations/export", "").0, 405);
    }

    #[test]
    fn annotations_import_route_counts() {
        let st = favorites_state("import");
        // One valid + one blank-nreg item: only the valid one is imported.
        let body = r#"[{"nreg":"2341-14","kind":"comment","text":"імпортовано"},
                       {"nreg":"  ","kind":"comment","text":"не має потрапити"}]"#;
        let (code, v) = call(&st, Method::Post, "/api/annotations/import", body);
        assert_eq!(code, 200);
        assert_eq!(v["imported"], 1);
        // The imported annotation is visible through the «Обране» list.
        let (code, v) = call(&st, Method::Get, "/api/annotations/all", "");
        assert_eq!(code, 200);
        let items = v["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["text"], "імпортовано");
        assert_eq!(items[0]["title"], "Про тестовий закон");
        // Non-JSON is a hard 400 with the shared error shape; GET is 405.
        let (code, v) = call(&st, Method::Post, "/api/annotations/import", "не json");
        assert_eq!(code, 400);
        assert!(v["error"].is_string());
        assert_eq!(call(&st, Method::Get, "/api/annotations/import", "").0, 405);
    }

    #[test]
    fn ui_page_has_favorites_tab() {
        // «Обране» page: topbar tab, section with header controls, kind
        // filter chips, list container, and the three new endpoints.
        assert!(ui::INDEX_HTML.contains("data-tab=\"favorites\""));
        assert!(ui::INDEX_HTML.contains("Обране"));
        assert!(ui::INDEX_HTML.contains("id=\"tab-favorites\""));
        assert!(ui::INDEX_HTML.contains("id=\"fav-refresh\""));
        assert!(ui::INDEX_HTML.contains("id=\"fav-count\""));
        assert!(ui::INDEX_HTML.contains("id=\"fav-export\""));
        assert!(ui::INDEX_HTML.contains("id=\"fav-import\""));
        assert!(ui::INDEX_HTML.contains("id=\"fav-import-file\""));
        assert!(ui::INDEX_HTML.contains("id=\"fav-status\""));
        assert!(ui::INDEX_HTML.contains("id=\"fav-list\""));
        assert!(ui::INDEX_HTML.contains("/api/annotations/all"));
        assert!(ui::INDEX_HTML.contains("/api/annotations/export"));
        assert!(ui::INDEX_HTML.contains("/api/annotations/import"));
        assert!(ui::INDEX_HTML.contains("loadFavorites"));
        assert!(ui::INDEX_HTML.contains("renderFavorites"));
        // Kind filter chips: усі | коментарі | виділення | закладки.
        assert!(ui::INDEX_HTML.contains("data-kind=\"\""));
        assert!(ui::INDEX_HTML.contains("data-kind=\"comment\""));
        assert!(ui::INDEX_HTML.contains("data-kind=\"highlight\""));
        assert!(ui::INDEX_HTML.contains("data-kind=\"bookmark\""));
        // Inline edit reuses the extracted PUT flow shared with the reader.
        assert!(ui::INDEX_HTML.contains("makeAnnEdit"));
    }

    #[test]
    fn ui_page_has_favorites_sort_chips() {
        // Sort chips row (the GUI favorites parity): новіші | старіші |
        // за назвою, one active, persisted in "rada-fav-sort" and applied
        // through the pure favSorted helper before filtering/grouping.
        assert!(ui::INDEX_HTML.contains("id=\"fav-sort\""));
        assert!(ui::INDEX_HTML.contains("id=\"fav-sort-new\""));
        assert!(ui::INDEX_HTML.contains("id=\"fav-sort-old\""));
        assert!(ui::INDEX_HTML.contains("id=\"fav-sort-title\""));
        assert!(ui::INDEX_HTML.contains("data-sort=\"new\""));
        assert!(ui::INDEX_HTML.contains("data-sort=\"old\""));
        assert!(ui::INDEX_HTML.contains("data-sort=\"title\""));
        assert!(ui::INDEX_HTML.contains("новіші"));
        assert!(ui::INDEX_HTML.contains("старіші"));
        assert!(ui::INDEX_HTML.contains("за назвою"));
        assert!(ui::INDEX_HTML.contains("function favSorted"));
        assert!(ui::INDEX_HTML.contains("favSorted(favItems, favSort)"));
        assert!(ui::INDEX_HTML.contains("\"rada-fav-sort\""));
    }

    #[test]
    fn ui_page_has_favorites_grouping() {
        // Grouped mode: «групувати» chip persisted in "rada-fav-group",
        // per-document headers with click-to-collapse (« — згорнуто») and
        // the «розгорнути всі» button shown only while something is folded.
        assert!(ui::INDEX_HTML.contains("id=\"fav-group\""));
        assert!(ui::INDEX_HTML.contains("групувати"));
        assert!(ui::INDEX_HTML.contains("\"rada-fav-group\""));
        assert!(ui::INDEX_HTML.contains("fav-group-head"));
        assert!(ui::INDEX_HTML.contains("згорнуто"));
        assert!(ui::INDEX_HTML.contains("id=\"fav-uncollapse\""));
        assert!(ui::INDEX_HTML.contains("розгорнути всі"));
        assert!(ui::INDEX_HTML.contains("favCollapsed"));
        assert!(ui::INDEX_HTML.contains("function toggleFavGroup"));
    }

    #[test]
    fn ui_page_has_favorites_text_filter() {
        // Text filter over the favorites rows (the GUI favorites parity):
        // a search box with the GUI hint placeholder in the kind-chip row
        // and a pure substring helper over the annotation text, the
        // document title field and nreg (all lowercased, empty ⇒ match all).
        assert!(ui::INDEX_HTML.contains("id=\"fav-filter\""));
        assert!(ui::INDEX_HTML.contains(
            "placeholder=\"фільтр тексту, назви чи номера…\""
        ));
        assert!(ui::INDEX_HTML.contains("function favRowMatches(item, needle)"));
        assert!(ui::INDEX_HTML.contains("(item.text || \"\").toLowerCase().includes(f)"));
        assert!(ui::INDEX_HTML.contains("(item.title || \"\").toLowerCase().includes(f)"));
        assert!(ui::INDEX_HTML.contains("(item.nreg || \"\").toLowerCase().includes(f)"));
    }

    #[test]
    fn ui_page_favorites_filter_count_line() {
        // The text filter composes with the kind chip inside the renderer
        // (both must pass) and shows a muted «показано X з Y» line (the
        // arrivals #arr-shown pattern) only while it hides rows; the input
        // repaints the cached favItems without refetching.
        assert!(ui::INDEX_HTML.contains("id=\"fav-shown\" class=\"hint\""));
        assert!(ui::INDEX_HTML.contains("показано "));
        assert!(ui::INDEX_HTML.contains("shown.textContent = \"показано \""));
        assert!(ui::INDEX_HTML.contains(".filter(a => favRowMatches(a, needle))"));
        assert!(ui::INDEX_HTML.contains(
            "$(\"fav-filter\").addEventListener(\"input\", () => renderFavorites());"
        ));
    }

    #[test]
    fn ui_page_has_favorites_txt_export_button() {
        // «експорт .txt» in the favorites controls row right after the JSON
        // backup button (the CLI annot/export plain-text parity): a plain
        // label and a click wired to the exportFavTxt builder.
        assert!(ui::INDEX_HTML.contains("id=\"btn-fav-export-txt\""));
        assert!(ui::INDEX_HTML.contains(">експорт .txt</button>"));
        assert!(ui::INDEX_HTML.contains(
            "$(\"btn-fav-export-txt\").onclick = () => exportFavTxt();"
        ));
        let json = ui::INDEX_HTML.find("id=\"fav-export\"").unwrap();
        let txt = ui::INDEX_HTML.find("id=\"btn-fav-export-txt\"").unwrap();
        assert!(txt > json && txt - json < 400, "button sits next to fav-export");
    }

    #[test]
    fn ui_page_has_favorites_txt_export_builder() {
        // exportFavTxt reuses the favorites filter+sort pipeline (favSorted
        // + the active kind chip + favRowMatches over the #fav-filter box)
        // so the file mirrors the on-screen order, opens with the
        // «Обрані анотації — X з Y» header, renders each row as
        // «[kind] title (nreg)» + trailing-trimmed text + a blank line, and
        // downloads through the reader export's Blob / object-URL idiom
        // under the fixed annots.txt name (revoked right after the click).
        let start = ui::INDEX_HTML
            .find("function exportFavTxt()")
            .expect("exportFavTxt defined");
        let end = start + ui::INDEX_HTML[start..]
            .find("btn-fav-export-txt")
            .expect("exportFavTxt wired to its button");
        let body = &ui::INDEX_HTML[start..end];
        // the same pipeline renderFavorites runs: sort, kind chip, text filter
        assert!(body.contains("favSorted(favItems, favSort)"));
        assert!(body.contains(".filter(a => !favKind || a.kind === favKind)"));
        assert!(body.contains(".filter(a => favRowMatches(a, needle))"));
        // header line «Обрані анотації — {shown} з {total}»
        assert!(body.contains(
            "\"Обрані анотації \u{2014} \" + items.length + \" з \" + favItems.length"
        ));
        // per-item «[kind] title (nreg)» line + trimmed text
        assert!(body.contains("\"[\" + it.kind + \"] \""));
        assert!(body.contains("(it.title || it.nreg || \"\")"));
        assert!(body.contains("head += \" (\" + it.nreg + \")\""));
        assert!(body.contains("(it.text || \"\").trimEnd()"));
        // the reader export's Blob download idiom under annots.txt
        assert!(body.contains("new Blob([doc], { type:"));
        assert!(body.contains("URL.createObjectURL(blob)"));
        assert!(body.contains("a.download = \"annots.txt\";"));
        assert!(body.contains("a.click();"));
        assert!(body.contains("a.remove();"));
        assert!(body.contains("URL.revokeObjectURL(url);"));
    }

    #[test]
    fn ui_page_has_favorites_citation_button() {
        // Per-row «цилата» button on the favorites rows (the reader btn-cite
        // parity): a plain-text label with the Ukrainian copy-hint, the same
        // chip-styled small-button class group as jump/edit/del, and placed
        // inside the row's action group right after the «→ текст» jump
        // button (away from the delete control).
        assert!(ui::INDEX_HTML.contains("cite.className = \"cite\";"));
        assert!(ui::INDEX_HTML.contains("cite.textContent = \"цилата\";"));
        assert!(ui::INDEX_HTML.contains(
            "cite.title = \"скопіювати повне посилання на акт\";"
        ));
        // Shares the row's small-button CSS (the .jump/.edit/.del chip group).
        assert!(ui::INDEX_HTML.contains("#fav-list .fav-item .cite"));
        let jump = ui::INDEX_HTML
            .find("row.appendChild(jump);")
            .expect("jump button appended to the row");
        let cite = ui::INDEX_HTML
            .find("row.appendChild(cite);")
            .expect("cite button appended to the row");
        let del = ui::INDEX_HTML
            .find("row.appendChild(del);")
            .expect("delete button appended to the row");
        assert!(cite > jump && cite < del, "cite sits beside jump in the action group");
    }

    #[test]
    fn ui_page_has_favorites_citation_uses_shared_builder() {
        // The row click copies through the reader's shared citation builder
        // (no re-inlined «Офіційний вебпортал» format string in the favorites
        // code): title falls back to nreg, the edition argument is the empty
        // string matching btn-cite's default «поточна редакція» option (which
        // omits the «Редакція від» segment), and the existing copyText helper
        // flashes «скопійовано» on the button.
        let start = ui::INDEX_HTML
            .find("function favRow(a)")
            .expect("favRow defined");
        let end = start + ui::INDEX_HTML[start..]
            .find("function renderFavorites")
            .expect("renderFavorites follows favRow");
        let body = &ui::INDEX_HTML[start..end];
        assert!(body.contains("cite.onclick = () => copyText("));
        assert!(body.contains("citationText(a.title || a.nreg, a.nreg, \"\")"));
        assert!(body.contains("cite, \"скопійовано\");"));
        // the citation format itself stays only in citationText
        assert!(!body.contains("Офіційний вебпортал"));
        assert!(!body.contains("Редакція від"));
    }

    #[test]
    fn fmt_yyyymmdd_passthrough() {
        assert_eq!(fmt_yyyymmdd(20261015), "15.10.2026");
        assert_eq!(fmt_yyyymmdd(42), "42");
    }

    // ---- result sorting (`sort=` parameter, «Пошук» column headers) --------

    /// Fresh indexed db with three documents at distinct adoption / edition
    /// dates and A→Я Cyrillic titles, for the sort tests. Unique dir per tag
    /// keeps parallel tests from colliding.
    fn sorted_state(tag: &str) -> Arc<State> {
        let dir = std::env::temp_dir().join(format!("rada-web-sort-test-{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        let st = Arc::new(State {
            db_path: dir.join("test.sqlite"),
            client: RadaClient::with_pacing(Duration::from_secs(3600)),
            pace_secs: 3600,
            jobs: Mutex::new(HashMap::new()),
            job_seq: AtomicU64::new(0),
        });
        let conn = db_open(&st.db_path).expect("open test index");
        conn.execute_batch(
            "INSERT INTO docs(nreg, nazva, status, org_date, org_num, poddat) VALUES
             ('1000-01', 'Абетка',  5, '20010101', '1000', '20030101'),
             ('2000-02', 'Кодекс',  1, '20020202', '2000', '20010202'),
             ('3000-03', 'Ятринка', 5, '20030303', '3000', '20020303');",
        )
        .expect("seed docs");
        drop(conn);
        st
    }

    /// `GET /api/search?{query}` through `route` (query parsed exactly as
    /// `handle_request` does), returning the JSON body.
    fn search_get(st: &Arc<State>, query: &str) -> Value {
        let mut req: Request = TestRequest::new()
            .with_method(Method::Get)
            .with_path("/api/search")
            .into();
        let resp = route(&Method::Get, "/api/search", &parse_query(query), &mut req, st)
            .expect("route");
        assert_eq!(resp.status_code().0, 200);
        serde_json::from_slice(&resp.into_reader().into_inner()).unwrap()
    }

    /// The hit nregs, in the order the API returned them.
    fn hit_nregs(v: &Value) -> Vec<String> {
        v["hits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| h["nreg"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn search_nreg_exact_shortcut() {
        let st = sorted_state("nreg");
        // Exact nreg-shaped query: single hit flagged for direct open.
        let v = search_get(&st, "q=2000-02");
        assert_eq!(v["nreg_exact"], true, "{v}");
        assert_eq!(v["total"], 1);
        assert_eq!(hit_nregs(&v), ["2000-02"]);
        // nreg-shaped but not in the index: normal search (no flag).
        let miss = search_get(&st, "q=9999-99");
        assert!(miss.get("nreg_exact").is_none(), "{miss}");
        assert_eq!(miss["total"], 0);
        // A title-shaped query never takes the shortcut.
        let title = search_get(&st, "q=%D0%9A%D0%BE%D0%B4%D0%B5%D0%BA%D1%81");
        assert!(title.get("nreg_exact").is_none(), "{title}");
        assert_eq!(hit_nregs(&title), ["2000-02"]);
    }

    #[test]
    fn search_sort_date_asc() {        let st = sorted_state("date");
        // Oldest adoption date first.
        assert_eq!(
            hit_nregs(&search_get(&st, "sort=date_asc")),
            ["1000-01", "2000-02", "3000-03"],
        );
        // Without `sort` the default stays newest-first.
        assert_eq!(
            hit_nregs(&search_get(&st, "")),
            ["3000-03", "2000-02", "1000-01"],
        );
    }

    #[test]
    fn search_sort_nazva() {
        let st = sorted_state("nazva");
        // Cyrillic NOCASE order: Абетка < Кодекс < Ятринка.
        assert_eq!(
            hit_nregs(&search_get(&st, "sort=nazva_asc")),
            ["1000-01", "2000-02", "3000-03"],
        );
        assert_eq!(
            hit_nregs(&search_get(&st, "sort=nazva_desc")),
            ["3000-03", "2000-02", "1000-01"],
        );
    }

    #[test]
    fn search_sort_garbage_defaults() {
        let st = sorted_state("garbage");
        // An unknown id must behave exactly like the (DateDesc) default.
        let junk = hit_nregs(&search_get(&st, "sort=zzz"));
        assert_eq!(junk, hit_nregs(&search_get(&st, "")));
        assert_eq!(junk, ["3000-03", "2000-02", "1000-01"]);
    }

    // ---- facet counts (dictionary dropdowns on «Пошук») --------------------

    /// Fresh indexed db with three documents at distinct status / types /
    /// org_id combinations — (5, "2|168", 10), (1, "2", 10), (5, "168", 20) —
    /// for the facet tests. Unique dir per tag keeps parallel tests apart.
    fn facet_state(tag: &str) -> Arc<State> {
        let dir = std::env::temp_dir().join(format!("rada-web-facet-test-{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        let st = Arc::new(State {
            db_path: dir.join("test.sqlite"),
            client: RadaClient::with_pacing(Duration::from_secs(3600)),
            pace_secs: 3600,
            jobs: Mutex::new(HashMap::new()),
            job_seq: AtomicU64::new(0),
        });
        let conn = db_open(&st.db_path).expect("open test index");
        conn.execute_batch(
            "INSERT INTO docs(nreg, nazva, status, types, org_id, org_date, org_num, poddat) VALUES
             ('1000-01', 'Абетка',  5, '2|168', 10, '20010101', '1000', '20030101'),
             ('2000-02', 'Кодекс',  1, '2',     10, '20020202', '2000', '20010202'),
             ('3000-03', 'Ятринка', 5, '168',   20, '20030303', '3000', '20020303');",
        )
        .expect("seed docs");
        drop(conn);
        st
    }

    #[test]
    fn search_endpoint_returns_facets() {
        let st = facet_state("shape");
        let v = search_get(&st, "");
        let f = &v["facets"];
        // (id, count) pairs, count desc then id; "2|168" counts in both buckets.
        assert_eq!(f["status"], json!([[5, 2], [1, 1]]), "{v}");
        assert_eq!(f["typ"], json!([[2, 2], [168, 2]]), "{v}");
        assert_eq!(f["org"], json!([[10, 2], [20, 1]]), "{v}");
        // The nreg-exact shortcut answers before any result set: no facets.
        let exact = search_get(&st, "q=1000-01");
        assert_eq!(exact["nreg_exact"], true, "{exact}");
        assert!(exact.get("facets").is_none(), "{exact}");
    }

    #[test]
    fn search_facets_ignore_own_column_filter() {
        let st = facet_state("own");
        // status=5 narrows the hits to 2, but the status facet keeps counting
        // the unselected option so the user can flip the choice…
        let v = search_get(&st, "status=5");
        assert_eq!(v["total"], 2);
        let f = &v["facets"];
        assert_eq!(f["status"], json!([[5, 2], [1, 1]]), "status filter ignored: {v}");
        // …while the other facets honor it (only the two status-5 docs).
        assert_eq!(f["typ"], json!([[168, 2], [2, 1]]), "typ honors the status filter: {v}");
        assert_eq!(f["org"], json!([[10, 1], [20, 1]]), "org honors the status filter: {v}");
    }

    #[test]
    fn search_facets_empty_result_and_zero_ids() {
        let st = facet_state("empty");
        // A query with no matches: facets are empty arrays, still HTTP 200.
        let v = search_get(&st, "q=%D0%BD%D0%B5%D0%BC%D0%B0%D1%94"); // «немає»
        assert_eq!(v["total"], 0);
        assert_eq!(v["facets"]["status"].as_array().unwrap().len(), 0, "{v}");
        assert_eq!(v["facets"]["typ"].as_array().unwrap().len(), 0, "{v}");
        assert_eq!(v["facets"]["org"].as_array().unwrap().len(), 0, "{v}");
        // Empty index (no docs at all): the shape is kept, no failure.
        let conn = db_open(&st.db_path).unwrap();
        conn.execute("DELETE FROM docs", []).unwrap();
        drop(conn);
        let v = search_get(&st, "");
        assert_eq!(v["total"], 0);
        assert!(v["facets"]["status"].is_array(), "{v}");
        // Defaults unset in the index (favorites seed): id 0 still surfaces.
        let st0 = favorites_state("facets-zero");
        let v = search_get(&st0, "");
        assert_eq!(v["total"], 1);
        assert_eq!(v["facets"]["status"], json!([[0, 1]]), "{v}");
        assert_eq!(v["facets"]["typ"].as_array().unwrap().len(), 0, "empty types explode to nothing: {v}");
        assert_eq!(v["facets"]["org"], json!([[0, 1]]), "{v}");
    }

    #[test]
    fn ui_page_has_sort_headers() {
        // Column-header sort row above the results: the bar, one chip per
        // column, and the sort parameter threaded into the search request.
        assert!(ui::INDEX_HTML.contains("id=\"sort-bar\""));
        assert!(ui::INDEX_HTML.contains("id=\"sort-nazva\""));
        assert!(ui::INDEX_HTML.contains("id=\"sort-date\""));
        assert!(ui::INDEX_HTML.contains("id=\"sort-podd\""));
        assert!(ui::INDEX_HTML.contains("id=\"sort-relevance\""));
        assert!(ui::INDEX_HTML.contains("data-col=\"nazva\""));
        assert!(ui::INDEX_HTML.contains("data-col=\"date\""));
        assert!(ui::INDEX_HTML.contains("data-col=\"podd\""));
        assert!(ui::INDEX_HTML.contains("data-col=\"relevance\""));
        assert!(ui::INDEX_HTML.contains(">Назва<"));
        assert!(ui::INDEX_HTML.contains(">Дата<"));
        assert!(ui::INDEX_HTML.contains(">Редакція<"));
        assert!(ui::INDEX_HTML.contains(">Релевантність<"));
        assert!(ui::INDEX_HTML.contains("p.set(\"sort\", state.sort)"));
        assert!(ui::INDEX_HTML.contains("renderSortBar"));
        assert!(ui::INDEX_HTML.contains("clickSort"));
    }

    #[test]
    fn ui_page_has_date_filters() {
        // Date-range filters on the «Пошук» page: two compact YYYYMMDD text
        // inputs with a muted hint for invalid values, and the doSearch
        // wiring that sends them as `date_from`/`date_to` only when valid.
        assert!(ui::INDEX_HTML.contains("id=\"date-from\""));
        assert!(ui::INDEX_HTML.contains("id=\"date-to\""));
        assert!(ui::INDEX_HTML.contains("id=\"date-hint\""));
        assert!(ui::INDEX_HTML.contains("від РРРРММДД"));
        assert!(ui::INDEX_HTML.contains("до РРРРММДД"));
        assert!(ui::INDEX_HTML.contains("дату вказано невірно"));
        // runSearch wiring: validated values become query parameters, and
        // the hint visibility is driven by the 8-digit check.
        assert!(ui::INDEX_HTML.contains("\"date-from\", \"date_from\""));
        assert!(ui::INDEX_HTML.contains("\"date-to\", \"date_to\""));
        assert!(ui::INDEX_HTML.contains("$(\"date-hint\").hidden"));
        assert!(ui::INDEX_HTML.contains("/^[0-9]{8}$/"));
    }

    #[test]
    fn ui_page_has_cache_search() {        // «Пошук у тексті»: offline full-text button on the search page that
        // calls the cache endpoint and renders ranked snippet rows.
        assert!(ui::INDEX_HTML.contains("id=\"btn-cache-search\""));
        assert!(ui::INDEX_HTML.contains("Пошук у тексті"));
        assert!(ui::INDEX_HTML.contains("async function doCacheSearch()"));
        assert!(ui::INDEX_HTML.contains("/api/cache/search?q="));
        assert!(ui::INDEX_HTML.contains("it.snippet"));
        assert!(ui::INDEX_HTML.contains("openDoc(it.nreg)"));
    }

    #[test]
    fn ui_page_has_cache_mode_banner() {
        // Cache-mode banner under the «недавні запити» row: a hidden-by-default
        // row that every successful cache search paints (zero hits included)
        // with the document-count hint and the «× звичайний пошук» exit chip.
        assert!(ui::INDEX_HTML.contains("id=\"cache-mode\" class=\"row\" hidden"));
        assert!(ui::INDEX_HTML.contains("пошук у кешованому тексті · документів: "));
        assert!(ui::INDEX_HTML.contains("id=\"cache-mode-exit\""));
        assert!(ui::INDEX_HTML.contains("× звичайний пошук"));
        assert!(ui::INDEX_HTML.contains("function renderCacheMode(count)"));
        assert!(ui::INDEX_HTML.contains("renderCacheMode(0);"));
        assert!(ui::INDEX_HTML.contains("renderCacheMode(d.items.length);"));
    }

    #[test]
    fn ui_page_cache_mode_exits_to_search() {
        // The exit chip hides the banner and re-runs the regular search from
        // the first page with the current #q; doSearch always hides the banner
        // up front (a regular search is never cache mode), and a failed cache
        // search leaves no stale banner next to the «Помилка: » meta line.
        assert!(ui::INDEX_HTML.contains(
            r##"$("cache-mode").hidden = true; doSearch(0);"##
        ));
        assert!(ui::INDEX_HTML.contains(
            r##"$("cache-mode").hidden = true; // a regular search is never cache mode"##
        ));
        assert!(ui::INDEX_HTML.contains(
            r##"$("cache-mode").hidden = true; // a failed cache search is not cache mode"##
        ));
    }

    #[test]
    fn ui_page_has_facet_counts() {
        // Dictionary dropdowns annotate their options with per-option hit
        // counts after every search («Чинний (1204)»): the annotation
        // function, the JSON field it consumes, the original-label cache it
        // restores before each search, and its call site in doSearch.
        assert!(ui::INDEX_HTML.contains("function applyFacets("));
        assert!(ui::INDEX_HTML.contains("optionLabels"));
        assert!(ui::INDEX_HTML.contains("resetFacetLabels()"));
        assert!(ui::INDEX_HTML.contains("applyFacets(\"f-status\", d.facets && d.facets.status)"));
        assert!(ui::INDEX_HTML.contains("applyFacets(\"f-typ\", d.facets && d.facets.typ)"));
        assert!(ui::INDEX_HTML.contains("applyFacets(\"f-org\", d.facets && d.facets.org)"));
        assert!(ui::INDEX_HTML.contains("facets"));
    }

    #[test]
    fn ui_page_has_select_facet_counts() {
        // Paint pass: every dictionary select gets the current query's facet
        // counts in its option labels («Кабінет Міністрів України (275)»),
        // built FRESH from the cached clean label plus the count, so a
        // repaint can never stack (« (5) (7)» is impossible).
        let h = ui::INDEX_HTML;
        let f = &h[h.find("function applyFacets(").expect("applyFacets")..];
        let f = &f[..f.find("\n}").expect("applyFacets end")];
        assert!(f.contains("optionLabels[selectId]"),
            "labels rebuild from the clean cache, never from the painted text");
        assert!(f.contains("byId[String(pair[0])] = pair[1]"),
            "the payload is [id, count] pairs from the search response's facets");
        assert!(f.contains(r##"o.textContent = n === undefined ? label : label + " (" + n + ")";"##),
            "count appended to the clean label; an option with no facet entry stays clean");
        assert!(f.contains("Array.isArray(counts) ? counts : []"),
            "a malformed (missing or non-array) axis paints every option clean instead of throwing");
        // All three axes are fed from the search payload after a successful
        // search (the facet data the /api/search response already carries).
        assert!(h.contains(r##"applyFacets("f-status", d.facets && d.facets.status)"##));
        assert!(h.contains(r##"applyFacets("f-typ", d.facets && d.facets.typ)"##));
        assert!(h.contains(r##"applyFacets("f-org", d.facets && d.facets.org)"##));
        // The counts reach the options only through these labels: the clean
        // label is captured once in fillDict, before any painting happens.
        let fd = &h[h.find("async function fillDict(").expect("fillDict")..];
        let fd = &fd[..fd.find("\n}").expect("fillDict end")];
        assert!(fd.contains(r##"optionLabels[selectId][o.value] = o.textContent;"##),
            "the clean label is cached when the dictionary populates the select");
    }

    #[test]
    fn ui_page_has_select_facet_counts_reset() {
        // Reset pass: a new search strips every painted count before the
        // fetch goes out, so facet-less results (null facets, the nreg-exact
        // early return) and failed searches all leave the options clean.
        let h = ui::INDEX_HTML;
        let r = &h[h.find("function resetFacetLabels() {").expect("resetFacetLabels")..];
        let r = &r[..r.find("\n}").expect("resetFacetLabels end")];
        assert!(r.contains(r##"for (const selectId of ["f-status", "f-typ", "f-org"])"##),
            "every dictionary select is restored, not just one");
        assert!(r.contains(r##"o.textContent = label;"##),
            "labels restored from the clean cache");
        let d = &h[h.find("async function doSearch(offset) {").expect("doSearch")..];
        let d = &d[..d.find("\n}").expect("doSearch end")];
        let reset = d.find("resetFacetLabels();").expect("facet reset");
        let fetch = d.find("await api(").expect("search fetch");
        assert!(reset < fetch,
            "stale counts die before the fetch: a failed or facet-less search repaints clean");
        // Chip and export label sources read the clean caches, never the
        // painted option text, so counts never leak out of the dropdowns.
        let bc = &h[h.find("function buildStatusChips() {").expect("buildStatusChips")..];
        let bc = &bc[..bc.find("\n}").expect("buildStatusChips end")];
        assert!(bc.contains(r##"items.push({ value: o.value, label: statusLabel(o.value) });"##),
            "quick status chips take the clean dict label through statusLabel(), not option text");
        let ac = &h[h.find("function activeChips() {").expect("activeChips")..];
        let ac = &ac[..ac.find("\n}").expect("activeChips end")];
        assert!(ac.contains(r##"const cache = optionLabels[id] || {};"##),
            "active-filter chips take the cached clean label, not the painted option text");
        assert!(ac.contains(r##"return cache[v] === undefined ? v : cache[v];"##),
            "a cached label always wins over the raw value");
        // syncStatusChips only toggles chip state; it never rewrites labels,
        // so the quick-chip row cannot pick up a count suffix either.
        let sc = &h[h.find("function syncStatusChips() {").expect("syncStatusChips")..];
        let sc = &sc[..sc.find("\n}").expect("syncStatusChips end")];
        assert!(!sc.contains("textContent"), "the chip repaint never rewrites labels");
    }

    #[test]
    fn ui_page_print_passes_theme() {
        // The «Друк» button appends the current theme id so the print view
        // restyles headings with the theme accent (per-theme print CSS).
        assert!(ui::INDEX_HTML.contains("rada-theme"));
        assert!(ui::INDEX_HTML.contains("\"?print=1&theme=\""));
    }

    #[test]
    fn ui_page_has_print_header_helper() {
        // The printable page masthead is one pure helper: bold act title
        // line, «реєстр. № nreg» (+ « · редакція від dd.mm.yyyy» for a dated
        // edition) and a muted «URL: <portal page> · надруковано: …» line;
        // every interpolated value goes through escHtml.
        let h = ui::INDEX_HTML;
        assert!(
            h.contains("function printHeaderHtml(title, nreg, edition, todayDdMmYyyy)"),
            "printHeaderHtml helper missing"
        );
        let f = &h[h.find("function printHeaderHtml(").unwrap()..];
        let f = &f[..f.find("\n}").unwrap()];
        assert!(f.contains("реєстр. № "), "{f}");
        assert!(f.contains("редакція від "), "{f}");
        assert!(f.contains("URL: "), "{f}");
        assert!(f.contains("надруковано: "), "{f}");
        assert!(f.contains("escHtml(title"), "{f}");
        assert!(f.contains("escHtml(nreg"), "{f}");
        assert!(f.contains("escHtml(siteUrlFor"), "{f}");
        assert!(f.contains("escHtml(todayDdMmYyyy"), "{f}");
        assert!(f.contains("escHtml(fmtArrDate(ed))"), "{f}");
        // The print date comes from the existing formatter style: today as
        // 8-digit YYYYMMDD rendered through fmtArrDate.
        assert!(h.contains("function todayYyyymmdd()"), "today formatter missing");
        assert!(
            h.contains("d.getFullYear() + p(d.getMonth() + 1) + p(d.getDate())"),
            "today built from getFullYear/getMonth/getDate"
        );
    }

    #[test]
    fn ui_page_has_print_header_wired() {
        // The print assembler injects the masthead into the printable page:
        // the «Друк» button opens a new tab, fetches the themed print view,
        // calls printHeaderHtml with the current card title, nreg, selected
        // edition and the today-formatted date, and writes the header in
        // right after the opening <body> tag (before the body content).
        let h = ui::INDEX_HTML;
        assert!(
            h.contains("openPrintView(\"/api/export/\""),
            "print button must assemble the printable page"
        );
        assert!(h.contains("async function openPrintView(url)"), "assembler missing");
        let s = &h[h.find("async function openPrintView(").unwrap()..];
        let s = &s[..s.find("\n}").unwrap()];
        assert!(s.contains("printHeaderHtml("), "{s}");
        assert!(s.contains("state.curCard ? state.curCard.nazva : \"\""), "{s}");
        assert!(s.contains("state.curNreg,"), "{s}");
        assert!(s.contains("(\"editions\").value"), "{s}");
        assert!(s.contains("fmtArrDate(todayYyyymmdd())"), "{s}");
        assert!(s.contains("w.document.write(insertPrintHeader(html, header))"), "{s}");
        // The header lands before the body content (right after the opening
        // <body…> tag; html without a body tag gets it prepended).
        let ins = &h[h.find("function insertPrintHeader(").unwrap()..];
        let ins = &ins[..ins.find("\n}").unwrap()];
        assert!(ins.contains("<body[^>]*>"), "{ins}");
        assert!(ins.contains("html.slice(0, m.index + m[0].length) + header"), "{ins}");
        assert!(ins.contains("return header + html"), "{ins}");
    }

    #[test]
    fn ui_page_has_deep_links() {
        // URL <-> state: search params are restored on load and written back
        // on every search; opening a document puts ?doc=<nreg> into the URL.
        assert!(ui::INDEX_HTML.contains("function readDeepLink()"));
        assert!(ui::INDEX_HTML.contains("function writeDeepLink()"));
        assert!(ui::INDEX_HTML.contains("writeDeepLink();"));
        assert!(ui::INDEX_HTML.contains("history.replaceState(null, \"\", \"?doc=\""));
        assert!(ui::INDEX_HTML.contains("readDeepLink();"));
    }

    #[test]
    fn ui_page_has_toc() {
        // «Зміст» panel (the GUI reader's outline parity): a ghost toggle
        // above the document text, the entries list, labels built from the
        // rendered .p-section/.p-article heading blocks, smooth scrollIntoView
        // navigation, and the reset that clears stale entries on openDoc.
        assert!(ui::INDEX_HTML.contains("id=\"btn-toc\""));
        assert!(ui::INDEX_HTML.contains("id=\"toc-list\""));
        assert!(ui::INDEX_HTML.contains("Зміст"));
        assert!(ui::INDEX_HTML.contains("function buildToc()"));
        assert!(ui::INDEX_HTML.contains("\".p-section, .p-article\""));
        assert!(ui::INDEX_HTML.contains("scrollIntoView({ behavior: \"smooth\", block: \"start\" })"));
        assert!(ui::INDEX_HTML.contains("function resetToc()"));
        assert!(ui::INDEX_HTML.contains("resetToc(); // stale outline entries must never survive into a new document"));
    }

    #[test]
    fn ui_page_has_toc_collapse() {
        // «Зміст» header is a clickable collapse toggle (the GUI reader's
        // outline_open parity): the header click flips the collapse state,
        // a pure tocCollapsedHtml helper is the single source for the header
        // paint, the folded state shows the muted « — згорнуто» suffix in the
        // exact glyph style of the arrivals/favourites group headers, and
        // #toc-list's hidden flag is driven solely by that state.
        assert!(ui::INDEX_HTML.contains("$(\"btn-toc\").onclick = () => {"));
        assert!(ui::INDEX_HTML.contains("tocCollapsed = !tocCollapsed;"));
        assert!(ui::INDEX_HTML.contains("$(\"toc-list\").hidden = tocCollapsed;"));
        assert!(ui::INDEX_HTML.contains("function tocCollapsedHtml(baseLabel, collapsed)"));
        assert!(ui::INDEX_HTML.contains("paintTocHeader(heads.length);"));
        // muted suffix: .hint span with the same em-dash « — згорнуто» glyph
        // the arrivals/fav collapse paint (rendered text = base + « — згорнуто»).
        assert!(ui::INDEX_HTML.contains("<span class=\\\"hint\\\">— згорнуто</span>"));
        assert!(ui::INDEX_HTML.contains("— згорнуто"));
        // collapse choice persisted in its own key (never collides with the
        // arrivals/fav keys — mirroring the «rada-fav-group» "1"/"0" idiom).
        assert!(ui::INDEX_HTML.contains("localStorage.setItem(TOC_COLLAPSED_KEY, tocCollapsed ? \"1\" : \"0\")"));
    }

    #[test]
    fn ui_page_has_toc_collapse_persisted() {
        // The collapse choice survives reloads and document switches: init
        // reads "rada-toc-collapsed" (default expanded when absent), every
        // toggle writes it back, resetToc leaves the choice untouched so the
        // next buildToc repaints header + container from it, and the entry
        // markup / smooth jump stay byte-identical.
        assert!(ui::INDEX_HTML.contains("const TOC_COLLAPSED_KEY = \"rada-toc-collapsed\";"));
        assert!(ui::INDEX_HTML.contains("let tocCollapsed = false;"));
        assert!(ui::INDEX_HTML.contains("localStorage.getItem(TOC_COLLAPSED_KEY) === \"1\""));
        // resetToc clears and hides but never resets the persisted choice.
        assert!(ui::INDEX_HTML.contains(
            "function resetToc() {\n  $(\"toc-list\").innerHTML = \"\";\n  $(\"toc-list\").hidden = true;\n  $(\"btn-toc\").hidden = true;\n}"
        ));
        // rebuild repaints header and container from the persisted state.
        assert!(ui::INDEX_HTML.contains(
            "$(\"btn-toc\").innerHTML = tocCollapsedHtml(\"Зміст (\" + n + \")\", tocCollapsed);"
        ));
        // list and jump markup untouched.
        assert!(ui::INDEX_HTML.contains("row.className = \"toc-item\";"));
        assert!(ui::INDEX_HTML.contains("scrollIntoView({ behavior: \"smooth\", block: \"start\" })"));
    }

    #[test]
    fn ui_page_has_copy_text() {
        // «Копіювати текст» in the export row (the GUI reader's copy parity):
        // the clipboard API with a textarea/execCommand fallback and the
        // transient «скопійовано» / «не вдалося» hint in #copy-hint.
        assert!(ui::INDEX_HTML.contains("id=\"btn-copy-text\""));
        assert!(ui::INDEX_HTML.contains("Копіювати текст"));
        assert!(ui::INDEX_HTML.contains("navigator.clipboard"));
        assert!(ui::INDEX_HTML.contains("id=\"copy-hint\""));
    }

    #[test]
    fn ui_page_has_doc_stats() {
        // Muted stats line beside the export row, same format as the GUI
        // reader toolbar: «{words} слів · {chars} симв. · ~{min} хв»
        // (≈400 wpm, min 1, thousands grouped with plain spaces).
        assert!(ui::INDEX_HTML.contains("id=\"doc-stats\""));
        assert!(ui::INDEX_HTML.contains("function groupTh("));
        assert!(ui::INDEX_HTML.contains("слів"));
        assert!(ui::INDEX_HTML.contains("симв."));
        assert!(ui::INDEX_HTML.contains("хв"));
        assert!(ui::INDEX_HTML.contains("/400"));
    }

    #[test]
    fn ui_page_has_active_filter_chips() {
        // «Активні фільтри» row under the search form (the GUI search page
        // parity): one chip per set filter with Ukrainian labels and a × to
        // clear it, the shared chipLabel shape helper («{kind}: {label} ×»),
        // and «скинути все» clearing everything at once.
        assert!(ui::INDEX_HTML.contains("<div id=\"active-filters\" class=\"row\" hidden></div>"));
        assert!(ui::INDEX_HTML.contains("function chipLabel(kind, label)"));
        assert!(ui::INDEX_HTML.contains("kind + \": \" + label + \" ×\""));
        assert!(ui::INDEX_HTML.contains("function activeChips("));
        assert!(ui::INDEX_HTML.contains("скинути все"));
        assert!(ui::INDEX_HTML.contains("chipLabel(\"Статус\""));
        assert!(ui::INDEX_HTML.contains("chipLabel(\"Тип\""));
        assert!(ui::INDEX_HTML.contains("chipLabel(\"Видавник\""));
        assert!(ui::INDEX_HTML.contains("chipLabel(\"Дата від\""));
        assert!(ui::INDEX_HTML.contains("chipLabel(\"Дата до\""));
    }

    #[test]
    fn ui_page_search_refreshes_active_filter_chips() {
        // The chip row repaints inside doSearch on every search — the init
        // doSearch(0) covers deep links that prefill filters too — a chip
        // click clears its own control and re-searches from page 1, and
        // «скинути все» zeroes all six filter controls (never #q).
        assert!(ui::INDEX_HTML.contains("renderActiveFilters();"));
        assert!(ui::INDEX_HTML.contains("$(c.id).value = \"\"; doSearch(0);"));
        assert!(ui::INDEX_HTML.contains(
            "for (const id of [\"f-status\", \"f-typ\", \"f-org\", \"f-number\", \"date-from\", \"date-to\"])"
        ));
    }

    #[test]
    fn ui_page_has_arrivals_filter() {
        // Text filter over the arrivals feed (the GUI arrivals page parity):
        // a filter box with the GUI hint text, a x clear button, a count
        // hidden while no filter is active, and the pure substring helper
        // over nazva/nreg/publics shared by the filter.
        assert!(ui::INDEX_HTML.contains("id=\"arr-filter\""));
        assert!(ui::INDEX_HTML.contains("placeholder=\"\u{043D}\u{0430}\u{0437}\u{0432}\u{0430}, \u{043D}\u{043E}\u{043C}\u{0435}\u{0440} \u{0447}\u{0438} \u{043F}\u{0443}\u{0431}\u{043B}\u{0456}\u{043A}\u{0430}\u{0446}\u{0456}\u{044F}\u{2026}\""));
        assert!(ui::INDEX_HTML.contains("id=\"arr-filter-clear\""));
        assert!(ui::INDEX_HTML.contains("id=\"arr-shown\" class=\"hint\""));
        assert!(ui::INDEX_HTML.contains("\u{043F}\u{043E}\u{043A}\u{0430}\u{0437}\u{0430}\u{043D}\u{043E} "));
        assert!(ui::INDEX_HTML.contains("function arrFilterMatch(d, needle)"));
    }

    #[test]
    fn ui_page_arrivals_filter_rerenders_from_cached_feed() {
        // The filter never refetches: the input handler re-renders the cached
        // arrivalsDocs array through the shared renderer with arrFilterMatch,
        // and the clear button empties the box and re-renders the full list.
        assert!(ui::INDEX_HTML.contains(
            "$(\"arr-filter\").addEventListener(\"input\", () => applyArrFilter());"
        ));
        assert!(ui::INDEX_HTML.contains(
            "const items = arrivalsDocs.filter((d) => arrFilterMatch(d, needle));"
        ));
        assert!(ui::INDEX_HTML.contains("renderArrivals(items);"));
        assert!(ui::INDEX_HTML.contains(
            "$(\"arr-filter-clear\").onclick = () => { $(\"arr-filter\").value = \"\"; applyArrFilter(); };"
        ));
    }

    #[test]
    fn ui_page_has_arrivals_new_badge() {
        // Gold «НОВЕ» pill for unseen arrivals (the GUI arrivals parity): the
        // pill class painted by the renderer, the «нових: N» counter next to
        // the shown count, the pure unseen helper and the persisted seen-set
        // key (cap 500, newest last, JSON errors tolerated).
        assert!(ui::INDEX_HTML.contains("arr-new-pill"));
        assert!(ui::INDEX_HTML.contains("\"\u{041d}\u{041e}\u{0412}\u{0415}\""));
        assert!(ui::INDEX_HTML.contains("id=\"arr-new-count\" class=\"hint\""));
        assert!(ui::INDEX_HTML.contains("\u{043d}\u{043e}\u{0432}\u{0438}\u{0445}: "));
        assert!(ui::INDEX_HTML.contains("function unseenNregs(docs, seen)"));
        assert!(ui::INDEX_HTML.contains("rada-arr-seen"));
    }

    #[test]
    fn ui_page_has_arrivals_only_new_toggle() {
        // Checkbox-style «лише нові» chip beside the counter: while active the
        // shared renderer narrows the list to the session newSet on top of the
        // text filter — the click handler flips the state var and repaints
        // through applyArrFilter (which syncs aria-pressed / selected-look).
        assert!(ui::INDEX_HTML.contains("id=\"arr-only-new\""));
        assert!(ui::INDEX_HTML.contains("\u{043b}\u{0438}\u{0448}\u{0435} \u{043d}\u{043e}\u{0432}\u{0456}"));
        assert!(ui::INDEX_HTML.contains(
            "$(\"arr-only-new\").onclick = () => { arrOnlyNew = !arrOnlyNew; applyArrFilter(); };"
        ));
    }

    #[test]
    fn ui_page_has_arrivals_date_groups() {
        // Arrivals feed grouped by publication date: the pure helper returns
        // [dateKey, items] pairs newest-first (unknown «» group last, order
        // kept inside a group), the formatter renders 8-digit YYYYMMDD as
        // dd.mm.yyyy and anything else verbatim, and the renderer paints one
        // muted header row per group («без дати» when the date is missing).
        assert!(ui::INDEX_HTML.contains("function groupArrivalsByDate(items)"));
        assert!(ui::INDEX_HTML.contains("function fmtArrDate(v)"));
        assert!(ui::INDEX_HTML.contains("arr-date-hint"));
        assert!(ui::INDEX_HTML.contains("\u{0431}\u{0435}\u{0437} \u{0434}\u{0430}\u{0442}\u{0438}"));
    }

    #[test]
    fn ui_page_has_arrivals_group_collapse() {
        // Clicking a group header collapses/expands its rows: a session-wide
        // Set of collapsed date keys drives the renderer (a collapsed group
        // paints only its header), the header wires its click to the toggle
        // helper, and that repaints the cached feed through applyArrFilter
        // (the same state pattern as arrOnlyNew).
        assert!(ui::INDEX_HTML.contains("const arrCollapsed = new Set();"));
        assert!(ui::INDEX_HTML.contains("head.onclick = () => toggleArrGroup(key);"));
        assert!(ui::INDEX_HTML.contains("function toggleArrGroup(key)"));
        assert!(ui::INDEX_HTML.contains("if (arrCollapsed.has(key)) arrCollapsed.delete(key);"));
    }

    #[test]
    fn ui_page_has_edition_compare() {
        // «Порівняти редакції» panel (the GUI reader's collapsible edition
        // comparison): a trigger beside «Текст», two selects pre-filled from
        // #editions, and the two pure line builders behind #cmp-result —
        // one «{date}: {words} слів · {chars} симв. · ~{min} хв» line per
        // edition plus the signed «різниця: » line (B minus A).
        assert!(ui::INDEX_HTML.contains("id=\"btn-compare-editions\""));
        assert!(ui::INDEX_HTML.contains("Порівняти редакції"));
        assert!(ui::INDEX_HTML.contains("id=\"cmp-a\""));
        assert!(ui::INDEX_HTML.contains("id=\"cmp-b\""));
        assert!(ui::INDEX_HTML.contains("function cmpSummaryLine("));
        assert!(ui::INDEX_HTML.contains("function cmpDiffLine("));
        assert!(ui::INDEX_HTML.contains("різниця: "));
    }

    #[test]
    fn ui_page_edition_compare_resets_on_open() {
        // openDoc calls resetCompare right next to resetToc: the panel hides,
        // #cmp-result empties, and the trigger re-disables until the next
        // card repopulates #editions — a stale comparison never lingers.
        assert!(ui::INDEX_HTML.contains("function resetCompare("));
        assert!(ui::INDEX_HTML.contains("resetCompare();"));
        assert!(ui::INDEX_HTML.contains("$(\"cmp-result\").innerHTML = \"\";"));
        assert!(ui::INDEX_HTML.contains("$(\"edition-compare\").hidden = true;"));
    }

    #[test]
    fn ui_page_has_recent_queries() {
        // «недавні запити» row under the search filters (the GUI search page
        // parity): the persisted newest-first term list (cap 10, deduped,
        // JSON errors tolerated), the row right after #active-filters, the
        // hint label + «очистити», and the pure push helper.
        assert!(ui::INDEX_HTML.contains("id=\"recent-q\" class=\"row\" hidden"));
        assert!(ui::INDEX_HTML.contains("\u{043D}\u{0435}\u{0434}\u{0430}\u{0432}\u{043D}\u{0456} \u{0437}\u{0430}\u{043F}\u{0438}\u{0442}\u{0438}:"));
        assert!(ui::INDEX_HTML.contains("\u{043E}\u{0447}\u{0438}\u{0441}\u{0442}\u{0438}\u{0442}\u{0438}"));
        assert!(ui::INDEX_HTML.contains("function pushRecentQ(list, term)"));
        assert!(ui::INDEX_HTML.contains("rada-recent-q"));
    }

    #[test]
    fn ui_page_search_records_recent_queries() {
        // Every search records the #q term into storage before the fetch (a
        // failed search still remembers it), and a term chip puts the term
        // back into #q and re-runs the search from the first page; the
        // recorder repaints the row, «очистити» empties the row.
        assert!(ui::INDEX_HTML.contains("if (q) rememberRecentQ(q);"));
        assert!(ui::INDEX_HTML.contains("JSON.stringify(pushRecentQ(getRecentQ(), t))"));
        assert!(ui::INDEX_HTML.contains(r##"$("q").value = t; doSearch(0);"##));
        assert!(ui::INDEX_HTML.contains("renderRecentQ();"));
        assert!(ui::INDEX_HTML.contains("row.hidden = items.length === 0;"));
    }

    #[test]
    fn ui_page_has_related_panel() {
        // «Пов'язані документи» panel in the reader (the GUI's related-docs
        // list): the trigger button, the muted error line's «повторити»
        // retry button that re-fires the fetch, the pure row builder, and
        // its empty-list hint.
        assert!(ui::INDEX_HTML.contains("id=\"btn-related\""));
        assert!(ui::INDEX_HTML.contains("\u{043F}\u{043E}\u{0432}\u{0442}\u{043E}\u{0440}\u{0438}\u{0442}\u{0438}"));
        assert!(ui::INDEX_HTML.contains("function renderRelatedItems("));
        assert!(ui::INDEX_HTML.contains(
            "\u{043F}\u{043E}\u{0441}\u{0438}\u{043B}\u{0430}\u{043D}\u{044C} \u{043D}\u{0430} \u{0456}\u{043D}\u{0448}\u{0456} \u{0430}\u{043A}\u{0442}\u{0438} \u{043D}\u{0435} \u{0437}\u{043D}\u{0430}\u{0439}\u{0434}\u{0435}\u{043D}\u{043E}"
        ));
    }

    #[test]
    fn ui_page_related_resets_on_open() {
        // openDoc wipes the related panel right next to resetToc /
        // resetCompare: the list hides and empties, its loaded-flag clears,
        // and the trigger re-enables — a stale related list (or a stuck
        // disabled button) never follows the user into the next document.
        assert!(ui::INDEX_HTML.contains("resetToc();"));
        assert!(ui::INDEX_HTML.contains("resetCompare();"));
        assert!(ui::INDEX_HTML.contains("$(\"related-list\").hidden = true;"));
        assert!(ui::INDEX_HTML.contains("$(\"related-list\").innerHTML = \"\";"));
        assert!(ui::INDEX_HTML.contains("$(\"related-list\").dataset.loaded = \"\";"));
        assert!(ui::INDEX_HTML.contains("$(\"btn-related\").disabled = false;"));
    }

    #[test]
    fn ui_page_has_hit_copy_button() {
        // Per-hit «копія» quick action on the regular-search rows (the GUI's
        // copy-nreg parity): a chip button inside the row's .hit-actions
        // wrapper that copies nreg through the clipboard API (the textarea/
        // execCommand fallback lives in copyText) and briefly flashes
        // «скопійовано» before restoring, all without opening the document.
        assert!(ui::INDEX_HTML.contains("actions.className = \"hit-actions\";"));
        assert!(ui::INDEX_HTML.contains(".hit-actions {"));
        assert!(ui::INDEX_HTML.contains("cp.textContent = \"копія\""));
        assert!(ui::INDEX_HTML.contains("copyText(h.nreg, cp, \"скопійовано\")"));
        assert!(ui::INDEX_HTML.contains("navigator.clipboard.writeText(text)"));
        assert!(ui::INDEX_HTML.contains("setTimeout(() => { btn.textContent = old;"));
        assert!(ui::INDEX_HTML.contains("ev.stopPropagation(); copyText(h.nreg, cp"));
    }

    #[test]
    fn ui_page_has_hit_site_button() {
        // Per-hit «сайт» quick action (the GUI's open-on-portal parity): the
        // pure siteUrlFor builder feeding a chip anchor in the hit renderer
        // that opens the portal page in a new tab, stopPropagation keeping
        // the row's openDoc click untouched.
        assert!(ui::INDEX_HTML.contains("function siteUrlFor(nreg)"));
        assert!(ui::INDEX_HTML.contains(
            "\"https://zakon.rada.gov.ua/laws/show/\" + encNreg(nreg)"
        ));
        assert!(ui::INDEX_HTML.contains("site.href = siteUrlFor(h.nreg);"));
        assert!(ui::INDEX_HTML.contains("site.target = \"_blank\";"));
        assert!(ui::INDEX_HTML.contains("site.rel = \"noopener\";"));
        assert!(ui::INDEX_HTML.contains("site.textContent = \"сайт\""));
        assert!(ui::INDEX_HTML.contains("target=\"_blank\""));
        assert!(ui::INDEX_HTML.contains("actions.append(cp, site);"));
    }

    #[test]
    fn ui_page_has_reader_nav_buttons() {
        // Reader prev/next through the last results list (the GUI reader's
        // «‹ N / M ›» stepping): two step buttons plus the position hint in
        // the toolbar, the shared fill helper fed by both the search and the
        // cache renderers, and openDoc binding the position (so a doc opened
        // from elsewhere leaves navPos = -1).
        assert!(ui::INDEX_HTML.contains("id=\"nav-prev\""));
        assert!(ui::INDEX_HTML.contains("id=\"nav-next\""));
        assert!(ui::INDEX_HTML.contains("id=\"nav-pos\""));
        assert!(ui::INDEX_HTML.contains(">попередній</button>"));
        assert!(ui::INDEX_HTML.contains(">наступний</button>"));
        assert!(ui::INDEX_HTML.contains("let navList = [];"));
        assert!(ui::INDEX_HTML.contains("let navPos = -1;"));
        assert!(ui::INDEX_HTML.contains("function fillNavList(nregs)"));
        assert!(ui::INDEX_HTML.contains("fillNavList(d.hits.map((h) => h.nreg));"));
        assert!(ui::INDEX_HTML.contains("fillNavList(d.items.map((it) => it.nreg));"));
        assert!(ui::INDEX_HTML.contains("navPos = navList.indexOf(nreg);"));
        assert!(ui::INDEX_HTML.contains("openDoc(navList[j]);"));
    }

    #[test]
    fn ui_page_has_reader_nav_keyboard() {
        // Alt+arrows step the reader through navList (matching the GUI's
        // Alt+Left/Right): the alt guard sits inside the shared keydown
        // listener, the typing-target guard keeps inputs/textarea/select
        // untouched, and both step buttons reuse the same readerNavStep.
        assert!(ui::INDEX_HTML.contains("ev.altKey"));
        assert!(ui::INDEX_HTML.contains("if (!isTypingTarget(ev) && readerNavStep(ev.key === \"ArrowRight\" ? 1 : -1))"));
        assert!(ui::INDEX_HTML.contains("ev.key === \"ArrowLeft\" || ev.key === \"ArrowRight\""));
        assert!(ui::INDEX_HTML.contains("$(\"nav-prev\").onclick = () => readerNavStep(-1);"));
        assert!(ui::INDEX_HTML.contains("$(\"nav-next\").onclick = () => readerNavStep(1);"));
    }

    #[test]
    fn ui_page_has_reader_edition_quicknav() {
        // Quick edition buttons beside #editions (the GUI reader's Alt+Up/Down
        // parity): plain Ukrainian labels without any glyphs, hidden AND
        // disabled in the markup, reset to hidden+disabled by openDoc, and
        // un-hidden by the card loader only when the select really has dated
        // editions (the same < 2 options guard as «Порівняти редакції»).
        assert!(ui::INDEX_HTML.contains("id=\"ed-prev\""));
        assert!(ui::INDEX_HTML.contains("id=\"ed-next\""));
        assert!(ui::INDEX_HTML.contains(
            "<button id=\"ed-prev\" type=\"button\" hidden disabled title=\"попередня редакція\">попередня</button>"
        ));
        assert!(ui::INDEX_HTML.contains(
            "<button id=\"ed-next\" type=\"button\" hidden disabled title=\"наступна редакція\">наступна</button>"
        ));
        // Beside the select: #editions → #ed-prev → #ed-next → «Текст».
        let sel = ui::INDEX_HTML.find("id=\"editions\"").expect("editions select");
        let prev = ui::INDEX_HTML.find("id=\"ed-prev\"").expect("ed-prev");
        let next = ui::INDEX_HTML.find("id=\"ed-next\"").expect("ed-next");
        let load = ui::INDEX_HTML.find("id=\"load-text\"").expect("load-text");
        assert!(sel < prev && prev < next && next < load,
            "both buttons sit between the select and «Текст»");
        // openDoc resets them to hidden+disabled beside the emptied select,
        // so a stale pair never lingers on the next document.
        let open = ui::INDEX_HTML
            .find("async function openDoc(nreg) {")
            .expect("openDoc");
        let end = ui::INDEX_HTML
            .find("async function loadCard() {")
            .expect("loadCard");
        let body = &ui::INDEX_HTML[open..end];
        assert!(body.contains(r#"$("ed-prev").hidden = true;"#), "openDoc hides the quick-nav");
        assert!(body.contains(r#"$("ed-next").hidden = true;"#));
        assert!(body.contains(r#"$("ed-prev").disabled = true;"#), "openDoc disables the quick-nav");
        assert!(body.contains(r#"$("ed-next").disabled = true;"#));
        // The card loader hides them for a single-option select (nothing to
        // cycle) and otherwise enables them.
        assert!(ui::INDEX_HTML.contains("const noEds = sel.options.length < 2;"));
        assert!(ui::INDEX_HTML.contains(r#"$("ed-prev").hidden = noEds;"#));
        assert!(ui::INDEX_HTML.contains(r#"$("ed-next").hidden = noEds;"#));
        assert!(ui::INDEX_HTML.contains(r#"$("ed-prev").disabled = false;"#));
        assert!(ui::INDEX_HTML.contains(r#"$("ed-next").disabled = false;"#));
        // While a paced text fetch runs the pair is disabled (the exportDoc
        // btn-disable idiom) and re-enabled in a finally so an error path
        // can never leave them dead.
        let lt = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("async function loadText(edition) {")
            .expect("loadText")..];
        let lt = &lt[..lt.find("\n}").expect("loadText end")];
        assert!(lt.contains(r#"$("ed-prev").disabled = true;"#), "fetch gates ed-prev");
        assert!(lt.contains(r#"$("ed-next").disabled = true;"#), "fetch gates ed-next");
        assert!(lt.contains("finally {"), "the gate always lifts");
    }

    #[test]
    fn ui_page_has_reader_edition_cycle_pure() {
        // Pure cycleEdition mirrors the GUI's cycle_edition: the combo list
        // ["" поточна, eds0, eds1, …] stepped with wrap-around, null when
        // there is nothing to do (single option or a cur outside the list).
        let f = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function cycleEdition(options, curValue, delta) {")
            .expect("cycleEdition")..];
        let f = &f[..f.find("\n}").expect("cycleEdition end")];
        assert!(f.contains("options.length <= 1"), "single option: nothing to do");
        assert!(f.contains("return null;"), "the null verdict");
        assert!(f.contains("options.indexOf(curValue)"), "cur lookup");
        assert!(f.contains("if (pos < 0) return null;"), "unknown cur: nothing to do");
        assert!(f.contains("(((pos + delta) % len) + len) % len"),
            "wrap-around both directions");
        assert!(f.contains("return options[next];"), "the next value string");
        // One shared step path for both buttons: it reads the LIVE select
        // values, and — because assigning select.value emits no change
        // event — re-runs the dropdown's own loadText change path directly.
        let s = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function stepEdition(delta) {")
            .expect("stepEdition")..];
        let s = &s[..s.find("\n}").expect("stepEdition end")];
        assert!(s.contains("Array.from(sel.options).map((o) => o.value)"), "live values");
        assert!(s.contains("cycleEdition(values, sel.value, delta)"), "single source");
        assert!(s.contains("if (next === null) return;"), "null stays a no-op");
        assert!(s.contains("sel.value = next;"), "the select follows the cycle");
        assert!(s.contains("loadText(next || \"\");"), "same change path as the dropdown");
        // Both buttons feed the same helper with ±1, like the nav steps.
        assert!(ui::INDEX_HTML.contains(r#"$("ed-prev").onclick = () => stepEdition(-1);"#));
        assert!(ui::INDEX_HTML.contains(r#"$("ed-next").onclick = () => stepEdition(1);"#));
    }

    #[test]
    fn ui_page_has_query_highlight_helper() {
        // Pure markTerms builder (browser-find style painting of query words in
        // ALREADY-ESCAPED title html): terms capped at 8 with a 3-char minimum,
        // regex-escaped longest-first alternation, case-insensitive Unicode
        // whole-word match ("giu" + \p{L}/\p{N} lookarounds), and a split on
        // &-led entity runs so a hit can never cut inside e.g. &amp;.
        assert!(ui::INDEX_HTML.contains("function markTerms(textHtml, terms)"));
        assert!(ui::INDEX_HTML.contains("mark class=\"qhl\""));
        assert!(ui::INDEX_HTML.contains(".filter((t) => t.length >= 3).slice(0, 8)"));
        assert!(ui::INDEX_HTML.contains(
            ".replace(/[.*+?^${}()|[\\]\\\\]/g, \"\\\\$&\")"
        ));
        assert!(ui::INDEX_HTML.contains("\"giu\""));
        assert!(ui::INDEX_HTML.contains("if (i % 2 === 1) { out += seg; continue; }"));
        assert!(ui::INDEX_HTML.contains(
            "mark.qhl { background: var(--find-bg); color: var(--find-fg);"
        ));
    }

    #[test]
    fn ui_page_has_query_highlight_in_renderer() {
        // The regular-search hit renderer paints titles through markTerms: the
        // terms derive from #q once per render (trimmed value lowercased, split
        // on whitespace, filler <3 chars dropped), the title is escaped before
        // wrapping, and the plain textContent path stays for queries without a
        // usable term.
        assert!(ui::INDEX_HTML.contains("const q = $(\"q\").value.trim();"));
        assert!(ui::INDEX_HTML.contains(
            "const queryTerms = q.toLowerCase().split(/\\s+/).filter((t) => t.length >= 3);"
        ));
        assert!(ui::INDEX_HTML.contains(
            "if (queryTerms.length) a.innerHTML = markTerms(escHtml(h.nazva || h.nreg), queryTerms);"
        ));
        assert!(ui::INDEX_HTML.contains("else a.textContent = h.nazva || h.nreg;"));
    }

    #[test]
    fn ui_page_has_cache_search_highlight() {
        // Cache-search rows paint the searched term browser-find style just
        // like the regular hits: the terms derive from the captured cacheTerm
        // (lowercased, split on whitespace, filler <3 chars dropped, the
        // 8-term cap living inside markTerms), title and snippet go through
        // markTerms over escaped html, the plain textContent paths stay for
        // queries without a usable term, and the row's openDoc click handler
        // is untouched by the innerHTML swap.
        assert!(ui::INDEX_HTML.contains("let cacheTerm = \"\";"));
        assert!(ui::INDEX_HTML.contains(
            "const queryTerms = cacheTerm.toLowerCase().split(/\\s+/).filter((t) => t.length >= 3);"
        ));
        assert!(ui::INDEX_HTML.contains(
            "if (queryTerms.length) a.innerHTML = markTerms(escHtml(it.title || it.nreg), queryTerms);"
        ));
        assert!(ui::INDEX_HTML.contains("else a.textContent = it.title || it.nreg;"));
        assert!(ui::INDEX_HTML.contains(
            "if (queryTerms.length) snip.innerHTML = markTerms(escHtml(it.snippet), queryTerms);"
        ));
        assert!(ui::INDEX_HTML.contains("else snip.textContent = it.snippet;"));
        assert!(ui::INDEX_HTML.contains(
            "a.onclick = (ev) => { ev.preventDefault(); openDoc(it.nreg); };"
        ));
    }

    #[test]
    fn ui_page_has_cache_search_term_capture() {
        // doCacheSearch captures the trimmed #q value into cacheTerm right
        // after reading it and before the fetch goes out, so rows rendered
        // after the response still highlight the term the search actually ran
        // with even when the input changes meanwhile.
        let body = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("async function doCacheSearch()")
            .expect("doCacheSearch")..];
        let capture = body.find("cacheTerm = q;").expect("cacheTerm capture");
        assert!(body
            .find("const q = $(\"q\").value.trim();")
            .expect("#q read")
            < capture);
        assert!(capture < body.find("/api/cache/search?q=").expect("cache endpoint"));
    }

    #[test]
    fn ui_page_has_arrivals_auto_refresh() {
        // «автооновлення» chip on the arrivals controls row: the choice is
        // persisted in "rada-arr-auto" ("1"/"0", privacy-mode errors
        // tolerated); ONE setInterval is created at load and each tick
        // re-runs the manual loadArrivals path (so the «НОВЕ» badge diff is
        // untouched) only while the chip is active, the arrivals pane is the
        // visible tab and the page is not hidden, with a busy flag against
        // overlapping fetches.
        assert!(ui::INDEX_HTML.contains("id=\"arr-auto\""));
        assert!(ui::INDEX_HTML.contains("\u{0430}\u{0432}\u{0442}\u{043e}\u{043e}\u{043d}\u{043e}\u{0432}\u{043b}\u{0435}\u{043d}\u{043d}\u{044f}"));
        assert!(ui::INDEX_HTML.contains("\"rada-arr-auto\""));
        assert!(ui::INDEX_HTML.contains("setInterval(arrAutoTick, ARR_AUTO_MS);"));
        assert!(ui::INDEX_HTML.contains("try { await loadArrivals(); } finally { arrAutoBusy = false; }"));
        assert!(ui::INDEX_HTML.contains(
            "if (!arrAuto || document.hidden || $(\"tab-arrivals\").hidden || arrAutoBusy) return;"
        ));
    }

    #[test]
    fn ui_page_has_arrivals_auto_hint() {
        // Muted «щохвилини» hint beside the chip: the click handler flips the
        // persisted state through the shared setter, which syncs the chip
        // .active look, aria-pressed and the hint visibility together, so
        // the hint reads exactly while the interval condition can fire.
        assert!(ui::INDEX_HTML.contains("id=\"arr-auto-hint\" class=\"hint\""));
        assert!(ui::INDEX_HTML.contains("\u{0449}\u{043e}\u{0445}\u{0432}\u{0438}\u{043b}\u{0438}\u{043d}\u{0438}"));
        assert!(ui::INDEX_HTML.contains("$(\"arr-auto\").onclick = () => setArrAuto(!arrAuto);"));
        assert!(ui::INDEX_HTML.contains("$(\"arr-auto\").classList.toggle(\"active\", on);"));
        assert!(ui::INDEX_HTML.contains("$(\"arr-auto-hint\").hidden = !on;"));
    }

    #[test]
    fn ui_page_has_arrivals_freshness_label() {
        // «оновлено: …» freshness label beside the arrivals counters (the
        // GUI's parity): muted hint span, hidden by default until the first
        // successful load; the shared loadArrivals stamps arrFetchedAt once,
        // inside the try after the live API call succeeds (so a failed load
        // never pretends the feed is fresh) and manual + auto-refresh ticks
        // share that single success point.
        assert!(ui::INDEX_HTML.contains("let arrFetchedAt = 0;"));
        assert!(ui::INDEX_HTML.contains("id=\"arr-fresh\" class=\"hint\" hidden"));
        assert!(ui::INDEX_HTML.contains("оновлено: "));
        assert!(ui::INDEX_HTML.contains("function updateArrFresh()"));
        let body = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("async function loadArrivals()")
            .expect("loadArrivals")..];
        let stamp = body.find("arrFetchedAt = Date.now();").expect("freshness stamp");
        assert!(stamp > body
            .find("const d = await api(\"/api/arrivals?limit=100\");")
            .expect("api call"));
        assert!(stamp < body.find("catch (e)").expect("catch"));
    }

    #[test]
    fn ui_page_has_arrivals_freshness_since_helper() {
        // Pure fmtSince builder with the GUI's exact wording: «щойно» under a
        // minute, «N хв тому» under an hour, «N год тому» under a day, «N дн
        // тому» beyond (integer floors). A lightweight 30 s tick repaints the
        // label with the same visibility guard as arrAutoTick, so a hidden
        // page or a hidden arrivals pane never touches the DOM.
        assert!(ui::INDEX_HTML.contains("function fmtSince(nowMs, thenMs)"));
        assert!(ui::INDEX_HTML.contains("щойно"));
        assert!(ui::INDEX_HTML.contains(" хв тому"));
        assert!(ui::INDEX_HTML.contains(" год тому"));
        assert!(ui::INDEX_HTML.contains(" дн тому"));
        assert!(ui::INDEX_HTML.contains("setInterval(arrFreshTick, 30000);"));
        assert!(ui::INDEX_HTML.contains(
            "function arrFreshTick() {\n  if (document.hidden || $(\"tab-arrivals\").hidden) return;"
        ));
        assert!(ui::INDEX_HTML.contains("  updateArrFresh();\n}\n\nsetInterval(arrFreshTick, 30000);"));
    }

    #[test]
    fn ui_page_has_arrivals_csv_button() {
        // «експорт .csv» chip on the arrivals controls row: a plain-text
        // button living beside the «лише нові» / «автооновлення» chips (same
        // placement row and chip styling language), clicking it runs the
        // client-side export through the shared exportArrivalsCsv entry point.
        assert!(ui::INDEX_HTML.contains("<button id=\"arr-export-csv\" type=\"button\""));
        assert!(ui::INDEX_HTML.contains(">експорт .csv</button>"));
        // Same toolbar row as the filter + «лише нові» + «автооновлення»
        // controls: everything between #arr-filter and the feed container.
        let row = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("id=\"arr-filter\"")
            .expect("arr-filter")..ui::INDEX_HTML
            .find("<div id=\"arrivals\"></div>")
            .expect("arrivals list")];
        for id in ["arr-filter-clear", "arr-only-new", "arr-auto", "arr-export-csv"] {
            assert!(row.contains(&format!("id=\"{id}\"")), "{id}: lives on the arrivals controls row");
        }
        let only = row.find("id=\"arr-only-new\"").expect("only-new chip");
        let auto = row.find("id=\"arr-auto\"").expect("auto chip");
        let csv = row.find("id=\"arr-export-csv\"").expect("csv chip");
        assert!(only < auto && auto < csv, "the export chip trails the «лише нові» / «автооновлення» chips");
        // Same chip styling language as the other two toggles.
        assert!(ui::INDEX_HTML.contains("#arr-export-csv {"), "chip CSS reuses the arrivals chip language");
        assert!(ui::INDEX_HTML.contains("#arr-export-csv:hover {"), "accent hover like the other chips");
        assert!(ui::INDEX_HTML.contains(
            "$(\"arr-export-csv\").onclick = () => exportArrivalsCsv();"
        ), "click goes through the shared export entry point");
    }

    #[test]
    fn ui_page_has_arrivals_csv_builder() {
        // ONE pure arrivalsCsv(docs) builder mirroring the GUI arrivals_csv
        // (rada-gui src/views/arrivals.rs): UTF-8 BOM up front so Excel opens
        // Cyrillic, the «nreg,status,nazva,publics» header, comma separator,
        // and fields quoted with doubled quotes when they contain a quote,
        // comma or line break. An empty visible list still yields the
        // header-only file (no toast, no abort). The exporter feeds it the
        // on-screen rows through visibleArrivals (the shared arrFilterMatch
        // text predicate + the «лише нові» gate) and hands the BOM'd text
        // over through the favorites .txt export's Blob / object-URL idiom.
        assert!(ui::INDEX_HTML.contains("function arrivalsCsv(docs)"));
        let b = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function arrivalsCsv(docs) {")
            .expect("arrivalsCsv")..];
        let b = &b[..b.find("\n}").expect("arrivalsCsv end")];
        // BOM + header row mirroring the GUI columns, comma separator.
        assert!(b.contains(r#""\uFEFFnreg,status,nazva,publics\n""#), "BOM + the GUI header row");
        assert!(b.contains("nreg,status,nazva,publics"), "the GUI arrivals_csv header columns");
        assert!(b.contains("arrCsvEsc(d.nreg) + \",\" + arrCsvEsc(d.status)"), "comma separator between escaped fields");
        assert!(b.contains("arrCsvEsc(d.nazva) + \",\" + arrCsvEsc(d.publics)"), "nazva + publics columns");
        // Quote-doubling escaper gated on quote/comma/CR/LF (GUI csv_esc parity).
        let e = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function arrCsvEsc(s) {")
            .expect("arrCsvEsc")..];
        let e = &e[..e.find("\n}").expect("arrCsvEsc end")];
        assert!(e.contains(r#"/["\n\r,]/.test(v)"#), "quote, comma and line breaks trigger quoting");
        assert!(e.contains(r#".replace(/"/g, "\"\"")"#), "quotes are doubled inside the field");
        // The exporter exports exactly the on-screen list: the shared
        // arrFilterMatch text predicate plus the arrNewSet «лише нові» gate.
        assert!(ui::INDEX_HTML.contains("function visibleArrivals()"));
        let v = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function visibleArrivals() {")
            .expect("visibleArrivals")..];
        let v = &v[..v.find("\n}").expect("visibleArrivals end")];
        assert!(v.contains("arrivalsDocs.filter((d) => arrFilterMatch(d, needle))"), "the shared text-filter predicate");
        assert!(v.contains("arrOnlyNew ? items.filter((d) => arrNewSet.has(d.nreg)) : items"), "the «лише нові» gate");
        let x = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function exportArrivalsCsv() {")
            .expect("exportArrivalsCsv")..];
        let x = &x[..x.find("\n}").expect("exportArrivalsCsv end")];
        assert!(x.contains("arrivalsCsv(visibleArrivals());"), "exports the filtered list, not the raw feed");
        // Blob download through the favorites .txt idiom under the fixed name.
        assert!(x.contains("new Blob([csv], { type: \"text/csv;charset=utf-8\" });"), "the BOM'd text ships as the Blob content");
        assert!(x.contains("URL.createObjectURL(blob)"));
        assert!(x.contains("a.download = \"novy-dokumenty.csv\";"), "fixed Cyrillic-safe file name");
        assert!(x.contains("a.click();"));
        assert!(x.contains("URL.revokeObjectURL(url);"));
        // Empty visible list: the header-only download is deliberate (no
        // toast/abort) — documented in the builder's comment.
        assert!(ui::INDEX_HTML.contains("header-only"), "the empty-list choice is documented");
    }

    #[test]
    fn ui_page_has_citation_button() {
        // «цилата» chip in the reader toolbar beside the copy-text/print
        // buttons (the GUI reader's context-menu copy-citation parity): no
        // document open keeps it disabled in the markup, openDoc re-disables
        // it while the card loads (a stale title must never be citable) and
        // the card success re-enables it; the click feeds the live doc's
        // citation through the shared copyText, which flashes «скопійовано»
        // on the button and restores the label.
        assert!(ui::INDEX_HTML.contains("id=\"btn-cite\" type=\"button\" disabled"));
        assert!(ui::INDEX_HTML.contains(">цилата</button>"));
        assert!(ui::INDEX_HTML.contains("$(\"btn-cite\").disabled = true;"));
        assert!(ui::INDEX_HTML.contains("$(\"btn-cite\").disabled = false;"));
        assert!(ui::INDEX_HTML.contains("$(\"btn-cite\").onclick = () => {"));
        assert!(ui::INDEX_HTML.contains("if (!state.curNreg || !state.curCard) return;"));
        assert!(ui::INDEX_HTML.contains(
            "citationText(state.curCard.nazva, state.curNreg, $(\"editions\").value)"
        ));
        assert!(ui::INDEX_HTML.contains("$(\"btn-cite\"), \"скопійовано\");"));
    }

    #[test]
    fn ui_page_has_citation_helper() {
        // Pure citationText builder: «title» // Офіційний вебпортал
        // парламенту України. plus an optional «Редакція від dd.mm.yyyy, »
        // part (the raw edition label runs through fmtArrDate, which formats
        // 8-digit YYYYMMDD and passes anything else verbatim; an empty label
        // omits the part) and a «URL: » line built by the shared siteUrlFor
        // portal-URL builder, with trailing spaces trimmed off.
        assert!(ui::INDEX_HTML.contains("function citationText(title, nreg, edition)"));
        assert!(ui::INDEX_HTML.contains("Офіційний вебпортал парламенту України"));
        assert!(ui::INDEX_HTML.contains("\"Редакція від \""));
        assert!(ui::INDEX_HTML.contains("fmtArrDate(ed)"));
        assert!(ui::INDEX_HTML.contains("\"URL: \" + siteUrlFor(nreg)"));
        assert!(ui::INDEX_HTML.contains(".trimEnd()"));
    }

    #[test]
    fn ui_page_has_hash_deep_link_parse() {
        // Pure parseHash helper for shareable fragments: both fields default
        // to null, the leading '#' is optional, parameters are separated by
        // '&' and split on the FIRST '=' each (values may contain '='),
        // unknown keys are ignored, and decodeURIComponent sits inside a
        // try/catch so a bad % sequence drops the parameter instead of
        // throwing (a malformed hash reads like no hash at all).
        assert!(ui::INDEX_HTML.contains("function parseHash(h)"));
        assert!(ui::INDEX_HTML.contains("{ doc: null, q: null }"));
        assert!(ui::INDEX_HTML.contains("charAt(0) === \"#\""));
        assert!(ui::INDEX_HTML.contains("split(\"&\")"));
        assert!(ui::INDEX_HTML.contains("indexOf(\"=\")"));
        assert!(ui::INDEX_HTML.contains("key !== \"doc\" && key !== \"q\""));
        assert!(ui::INDEX_HTML.contains("try { val = decodeURIComponent("));
        assert!(ui::INDEX_HTML.contains("} catch (_) { continue; }"));
    }

    #[test]
    fn ui_page_has_hash_deep_link_apply() {
        // The hash is read once during init (after the normal startup has
        // run): "#doc=" opens the reader on the «Пошук» tab and "#q=" fills
        // the query box and searches, doc winning when both are present.
        // No hashchange listener exists, so the openDoc/doSearch hash sync
        // (replaceState, so no history entry and no anchor jump) can never
        // feed back into the on-load application.
        assert!(ui::INDEX_HTML.contains(
            "parseHash(window.location.hash || location.hash)"
        ));
        assert!(ui::INDEX_HTML.contains("if (h.doc)"));
        assert!(ui::INDEX_HTML.contains("openDoc(h.doc);"));
        assert!(ui::INDEX_HTML.contains("} else if (h.q) {"));
        assert!(ui::INDEX_HTML.contains("$(\"q\").value = h.q;"));
        assert!(ui::INDEX_HTML.contains("doSearch(0);"));
        assert!(!ui::INDEX_HTML.contains("hashchange"));
        assert!(ui::INDEX_HTML.contains("\"#doc=\" + encNreg(nreg)"));
        assert!(ui::INDEX_HTML.contains("\"#q=\" + encodeURIComponent(q)"));
    }

    #[test]
    fn ui_page_has_help_button() {
        // Header «клавіші» toggle and the hidden-by-default shortcut popover:
        // the click handler flips the hidden attribute, a document listener
        // closes it on any click outside (the toggle button itself is exempt
        // and stops propagation so the same click cannot fight itself), and
        // Escape closes it only while it is visible — the find bar's own
        // Escape-to-clear listener on #find-input stays untouched.
        assert!(ui::INDEX_HTML.contains("id=\"btn-help\""));
        assert!(ui::INDEX_HTML.contains(">клавіші</button>"));
        assert!(ui::INDEX_HTML.contains("id=\"help-pop\" hidden"));
        assert!(ui::INDEX_HTML.contains("$(\"btn-help\").onclick"));
        assert!(ui::INDEX_HTML.contains(
            "$(\"help-pop\").hidden = !$(\"help-pop\").hidden;"
        ));
        assert!(ui::INDEX_HTML.contains("if (pop.hidden) return;"));
        assert!(ui::INDEX_HTML.contains("pop.contains(t) || t === $(\"btn-help\")"));
        assert!(ui::INDEX_HTML.contains("if (!pop.hidden && ev.key === \"Escape\")"));
    }

    #[test]
    fn ui_page_has_help_rows_source() {
        // The popover renders from one source of truth: the pure helpRows()
        // list of [keys, description] pairs mirroring the real keydown
        // listeners (Ctrl+F find focus, Alt+arrows and plain-arrow reader
        // stepping, Enter/Shift+Enter find stepping, Esc find clear, Enter
        // nreg resolve); renderHelp() maps over it and builds every row as
        // help-row / help-keys spans without innerHTML.
        assert!(ui::INDEX_HTML.contains("function helpRows()"));
        assert!(ui::INDEX_HTML.contains("\"Ctrl+F\""));
        assert!(ui::INDEX_HTML.contains("\"Alt+← / Alt+→\""));
        assert!(ui::INDEX_HTML.contains("Alt+←"));
        assert!(ui::INDEX_HTML.contains("Alt+→"));
        assert!(ui::INDEX_HTML.contains("попередній/наступний документ"));
        assert!(ui::INDEX_HTML.contains("for (const [keys, desc] of helpRows())"));
        assert!(ui::INDEX_HTML.contains("row.className = \"help-row\""));
        assert!(ui::INDEX_HTML.contains("k.className = \"help-keys\""));
        assert!(ui::INDEX_HTML.contains("renderHelp();"));
    }

    #[test]
    fn ui_page_has_scroll_top_button() {
        // Floating «догори» button for the reader (acts run to millions of
        // characters): hidden by default with a plain-text label, fixed to
        // the free bottom-right viewport corner and styled with theme
        // variables only, the pure TOP_SHOW_AT show threshold, and the
        // scroll listener on `window` — the element that actually scrolls
        // #doc-text (no overflow container around it, so the outline's
        // scrollIntoView jumps bubble up to the window scroller too).
        assert!(ui::INDEX_HTML.contains(
            "<button id=\"btn-top\" type=\"button\" hidden>догори</button>"
        ));
        let css = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("#btn-top {")
            .expect("btn-top css")..];
        let css = &css[..css.find('}').expect("btn-top css end")];
        assert!(css.contains("position: fixed;"));
        assert!(css.contains("right: 16px; bottom: 16px;"));
        assert!(css.contains("background: var(--panel2);"));
        assert!(ui::INDEX_HTML.contains(
            "#btn-top:hover { color: var(--text); border-color: var(--accent); }"
        ));
        assert!(ui::INDEX_HTML.contains("const TOP_SHOW_AT = 800;"));
        assert!(ui::INDEX_HTML.contains(
            "window.addEventListener(\"scroll\", updateTopButton);"
        ));
    }

    #[test]
    fn ui_page_has_scroll_top_actions() {
        // Clicking «догори» hides the button at once (no waiting out the
        // smooth ride) and smooth-scrolls that same window scroller back to
        // the top; the show/hide test compares the window's scrollTop with
        // TOP_SHOW_AT; openDoc hides the button, since a freshly opened
        // document starts at the top anyway.
        assert!(ui::INDEX_HTML.contains(
            "$(\"btn-top\").hidden = scrollTop <= TOP_SHOW_AT;"
        ));
        assert!(ui::INDEX_HTML.contains(
            "window.scrollTo({ top: 0, behavior: \"smooth\" });"
        ));
        assert!(ui::INDEX_HTML.contains(
            "$(\"btn-top\").hidden = true; // a fresh document starts at the top anyway"
        ));
        let body = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("$(\"btn-top\").onclick = () => {")
            .expect("btn-top click")..];
        let hide = body
            .find("$(\"btn-top\").hidden = true;")
            .expect("immediate hide on click");
        let scroll = body
            .find("window.scrollTo({ top: 0")
            .expect("smooth scroll to top");
        assert!(hide < scroll);
    }

    #[test]
    fn ui_page_has_reader_cached_badge() {
        // Muted «кешовано» hint beside the reader's stats label (the GUI
        // reader's badge parity): plain text, hidden by default, and reset
        // to hidden by openDoc (and by every fresh text load) so a stale
        // hint never follows the user into the next document.
        assert!(ui::INDEX_HTML
            .contains("<span id=\"doc-cached\" class=\"hint\" hidden>кешовано</span>"));
        assert!(ui::INDEX_HTML.contains("$(\"doc-cached\").hidden = true;"));
        let open = ui::INDEX_HTML
            .find("async function openDoc(nreg) {")
            .expect("openDoc");
        let end = ui::INDEX_HTML
            .find("async function loadCard() {")
            .expect("loadCard");
        assert!(open < end);
        assert!(
            ui::INDEX_HTML[open..end].contains(r#"$("doc-cached").hidden = true;"#),
            "openDoc must hide the cached hint"
        );
    }

    #[test]
    fn ui_page_has_reader_cached_source() {
        // The hint un-hides ONLY in the from-cache branch of the text
        // loader: the body counts as cache-served only when the text
        // response itself reports `cached`, and the reader then asks
        // /api/cached?nreg= to confirm before un-hiding — the mere
        // existence of a copy never lights it up, and a failed probe stays
        // silent (.catch swallows it, the reader never blocks).
        assert!(ui::INDEX_HTML.contains("function showCachedHint(nreg, d)"));
        assert!(ui::INDEX_HTML.contains("d.cached !== true"));
        assert!(ui::INDEX_HTML.contains("/api/cached?nreg="));
        assert!(ui::INDEX_HTML.contains("s.cached === true"));
        assert!(ui::INDEX_HTML.contains(".catch(() => {})"));
        let hint = ui::INDEX_HTML
            .find("function showCachedHint(nreg, d)")
            .expect("showCachedHint");
        let body = &ui::INDEX_HTML[hint..];
        let guard = body.find("d.cached !== true").expect("live guard");
        let probe = body.find("/api/cached?nreg=").expect("source probe");
        let show = body
            .find(r#"$("doc-cached").hidden = false;"#)
            .expect("unhide only in the from-cache branch");
        assert!(guard < probe, "the probe fires only after the live guard");
        assert!(probe < show, "the hint opens only inside the probe's from-cache reply");
        // The route itself (read-only): 405 on POST, 400 without nreg, and
        // {"cached":bool} mirroring docs_text (seeded directly, no network).
        let st = favorites_state("cached");
        let get = |path: &str, query: &str| -> (u16, Value) {
            let mut req: Request = TestRequest::new()
                .with_method(Method::Get)
                .with_path(path)
                .into();
            let resp = route(&Method::Get, path, &parse_query(query), &mut req, &st).unwrap();
            let code = resp.status_code().0;
            let bytes = resp.into_reader().into_inner();
            (code, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
        };
        let (code, _) = call(&st, Method::Post, "/api/cached", "");
        assert_eq!(code, 405);
        let (code, v) = get("/api/cached", "");
        assert_eq!(code, 400, "{v}");
        let (code, v) = get("/api/cached", "nreg=2341-14");
        assert_eq!(code, 200, "{v}");
        assert_eq!(v["cached"], false, "no rows yet: {v}");
        {
            let conn = db_open(&st.db_path).unwrap();
            rada_db::textcache::put(&conn, "2341-14", "", "кешоване тіло").unwrap();
        }
        let (code, v) = get("/api/cached", "nreg=2341-14");
        assert_eq!(code, 200, "{v}");
        assert_eq!(v["cached"], true, "cached copy: {v}");
        let (code, v) = get("/api/cached", "nreg=9-9");
        assert_eq!(code, 200, "{v}");
        assert_eq!(v["cached"], false, "unknown nreg stays false: {v}");
    }

    #[test]
    fn ui_page_has_export_buttons() {
        // «.md» / «.txt» / «.html» chips in the reader toolbar right after
        // the copy-text/цилата buttons (the GUI reader's export_document
        // parity): disabled in the markup until a document is open, re-
        // disabled by openDoc while the card loads (a stale title must never
        // leak into a file) and enabled on card success — the exact btn-cite
        // mechanics; each click feeds its format through the shared
        // exportDoc builder with the button itself as the busy target.
        for (id, fmt) in [
            ("btn-export-md", "md"),
            ("btn-export-txt", "txt"),
            ("btn-export-html", "html"),
        ] {
            assert!(
                ui::INDEX_HTML
                    .contains(&format!("<button id=\"{id}\" type=\"button\" disabled title=\"")),
                "{id}: disabled in the markup: {id}"
            );
            assert!(
                ui::INDEX_HTML.contains(&format!("$(\"{id}\").disabled = true; //")),
                "{id}: re-disabled while a new card loads"
            );
            assert!(
                ui::INDEX_HTML.contains(&format!("$(\"{id}\").disabled = false;")),
                "{id}: enabled on card success"
            );
            assert!(
                ui::INDEX_HTML.contains(&format!(
                    "$(\"{id}\").onclick = () => exportDoc(\"{fmt}\", $(\"{id}\"));"
                )),
                "{id}: click goes through the shared exportDoc"
            );
        }
        assert!(ui::INDEX_HTML.contains(">.md</button>"));
        assert!(ui::INDEX_HTML.contains(">.txt</button>"));
        assert!(ui::INDEX_HTML.contains(">.html</button>"));
    }

    #[test]
    fn ui_page_has_export_document_builder() {
        // Shared exportDoc(fmt, btn): the body comes from the loader cache
        // (state.curText) or, when not fetched yet, from the reader's own
        // text endpoint through api(); the md file opens with the «# title»
        // heading and the «> nreg — rada-client» line, the txt file with a
        // 60 '=' rule under the «title (nreg)» line, and the html file is a
        // minimal standalone page whose title/nreg go through escHtml and
        // whose paragraphs are split on blank lines and escHtml'd; the file
        // is then handed over as a Blob object URL under the sanitized
        // «{nreg}.{fmt}» name and revoked right after the click.
        assert!(ui::INDEX_HTML.contains("async function exportDoc(fmt, btn)"));
        assert!(ui::INDEX_HTML.contains("if (!state.curNreg || !state.curCard) return;"));
        assert!(ui::INDEX_HTML.contains("let body = state.curText;"));
        assert!(ui::INDEX_HTML.contains("\"/api/doc/\" + encNreg(nreg) + \"/text\""));
        // md: heading + metadata line
        assert!(ui::INDEX_HTML.contains("\"# \" + title"));
        assert!(ui::INDEX_HTML.contains("+ nreg + \" \u{2014} rada-client\""));
        // txt: title (nreg) + the 60-column rule
        assert!(ui::INDEX_HTML.contains("const EXPORT_RULE = \""));
        assert!(ui::INDEX_HTML.contains(&"=".repeat(60)));
        assert!(ui::INDEX_HTML.contains("title + \" (\" + nreg + \")\\n\" + EXPORT_RULE"));
        // html: standalone page, escaped strings, blank-line paragraph split
        assert!(ui::INDEX_HTML.contains("<!doctype html>"));
        assert!(ui::INDEX_HTML.contains("escHtml(title)"));
        assert!(ui::INDEX_HTML.contains("escHtml(nreg)"));
        assert!(ui::INDEX_HTML.contains(".split(/\\n\\s*\\n/)"));
        assert!(ui::INDEX_HTML.contains("\"<p>\" + escHtml(p.trim())"));
        // Blob download flow with the {nreg}.{fmt} name and the revoke
        assert!(ui::INDEX_HTML.contains("new Blob([doc], { type:"));
        assert!(ui::INDEX_HTML.contains("URL.createObjectURL(blob)"));
        assert!(ui::INDEX_HTML.contains(
            "a.download = nreg.replace(/[^A-Za-z0-9._-]/g, \"_\") + \".\" + fmt;"
        ));
        assert!(ui::INDEX_HTML.contains("a.click();"));
        assert!(ui::INDEX_HTML.contains("URL.revokeObjectURL(url);"));
    }

    #[test]
    fn ui_page_has_find_match_counter() {
        // The web find bar mirrors the GUI reader's «{cur} з {total}» counter
        // (the muted span right after #find-input, beside the prev/next
        // buttons): hidden by default in the markup, «нічого не знайдено»
        // for a query without hits, the pure countMatches(haystack, needle)
        // non-overlapping case-insensitive scan as the total's source, and
        // openDoc resetting both text and hidden so no stale «5 з 9» from a
        // previous act follows the user into the next document.
        assert!(ui::INDEX_HTML.contains(
            "<span id=\"find-count\" class=\"hint\" hidden></span>"
        ));
        assert!(ui::INDEX_HTML.contains("нічого не знайдено"));
        assert!(ui::INDEX_HTML.contains("function countMatches(haystack, needle)"));
        assert!(ui::INDEX_HTML.contains("function setFindCount()"));
        let cm = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function countMatches(haystack, needle) {")
            .expect("countMatches")..];
        let cm = &cm[..cm.find("\n}").expect("countMatches end")];
        assert!(cm.contains("toLowerCase()"), "case-insensitive scan");
        assert!(cm.contains("indexOf(lowQ, i + needle.length)"), "non-overlapping");
        let open = ui::INDEX_HTML
            .find("async function openDoc(nreg) {")
            .expect("openDoc");
        let end = ui::INDEX_HTML
            .find("async function loadCard() {")
            .expect("loadCard");
        let body = &ui::INDEX_HTML[open..end];
        assert!(
            body.contains(r#"$("find-count").textContent = "";"#),
            "openDoc must clear the counter text"
        );
        assert!(
            body.contains(r#"$("find-count").hidden = true;"#),
            "openDoc must hide the counter"
        );
    }

    #[test]
    fn ui_page_has_find_counter_updates() {
        // The counter refreshes in the one shared place: setFindCount paints
        // «{cur} з {total}» from the existing findIdx state and the total
        // from countMatches over the loaded text (the rendered match list
        // only as fallback), runFind calls it on every query execution
        // after the hits are rebuilt, and showFind calls it after next/prev
        // move findIdx — so the number can never drift from the highlight.
        let f = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function setFindCount() {")
            .expect("setFindCount")..];
        let f = &f[..f.find("\n}").expect("setFindCount end")];
        assert!(f.contains("\" з \""), "the «N з M» separator");
        assert!(f.contains("findIdx + 1"), "current from the index state");
        assert!(f.contains("countMatches(state.curText, q)"), "total from countMatches");
        assert!(f.contains("findHits.length"), "match list length as fallback");
        assert!(f.contains("el.hidden = true;"), "empty query hides it");
        let r = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function runFind() {")
            .expect("runFind")..];
        let r = &r[..r.find("\n}").expect("runFind end")];
        let build = r.find("findHits.push(m);").expect("hit collection");
        let paint = r.rfind("setFindCount();").expect("counter refresh on query");
        assert!(build < paint, "the counter repaints after the hits are rebuilt");
        let s = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function showFind(delta, scroll) {")
            .expect("showFind")..];
        let s = &s[..s.find("\n}").expect("showFind end")];
        let step = s
            .find("findIdx = (findIdx + delta + findHits.length) % findHits.length;")
            .expect("index step");
        let repaint = s.find("setFindCount();").expect("counter refresh on next/prev");
        assert!(step < repaint, "the counter follows the moved index");
    }

    #[test]
    fn ui_page_has_status_quick_chips() {
        // «статус» quick chips row under the search filters: one-click
        // shortcuts for the frequent statuses, built DYNAMICALLY from the
        // dictionary options already loaded into #f-status (the first
        // STATUS_CHIPS_MAX real entries after the empty «усі» option, label
        // from the dict label, value = the dict id — no hardcoded ids) plus
        // a trailing «Усі» chip that clears the select; the chips reuse the
        // existing .chip styling language with the accent for the selection.
        assert!(ui::INDEX_HTML.contains("<div id=\"status-chips\" class=\"row\" hidden></div>"));
        assert!(ui::INDEX_HTML.contains("const STATUS_CHIPS_MAX = 3;"));
        let b = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function buildStatusChips() {")
            .expect("buildStatusChips")..];
        let b = &b[..b.find("\n}").expect("buildStatusChips end")];
        assert!(b.contains(r#"for (const o of $("f-status").options)"#), "chips build from the status select options");
        assert!(b.contains("if (!o.value) continue;"), "the empty «усі» option is skipped");
        assert!(b.contains("items.length >= STATUS_CHIPS_MAX"), "at most three dict chips");
        assert!(b.contains("statusLabel(o.value)"), "chip label from the dict label, not a hardcoded string");
        assert!(b.contains(r#"mk("", "Усі""#), "the trailing «Усі» chip (value \"\")");
        assert!(b.contains("b.className = \"chip\";"), "shared chip styling class");
        assert!(ui::INDEX_HTML.contains("#status-chips .chip {"), "chip CSS reuses the row-chip language");
        assert!(ui::INDEX_HTML.contains("#status-chips .chip.active {"), "the selected chip takes the accent");
    }

    #[test]
    fn ui_page_has_status_chips_sync() {
        // Clicking a chip sets #f-status and re-runs the current search
        // through the existing entry point (doSearch(0), the same call the
        // «Активні фільтри» chips make); the chips are rebuilt once the
        // dictionary load finished, painted from the select's state on every
        // search run (deep-link preselects and active-filter clears all go
        // through doSearch) and on dropdown changes; «Усі» (empty value) is
        // the active chip while the select is empty.
        assert!(ui::INDEX_HTML.contains(r#"fillDict("f-status", "stan").then(buildStatusChips);"#),
            "chips are built after the status dict populates the select");
        assert!(ui::INDEX_HTML.contains(r#"$("f-status").value = value; doSearch(0);"#),
            "a chip click sets the select and re-searches via doSearch");
        assert!(ui::INDEX_HTML.contains(r#"$("f-status").addEventListener("change", () => syncStatusChips());"#),
            "dropdown edits repaint the chips");
        let d = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("async function doSearch(offset) {")
            .expect("doSearch")..];
        let d = &d[..d.find("\n}").expect("doSearch end")];
        let reset = d.find("resetFacetLabels();").expect("facet reset");
        let sync = d.find("syncStatusChips();").expect("chips painted per search run");
        assert!(reset < sync, "the chips repaint with the search state, like the facet labels");
        let s = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function syncStatusChips() {")
            .expect("syncStatusChips")..];
        let s = &s[..s.find("\n}").expect("syncStatusChips end")];
        assert!(s.contains(r#"const v = $("f-status").value;"#), "painted from the select, not from chip clicks");
        assert!(s.contains(r#"(b.dataset.value || "") === v"#), "«Усі» (empty value) matches the empty select");
        assert!(s.contains(r#"b.classList.toggle("active", on)"#), "selected chip styling class reuse");
    }

    #[test]
    fn ui_page_has_empty_results_suggestions() {
        // A zero-hits search shows the GUI-style rich empty state instead of
        // the bare meta line: ONE pure builder renders the block, its first
        // line keeps the original muted «нічого не знайдено», and the
        // «скинути фільтри» chip (a real <button id="empty-reset">) runs the
        // SAME reset the «скинути все» active-filter chip uses, re-searching
        // through the existing entry point (doSearch(0)).
        assert!(ui::INDEX_HTML.contains("function emptyResultsHtml(hasActiveFilters)"));
        assert!(ui::INDEX_HTML.contains("нічого не знайдено"), "the original zero-hits line is preserved");
        let b = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function emptyResultsHtml(hasActiveFilters) {")
            .expect("emptyResultsHtml")..];
        let b = &b[..b.find("\n}").expect("emptyResultsHtml end")];
        assert!(b.contains("<div class=\"hint\">нічого не знайдено</div>"), "the zero-hits line leads the block");
        assert!(b.contains("id=\"empty-reset\""), "the reset action is a real button element");
        assert!(b.contains("скинути фільтри"));
        // The shared reset: «скинути все» and «скинути фільтри» both bind to
        // resetAllFilters, which zeroes the six filter controls and
        // re-searches from the first page.
        let r = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function resetAllFilters() {")
            .expect("resetAllFilters")..];
        let r = &r[..r.find("\n}").expect("resetAllFilters end")];
        assert!(r.contains(
            "for (const id of [\"f-status\", \"f-typ\", \"f-org\", \"f-number\", \"date-from\", \"date-to\"])"
        ), "the same six-control reset the «скинути все» chip performs");
        assert!(r.contains("doSearch(0);"), "the reset re-searches from page 1");
        assert!(ui::INDEX_HTML.contains("all.onclick = resetAllFilters;"),
            "«скинути все» runs the shared reset");
        let e = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function renderEmptyResults() {")
            .expect("renderEmptyResults")..];
        let e = &e[..e.find("\n}").expect("renderEmptyResults end")];
        assert!(e.contains("rst.onclick = resetAllFilters;"),
            "«скинути фільтри» reuses the existing reset + re-search call");
        // Wired into the zero-hits branch only; hit rows stay untouched.
        let d = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("async function doSearch(offset) {")
            .expect("doSearch")..];
        let d = &d[..d.find("\n}").expect("doSearch end")];
        assert!(d.contains("if (!d.hits.length) {"), "the block renders only when nothing matched");
        assert!(d.contains("renderEmptyResults();"));
    }

    #[test]
    fn ui_page_has_empty_results_filter_clears() {
        // The empty state mirrors the «Активні фільтри» per-chip clears: one
        // .empty-clear <button> per SET filter with data-target naming its
        // control, cleared through the SHARED per-chip path
        // (clearFilterAndSearch zeroes just that control and re-searches via
        // doSearch(0) — the same code path the active-filter chips run);
        // the «шукати в кеші» chip is gated on the cache-search entry point
        // and hands the current term to doCacheSearch.
        let b = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function emptyResultsHtml(hasActiveFilters) {")
            .expect("emptyResultsHtml")..];
        let b = &b[..b.find("\n}").expect("emptyResultsHtml end")];
        assert!(b.contains("if (hasActiveFilters)"), "the reset chip appears only while filters block results");
        assert!(b.contains("activeChips()"), "one clear per filter actually set");
        assert!(b.contains("class=\"chip empty-clear\""), "per-filter clear buttons carry the empty-clear class");
        assert!(b.contains(r#"data-target="' + escHtml(c.id) + '"#),
            "each clear names its filter control via data-target (escaped)");
        assert!(b.contains(r#"typeof doCacheSearch === "function""#),
            "the cache chip is gated on the cache-search entry point");
        assert!(b.contains("id=\"empty-cache\""));
        assert!(b.contains("шукати в кеші"));
        let c = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function clearFilterAndSearch(c) {")
            .expect("clearFilterAndSearch")..];
        let c = &c[..c.find("\n}").expect("clearFilterAndSearch end")];
        assert!(c.contains("$(c.id).value = \"\"; doSearch(0);"),
            "the shared clear zeroes just its own control, then re-searches via doSearch(0)");
        assert!(ui::INDEX_HTML.contains("b.onclick = () => clearFilterAndSearch(c);"),
            "the «Активні фільтри» chips run the same shared clear path");
        let e = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function renderEmptyResults() {")
            .expect("renderEmptyResults")..];
        let e = &e[..e.find("\n}").expect("renderEmptyResults end")];
        assert!(e.contains("querySelectorAll(\".empty-clear\")"));
        assert!(e.contains("clearFilterAndSearch({ id: b.dataset.target })"),
            "an empty-clear chip clears just its filter and re-searches via the existing entry point");
        assert!(e.contains("cch.onclick = () => doCacheSearch();"),
            "the cache chip switches to cache-mode search with the current term");
    }

    #[test]
    fn ui_page_has_reader_doc_stats_line() {
        // The GUI reader paints a compact muted stats one-liner beside the
        // body (reading_stats in rada-gui/src/views/reader.rs); the web
        // reader mirrors it under the doc title header: #doc-head (nazva +
        // status) leads, then #doc-stats as a .hint one-liner (the panel's
        // muted language), then the body area — hidden in the markup until a
        // body paints.
        assert!(ui::INDEX_HTML.contains(
            "<div id=\"doc-stats\" class=\"hint\" hidden></div>"
        ), "muted one-liner, hidden until a body is loaded");
        let pane = ui::INDEX_HTML
            .find("<div id=\"doc-pane\" hidden>")
            .expect("doc pane");
        let head = ui::INDEX_HTML
            .find("<div id=\"doc-head\"></div>")
            .expect("doc title header");
        let stats = ui::INDEX_HTML
            .find("id=\"doc-stats\"")
            .expect("doc stats element");
        let body = ui::INDEX_HTML
            .find("id=\"doc-text\"")
            .expect("doc body area");
        assert!(pane < head && head < stats && stats < body,
            "the stats line sits right under the title header, above the body");
        // openDoc resets it (text AND hidden) so «401 слів …» of a previous
        // act never follows the user into the next document, and unloaded
        // docs show no «0 слів» noise.
        let open = ui::INDEX_HTML
            .find("async function openDoc(nreg) {")
            .expect("openDoc");
        let end = ui::INDEX_HTML
            .find("async function loadCard() {")
            .expect("loadCard");
        let ob = &ui::INDEX_HTML[open..end];
        assert!(ob.contains(r#"$("doc-stats").textContent = "";"#),
            "openDoc clears a stale stats line");
        assert!(ob.contains(r#"$("doc-stats").hidden = true;"#),
            "openDoc hides it again until the next body renders");
    }

    #[test]
    fn ui_page_has_reader_doc_stats_pure() {
        // ONE pure docStats(text) returns the string, mirroring the GUI
        // reader's reading_stats (rada-gui/src/views/reader.rs) definition
        // for definition: words = whitespace-separated tokens (the GUI's
        // text.split_whitespace().count() — no letter/digit filter, Cyrillic
        // words count like Latin ones), chars = the FULL length (the GUI's
        // text.chars().count(), Cyrillic-safe via Array.from), reading time
        // at the GUI's ≈400 words/min with the same div_ceil(400).max(1)
        // floor of 1 хв, thousands grouped with plain spaces, and the exact
        // GUI labels «{words} слів · {chars} симв. · ~{min} хв» — no emoji,
        // no arrows. Its own GUI test pins «401 слів · 2 406 симв. · ~2 хв»
        // for 401 «слово» tokens; the same vectors hold here.
        let d = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function docStats(text) {")
            .expect("docStats")..];
        let d = &d[..d.find("\n}").expect("docStats end")];
        assert!(d.contains("t.split(/\\s+/).filter(Boolean).length"),
            "words mirror the GUI's split_whitespace().count()");
        assert!(d.contains("if (!words) return \"\";"),
            "empty/whitespace-only text returns \"\" — empty-to-hidden contract");
        assert!(d.contains("Array.from(t).length"),
            "chars mirror the GUI's chars().count() (code points, not bytes)");
        assert!(d.contains("Math.max(1, Math.ceil(words/400))"),
            "≈400 words/min with the minimum-1 «~N хв» floor (the GUI's div_ceil(400).max(1))");
        assert!(d.contains("\" слів · \"") && d.contains("\" симв. · ~\"") && d.contains("\" хв\""),
            "the exact GUI reading_stats labels, no emoji or arrows");
        assert!(d.contains("groupTh(words)") && d.contains("groupTh(chars)"),
            "thousands grouped with plain spaces, like the GUI");
        // ONE paint point: renderDocStats feeds on the RAW state.curText via
        // docStats (never innerHTML — the find <mark>s must not pollute the
        // counts) and hides the element on the empty string; runFind, the
        // only body renderer, repaints the stats right after the body.
        let r = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function renderDocStats() {")
            .expect("renderDocStats")..];
        let r = &r[..r.find("\n}").expect("renderDocStats end")];
        assert!(r.contains("docStats(state.curText)"),
            "painted from the raw loaded text, never from the marked-up DOM");
        assert!(!r.contains("innerHTML"), "the counts never read the DOM");
        assert!(r.contains("el.textContent = s;"), "the pure string is painted as-is");
        assert!(r.contains("el.hidden = !s;"), "the empty string hides the line");
        let f = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function runFind() {")
            .expect("runFind")..];
        let f = &f[..f.find("\n}").expect("runFind end")];
        let paint = f.find("pre.innerHTML = goldHtml(state.curText);").expect("body paint");
        let stats = f.find("renderDocStats();").expect("stats paint with the body");
        assert!(paint < stats, "the stats line repaints on every body render");
    }

    #[test]
    fn ui_page_has_article_copy_button() {
        // Per-heading «копія» chip (the GUI's «копія» quick-action wording,
        // favorites.rs/search.rs): appended by buildToc INSIDE each heading
        // block, from the very loop over the toc's own ".p-section,
        // .p-article" selector — so a chip exists exactly where a «Зміст»
        // entry does, and the two can never diverge. It reuses the shared
        // chip class, carries the plain (emoji-free, arrow-free) «копія»
        // label and the lowercase «скопіювати текст статті» hover hint, and
        // a named perf cap skips the buttons on enormous documents.
        assert!(ui::INDEX_HTML.contains("const ARTICLE_COPY_MAX = 400;"),
            "the perf cap is a named const");
        let b = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function buildToc() {")
            .expect("buildToc")..];
        let b = &b[..b.find("\n}").expect("buildToc end")];
        // the SAME selector «Зміст» lists from — chips mirror the outline.
        assert!(b.contains("querySelectorAll(\".p-section, .p-article\")"),
            "chips follow the toc's own article selector");
        assert!(b.contains("heads.length <= ARTICLE_COPY_MAX"),
            "the cap guards the button pass, not the outline");
        assert!(b.contains("cp.className = \"chip p-copy\""),
            "reuses the existing chip styling");
        assert!(b.contains("cp.textContent = \"копія\""));
        assert!(b.contains("cp.title = \"скопіювати текст статті\""));
        assert!(b.contains("h.appendChild(cp);"),
            "the chip lives inside the heading block itself");
    }

    #[test]
    fn ui_page_has_article_copy_excludes_label() {
        // The clipboard payload is captured at BUILD time: the heading text
        // is read into `title`/`artText` before h.appendChild(cp), so the
        // chip's own «копія» label is structurally absent from the copied
        // string; the click reuses the shared copyText with the
        // «скопійовано» flash (stopPropagation first). Find-highlight safety
        // is structural too: runFind's TreeWalker skips every text node
        // whose ancestor is a .p-copy chip, so the label never becomes a
        // find hit, and the counter's total reads the raw state.curText,
        // never the marked-up DOM.
        let b = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function buildToc() {")
            .expect("buildToc")..];
        let b = &b[..b.find("\n}").expect("buildToc end")];
        let read = b.find("const title = h.textContent").expect("heading text read");
        let cap = b.find("const artText = title.trim()").expect("build-time capture");
        let append = b.find("h.appendChild(cp);").expect("chip appended");
        assert!(read < cap, "the capture reads the text already held in title");
        assert!(cap < append,
            "the copied text is captured BEFORE the chip enters the DOM — the label can never reach the clipboard");
        assert!(b.contains("ev.stopPropagation(); copyText(artText, cp, \"скопійовано\")"),
            "shared copyText with the «скопійовано» flash, no row click");
        let r = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function runFind() {")
            .expect("runFind")..];
        let r = &r[..r.find("\n}").expect("runFind end")];
        assert!(r.contains("closest(\".p-copy\")"),
            "the find walker structurally skips the chip labels");
        assert!(r.contains("NodeFilter.SHOW_TEXT"), "the walker is still the find pipeline");
    }

    #[test]
    fn ui_page_has_font_scale_controls() {
        // Reader text-resize toolbar buttons A+/A-/A-0 (plain ASCII labels,
        // no emoji/arrows) sit in the reader toolbar beside the «Простий»
        // toggle, and the scale persists in «rada-font-scale» (the same key
        // family as «rada-toc-collapsed»). The clamped/stepped value is
        // computed by the PURE clampFontScale() so it can be string-tested
        // like fmtSince/countMatches/docStats, and the named bounds mirror
        // the desktop GUI exactly: views/reader.rs zoom_step
        // `(cur as i64 + delta as i64).clamp(60, 250) as u32` and
        // settings.rs text_scale `(self.zoom as f32 / 100.0).clamp(0.6, 2.5)`
        // — i.e. 0.6..=2.5 in 0.1 steps, reset to 1.0.
        assert!(ui::INDEX_HTML.contains("id=\"btn-font-plus\""));
        assert!(ui::INDEX_HTML.contains("id=\"btn-font-minus\""));
        assert!(ui::INDEX_HTML.contains("id=\"btn-font-reset\""));
        assert!(ui::INDEX_HTML.contains(">A+</button>"));
        assert!(ui::INDEX_HTML.contains(">A-</button>"));
        assert!(ui::INDEX_HTML.contains(">A0</button>"));
        assert!(ui::INDEX_HTML.contains("\"rada-font-scale\""));
        assert!(ui::INDEX_HTML.contains("fontScale: 1,"), "state keeps the scale");
        // The GUI clamp values mirrored 1:1 from rada-gui (zoom 60..=250 %).
        assert!(ui::INDEX_HTML.contains("const FONT_SCALE_MIN = 0.6;"));
        assert!(ui::INDEX_HTML.contains("const FONT_SCALE_MAX = 2.5;"));
        assert!(ui::INDEX_HTML.contains("const FONT_SCALE_STEP = 0.1;"));
        assert!(ui::INDEX_HTML.contains("zoom_step"), "the GUI source is cited in the comment");
        assert!(ui::INDEX_HTML.contains("clamp(60, 250)"), "the GUI bound is quoted");
        // The pure helper clamps then rounds to the 0.1 grid.
        let c = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function clampFontScale(v) {")
            .expect("clampFontScale")..];
        let c = &c[..c.find("\n}").expect("clampFontScale end")];
        assert!(c.contains("Math.min(FONT_SCALE_MAX, Math.max(FONT_SCALE_MIN, n))"),
            "clamps with the named GUI-mirrored bounds");
        assert!(c.contains("Math.round(clamped * 10) / 10"), "steps to 0.1");
        assert!(c.contains("if (!isFinite(n)) return 1;"), "non-finite input resets to 1.0");
        // Every change routes through setFontScale → clampFontScale + persist + repaint.
        let s = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function setFontScale(v) {")
            .expect("setFontScale")..];
        let s = &s[..s.find("\n}").expect("setFontScale end")];
        assert!(s.contains("state.fontScale = clampFontScale(v);"));
        assert!(s.contains("localStorage.setItem(FONT_SCALE_KEY, String(state.fontScale));"));
        assert!(s.contains("applyFontScale();"));
        // Toolbar buttons step ±0.1 around state.fontScale; A0 resets to 1.0.
        assert!(ui::INDEX_HTML.contains(
            "$(\"btn-font-plus\").onclick = () => setFontScale(state.fontScale + FONT_SCALE_STEP);"
        ));
        assert!(ui::INDEX_HTML.contains(
            "$(\"btn-font-minus\").onclick = () => setFontScale(state.fontScale - FONT_SCALE_STEP);"
        ));
        assert!(ui::INDEX_HTML.contains(
            "$(\"btn-font-reset\").onclick = () => setFontScale(1);"
        ));
        // Restored on load from «rada-font-scale».
        assert!(ui::INDEX_HTML.contains(
            "parseFloat(localStorage.getItem(FONT_SCALE_KEY))"
        ));
    }

    #[test]
    fn ui_page_has_font_scale_keyboard_shortcuts() {
        // Ctrl+= / Ctrl+Plus grows, Ctrl+- shrinks, Ctrl+0 resets — the web
        // twin of the GUI's Ctrl+Plus/Minus/0 reader zoom (app.rs
        // zoom_reader). The branches live in the shared keydown listener
        // beside the Ctrl+F find focus (the file's existing idiom), each
        // guarded by the Ctrl/Cmd-mod + no-Alt + reader-open conditions,
        // each preventDefaults so the browser never zooms the page, and all
        // three route through setFontScale → clampFontScale (the pure
        // 0.6..=2.5 GUI-mirrored clamp).
        let h = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("// Ctrl+F / Cmd+F opens the document find bar instead of the browser find.")
            .expect("reader keydown listener")..];
        let h = &h[..h.find("\n});").expect("keydown listener end")];
        // Distinct key literals for each of the three commands.
        assert!(h.contains("ev.key === \"=\" || ev.key === \"+\" || ev.key === \"NumpadAdd\""),
            "Ctrl+= / Ctrl+Plus grow branch");
        assert!(h.contains("ev.key === \"-\" || ev.key === \"NumpadSubtract\""),
            "Ctrl+- shrink branch");
        assert!(h.contains("ev.key === \"0\" || ev.key === \"Numpad0\""),
            "Ctrl+0 reset branch");
        // Each handled branch cancels the browser's own page zoom.
        assert_eq!(h.matches("ev.preventDefault();\n    setFontScale").count(), 3,
            "every zoom branch preventDefaults before stepping");
        // Routed through the clamped setter with the distinct literals.
        assert!(h.contains("setFontScale(state.fontScale + FONT_SCALE_STEP);"));
        assert!(h.contains("setFontScale(state.fontScale - FONT_SCALE_STEP);"));
        assert!(h.contains("setFontScale(1);"), "Ctrl+0 resets to 1.0");
        assert!(h.contains("!$(\"doc-pane\").hidden"), "reader-scoped, like Ctrl+F");
        assert!(h.contains("clampFontScale"), "the comment cites the pure clamp route");
    }

    /// Index-stats footer line: the pure `statusFooterHtml(d)` builder is
    /// present, carries the exact Ukrainian segments («тіл у кеші», the
    /// «оновлено: » freshness label with its «невідомо» fallback) and is the
    /// single paint point of `refreshFoot` (which keeps feeding it through
    /// the file's existing `/api/status` fetch idiom).
    #[test]
    fn ui_page_has_status_footer_line() {
        assert!(
            ui::INDEX_HTML.contains("function statusFooterHtml(d) {"),
            "pure footer builder present"
        );
        assert!(ui::INDEX_HTML.contains("тіл у кеші"), "cache bodies label");
        assert!(ui::INDEX_HTML.contains("оновлено: "), "freshness label");
        assert!(ui::INDEX_HTML.contains("невідомо"), "unknown-date fallback");
        assert!(
            ui::INDEX_HTML.contains("fmtImportDate(d.last_import)"),
            "the builder formats the status date through fmtImportDate"
        );
        let start = ui::INDEX_HTML
            .find("async function refreshFoot() {")
            .expect("refreshFoot present");
        let body = &ui::INDEX_HTML[start..];
        let body = &body[..body.find("\n}").expect("refreshFoot end")];
        assert!(
            body.contains("statusFooterHtml(s)"),
            "the footer paints through the builder"
        );
        assert!(
            body.contains("\"/api/status\""),
            "the builder is fed by the existing status fetch"
        );
    }

    /// `fmtImportDate` semantics asserted on the source literal: the strict
    /// ISO guard, the exact dd.mm.yyyy reordering (the doc comment pins
    /// 2026-10-25 becoming 25.10.2026) and the garbage/empty/null null path;
    /// plus the three footer fields present as serialized string literals
    /// in the `/api/status` json! object of this very source file.
    #[test]
    fn fmt_import_date_semantics_and_status_fields() {
        let start = ui::INDEX_HTML
            .find("function fmtImportDate(iso) {")
            .expect("fmtImportDate defined");
        let comment = &ui::INDEX_HTML[start.saturating_sub(400)..start];
        assert!(comment.contains("2026-10-25"), "doc comment pins the sample date");
        assert!(comment.contains("25.10.2026"), "doc comment pins the expected day-first form");
        let body = &ui::INDEX_HTML[start..];
        let body = &body[..body.find("\n}").expect("fmtImportDate end")];
        assert!(
            body.contains("/^\\d{4}-\\d{2}-\\d{2}$/.test(s)"),
            "strict YYYY-MM-DD guard (garbage answers null)"
        );
        assert!(
            body.contains("s.slice(8, 10) + \".\" + s.slice(5, 7) + \".\" + s.slice(0, 4)"),
            "2026-10-25 reorders to 25.10.2026"
        );
        assert!(body.contains("return null"), "empty/null path returns null");
        // /api/status serializes the footer fields (this file's own source).
        let src = include_str!("lib.rs");
        for field in ["\"docs\"", "\"cache_rows\"", "\"last_import\""] {
            assert!(src.contains(field), "/api/status must serialize {field}");
        }
    }

    /// Batch offline-cache probe (`GET /api/cache/cached?nregs=a,b,c`):
    /// method/param validation, the docs_text filter (only nregs with a
    /// cached body answer, seeded directly — no network), and the pure
    /// `parse_nregs_param` parser (trim, empties, duplicates, 200 cap).
    #[test]
    fn cache_cached_endpoint_batch_probe() {
        let st = favorites_state("cbatch");
        {
            let conn = db_open(&st.db_path).unwrap();
            rada_db::textcache::put(&conn, "2341-14", "", "кешоване тіло").unwrap();
        }
        // Query-aware call (the plain `call` helper sends no params).
        let get = |path: &str, query: &str| -> (u16, Value) {
            let mut req: Request = TestRequest::new()
                .with_method(Method::Get)
                .with_path(path)
                .into();
            let resp = route(&Method::Get, path, &parse_query(query), &mut req, &st).unwrap();
            let code = resp.status_code().0;
            let bytes = resp.into_reader().into_inner();
            (code, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
        };
        let (code, _) = call(&st, Method::Post, "/api/cache/cached", "");
        assert_eq!(code, 405, "write methods rejected");
        let (code, v) = get("/api/cache/cached", "");
        assert_eq!(code, 200, "{v}");
        assert_eq!(
            v["cached"].as_array().unwrap().len(),
            0,
            "missing param is an empty list: {v}"
        );
        let (code, v) = get("/api/cache/cached", "nregs=");
        assert_eq!(code, 200, "{v}");
        assert_eq!(
            v["cached"].as_array().unwrap().len(),
            0,
            "empty param is an empty list: {v}"
        );
        // Mixed page: only the seeded docs_text row answers; the blank part
        // and the duplicate drop out, the unknown nreg stays out.
        let (code, v) = get("/api/cache/cached", "nregs=9-9,2341-14,%20,2341-14");
        assert_eq!(code, 200, "{v}");
        let hits = v["cached"].as_array().unwrap();
        assert_eq!(hits.len(), 1, "only the cached body: {hits:?}");
        assert_eq!(hits[0], "2341-14");
        // The pure parser: missing/blank → empty, trim, drop empties,
        // de-duplicate keeping order, extras past 200 ignored.
        assert_eq!(parse_nregs_param(None), Vec::<String>::new());
        assert_eq!(parse_nregs_param(Some("  ")), Vec::<String>::new());
        assert_eq!(
            parse_nregs_param(Some(" a , ,a,b ")),
            vec!["a".to_string(), "b".to_string()]
        );
        let many = (0..250).map(|i| format!("n{i}")).collect::<Vec<_>>().join(",");
        assert_eq!(parse_nregs_param(Some(&many)).len(), 200, "extras past 200 ignored");
    }

    /// « · кеш» badges on the search result rows (the desktop cache_badge
    /// twin): the pure `cacheBadgeHtml(cachedSet, nreg)` builder carries the
    /// exact wording and hover title, the batch painter asks
    /// `/api/cache/cached?nregs=` through it inside a try/catch (a failed
    /// probe leaves the results usable), and the results renderer stamps
    /// rows with `data-nreg` and calls the painter after the rows rendered.
    #[test]
    fn ui_page_has_cache_badges() {
        let start = ui::INDEX_HTML
            .find("function cacheBadgeHtml(cachedSet, nreg) {")
            .expect("pure badge builder present");
        let body = &ui::INDEX_HTML[start..];
        let body = &body[..body.find("\n}").expect("cacheBadgeHtml end")];
        assert!(body.contains(" · кеш</span>"), "badge wording mirrors the GUI");
        assert!(
            body.contains("текст уже в офлайн-кеші — відкриється без мережі"),
            "hover title explains the offline copy"
        );
        assert!(body.contains("return \"\""), "uncached nreg paints nothing");
        // The batch painter: asks the endpoint, paints through the builder,
        // and swallows failures so the results stay fully usable.
        let start = ui::INDEX_HTML
            .find("async function paintCacheBadges(nregs) {")
            .expect("badge painter present");
        let body = &ui::INDEX_HTML[start..];
        let body = &body[..body.find("\n}").expect("paintCacheBadges end")];
        assert!(body.contains("/api/cache/cached?nregs="), "batch probe");
        assert!(
            body.contains("cacheBadgeHtml(set, nreg)"),
            "badges painted through the pure builder"
        );
        assert!(body.contains("catch"), "a failed probe never disturbs the results");
        assert!(body.contains(".hit[data-nreg]"), "rows matched by the stamped nreg");
        // The results renderer: rows stamped, painter called after they render.
        let start = ui::INDEX_HTML
            .find("async function doSearch(offset) {")
            .expect("doSearch defined");
        let body = &ui::INDEX_HTML[start..];
        let body = &body[..body.find("\n}").expect("doSearch end")];
        assert!(body.contains("div.dataset.nreg = h.nreg;"), "rows carry their nreg");
        let nav = body
            .find("fillNavList(d.hits.map((h) => h.nreg));")
            .expect("nav fill after the rows");
        let paint = body
            .find("paintCacheBadges(d.hits.map((h) => h.nreg));")
            .expect("renderer calls the painter");
        assert!(nav < paint, "badges paint after the rows rendered");
    }

    // ---- «Свіжі документи (за датою)» home card (`/api/fresh`) ---------------

    /// Fresh indexed db for the `/api/fresh` tests: three cards with full
    /// 8-digit adoption dates (two sharing the newest one, so the nreg tie
    /// rule shows) plus three mis-dated rows (short, empty, dashed) that
    /// must stay out. Unique dir per tag keeps parallel tests from colliding.
    fn fresh_state(tag: &str) -> Arc<State> {
        let dir = std::env::temp_dir().join(format!("rada-web-fresh-test-{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create test dir");
        let st = Arc::new(State {
            db_path: dir.join("test.sqlite"),
            client: RadaClient::with_pacing(Duration::from_secs(3600)),
            pace_secs: 3600,
            jobs: Mutex::new(HashMap::new()),
            job_seq: AtomicU64::new(0),
        });
        let conn = db_open(&st.db_path).expect("open test index");
        conn.execute_batch(
            "INSERT INTO docs(nreg, nazva, org_date) VALUES
             ('2000-02', 'Буквар',  '20251231'),
             ('3000-03', 'Вісти',   '20251231'),
             ('1000-01', 'Абетка',  '20010101'),
             ('4000-04', 'Гасло',   '2025'),
             ('5000-05', 'Депеша',  ''),
             ('6000-06', 'Ескіз',   '2026-01-02');",
        )
        .expect("seed docs");
        drop(conn);
        st
    }

    /// `GET /api/fresh?{query}` through `route` (query parsed exactly as
    /// `handle_request` does), returning the JSON body.
    fn fresh_get(st: &Arc<State>, query: &str) -> Value {
        let mut req: Request = TestRequest::new()
            .with_method(Method::Get)
            .with_path("/api/fresh")
            .into();
        let resp = route(&Method::Get, "/api/fresh", &parse_query(query), &mut req, st)
            .expect("route");
        assert_eq!(resp.status_code().0, 200);
        serde_json::from_slice(&resp.into_reader().into_inner()).unwrap()
    }

    #[test]
    fn fresh_endpoint_orders_limits_and_filters() {
        let st = fresh_state("main");
        // Newest adoption date first, same-day rows tie on the nreg (the
        // desktop home card's exact ordering), titles ride along.
        let v = fresh_get(&st, "");
        let docs = v["docs"].as_array().unwrap();
        let nregs: Vec<&str> = docs.iter().map(|d| d["nreg"].as_str().unwrap()).collect();
        assert_eq!(nregs, ["2000-02", "3000-03", "1000-01"], "{v}");
        let dates: Vec<&str> = docs.iter().map(|d| d["date"].as_str().unwrap()).collect();
        assert_eq!(dates, ["20251231", "20251231", "20010101"]);
        let titles: Vec<&str> = docs.iter().map(|d| d["title"].as_str().unwrap()).collect();
        assert_eq!(titles, ["Буквар", "Вісти", "Абетка"]);
        // Mis-dated rows never surface: short, empty and dashed org_date
        // values all sit outside the 8-digit GLOB anchor.
        let raw = v.to_string();
        assert!(!raw.contains("4000-04"), "short date excluded: {raw}");
        assert!(!raw.contains("5000-05"), "empty date excluded: {raw}");
        assert!(!raw.contains("6000-06"), "dashed date excluded: {raw}");
        // The limit is respected (and the order kept on the truncated page).
        let v2 = fresh_get(&st, "limit=2");
        let docs2 = v2["docs"].as_array().unwrap();
        assert_eq!(docs2.len(), 2);
        assert_eq!(docs2[0]["nreg"], "2000-02");
        assert_eq!(docs2[1]["nreg"], "3000-03");
        // Only GET is routed.
        assert_eq!(call(&st, Method::Post, "/api/fresh", "").0, 405);
    }

    #[test]
    fn fresh_home_card_page_integration() {
        // The landing view paints the desktop home card's web twin.
        assert!(
            ui::INDEX_HTML.contains("Свіжі документи (за датою):"),
            "heading mirrors the desktop home card"
        );
        assert!(
            ui::INDEX_HTML.contains("id=\"fresh-home\""),
            "the landing block exists in the search tab"
        );
        // Pure 8-digit date helper, same guard style as fmtImportDate:
        // exactly 8 ASCII digits reorder, everything else stays the raw
        // string.
        let start = ui::INDEX_HTML
            .find("function fmtYmd8(d) {")
            .expect("pure fmt helper present");
        let body = &ui::INDEX_HTML[start..];
        let body = &body[..body.find("\n}").expect("fmtYmd8 end")];
        assert!(body.contains("/^[0-9]{8}$/"), "8 ASCII digit guard");
        assert!(body.contains("slice(6, 8)"), "dd.mm.yyyy reordering");
        assert!(body.contains("return s;"), "garbage passes through raw");
        // The painter: asks the endpoint, renders through the pure builder,
        // binds every row to the reader-open idiom, skips failures silently.
        let start = ui::INDEX_HTML
            .find("async function renderFreshHome() {")
            .expect("home painter present");
        let body = &ui::INDEX_HTML[start..];
        let body = &body[..body.find("\n}").expect("renderFreshHome end")];
        assert!(body.contains("/api/fresh?limit=14"), "fetches the fresh endpoint");
        assert!(body.contains("freshHomeHtml(docs)"), "rows built by the pure builder");
        assert!(
            body.contains("row.onclick = () => openDoc(doc.nreg);"),
            "rows open through the reader-open idiom"
        );
        assert!(body.contains("li[data-i]"), "rows matched by index, not raw nregs");
        assert!(body.contains("catch"), "a failed fetch skips silently");
        // The startup wiring: init paints the landing block.
        let start = ui::INDEX_HTML
            .find("async function init() {")
            .expect("init present");
        let body = &ui::INDEX_HTML[start..];
        let body = &body[..body.find("\n}").expect("init end")];
        assert!(body.contains("renderFreshHome();"), "init paints the card");
    }

    // ---- «Документи за роками» landing histogram (`/api/years`) -------------

    /// Fresh indexed db for the `/api/years` tests: dated rows across three
    /// years (2024 the densest, 2025 deliberately absent so the zero-count
    /// omission shows), plus mis-dated rows (short, empty, dashed and a
    /// non-digit 8-char value) that must stay out of the histogram. Unique
    /// dir per tag keeps parallel tests from colliding.
    fn years_state(tag: &str) -> Arc<State> {
        let dir = std::env::temp_dir().join(format!("rada-web-years-test-{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create test dir");
        let st = Arc::new(State {
            db_path: dir.join("test.sqlite"),
            client: RadaClient::with_pacing(Duration::from_secs(3600)),
            pace_secs: 3600,
            jobs: Mutex::new(HashMap::new()),
            job_seq: AtomicU64::new(0),
        });
        let conn = db_open(&st.db_path).expect("open test index");
        conn.execute_batch(
            "INSERT INTO docs(nreg, nazva, org_date) VALUES
             ('7100-01', 'Травень',  '20240501'),
             ('7101-02', 'Січень',   '20240115'),
             ('7102-03', 'Березень', '20240310'),
             ('7103-04', 'Грудень',  '20261231'),
             ('7104-05', 'Липень',   '20260704'),
             ('7105-06', 'Квітень',  '20010401'),
             ('7106-07', 'Шаблон',   '2025'),
             ('7107-08', 'Порожньо', ''),
             ('7108-09', 'Різник',   '2026-01-02'),
             ('7109-10', 'Нецифра',  '20AB1231');",
        )
        .expect("seed docs");
        drop(conn);
        st
    }

    #[test]
    fn years_endpoint_orders_filters_and_answers_json() {
        let st = years_state("main");
        let mut req: Request = TestRequest::new()
            .with_method(Method::Get)
            .with_path("/api/years")
            .into();
        let resp = route(&Method::Get, "/api/years", &[], &mut req, &st).expect("route");
        assert_eq!(resp.status_code().0, 200);
        let ct = resp
            .headers()
            .iter()
            .find(|x| x.field.as_str().as_str().eq_ignore_ascii_case("content-type"))
            .map(|x| x.value.to_string())
            .unwrap_or_default();
        assert!(ct.contains("application/json"), "content-type: {ct}");
        let v: Value = serde_json::from_slice(&resp.into_reader().into_inner()).unwrap();
        let rows = v["rows"].as_array().unwrap();
        // Ascending order over ALL full-8-digit-dated docs, counts exact.
        let years: Vec<&str> = rows.iter().map(|r| r["year"].as_str().unwrap()).collect();
        assert_eq!(years, ["2001", "2024", "2026"], "{v}");
        let counts: Vec<i64> = rows.iter().map(|r| r["count"].as_i64().unwrap()).collect();
        assert_eq!(counts, [1, 3, 2], "{v}");
        // A year with no rows never appears (GROUP BY omits it: zero-count
        // years stay out, the CLI `years` rule).
        let raw = v.to_string();
        assert!(!raw.contains("\"2025\""), "zero-count year omitted: {raw}");
        // Mis-dated rows never surface: short, empty, dashed and non-digit
        // org_date values all sit outside the 8-digit GLOB anchor.
        assert!(!raw.contains("7106-07"), "short date excluded: {raw}");
        assert!(!raw.contains("7107-08"), "empty date excluded: {raw}");
        assert!(!raw.contains("7108-09"), "dashed date excluded: {raw}");
        assert!(!raw.contains("7109-10"), "non-digit date excluded: {raw}");
        // Only GET is routed.
        assert_eq!(call(&st, Method::Post, "/api/years", "").0, 405);
    }

    #[test]
    fn years_home_page_integration() {
        // The landing view paints the CLI `years` report's web twin right
        // under the fresh-docs card.
        assert!(
            ui::INDEX_HTML.contains("Документи за роками:"),
            "heading names the histogram"
        );
        assert!(
            ui::INDEX_HTML.contains("id=\"years-home\""),
            "the landing block exists in the search tab"
        );
        // The pure builder: empty in → empty out, the documented last-15
        // cap, and bars painted as proportional percent widths.
        let start = ui::INDEX_HTML
            .find("function yearsHomeHtml(rows) {")
            .expect("pure builder present");
        let body = &ui::INDEX_HTML[start..];
        let body = &body[..body.find("\n}").expect("yearsHomeHtml end")];
        assert!(body.contains("return \"\""), "empty rows paint nothing");
        assert!(body.contains("slice(-15)"), "only the last 15 years show");
        assert!(body.contains("width:"), "bar width scales with count/max");
        assert!(body.contains("Документи за роками:"), "the builder owns the heading");
        // The painter: asks the endpoint, renders through the pure builder,
        // and swallows failures so the landing stays untouched.
        let start = ui::INDEX_HTML
            .find("async function renderYearsHome() {")
            .expect("home painter present");
        let body = &ui::INDEX_HTML[start..];
        let body = &body[..body.find("\n}").expect("renderYearsHome end")];
        assert!(body.contains("/api/years"), "fetches the years endpoint");
        assert!(
            body.contains("yearsHomeHtml(rows)"),
            "rows built by the pure builder"
        );
        assert!(body.contains("catch"), "a failed fetch skips silently");
        // The startup wiring: init paints the histogram alongside the card.
        let start = ui::INDEX_HTML
            .find("async function init() {")
            .expect("init present");
        let body = &ui::INDEX_HTML[start..];
        let body = &body[..body.find("\n}").expect("init end")];
        assert!(body.contains("renderYearsHome();"), "init paints the histogram");
        // Same lifetime as the fresh card: a search the user has already
        // run retires the block for good.
        let start = ui::INDEX_HTML
            .find("async function doSearch(offset) {")
            .expect("doSearch defined");
        let body = &ui::INDEX_HTML[start..];
        let body = &body[..body.find("\n}").expect("doSearch end")];
        assert!(body.contains("yearsHomeDismissed = true;"), "later searches retire it");
        assert!(
            body.contains("$(\"years-home\").hidden = true;"),
            "the block hides on the first real search"
        );
    }

    #[test]
    fn ui_page_has_favorites_csv_builder() {
        // ONE pure favoritesCsv(rows) builder mirroring the arrivals
        // arrivalsCsv format family: UTF-8 BOM up front (Excel opens
        // Cyrillic correctly), the «nreg,title,kind,created_at,text» header
        // (what a favorites row carries on screen: document number + title,
        // annotation kind, the created_at save stamp and the text), comma
        // separator, and rows appended in the given order — the file always
        // mirrors the on-screen order.
        assert!(ui::INDEX_HTML.contains("function favoritesCsv(rows)"));
        let b = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function favoritesCsv(rows) {")
            .expect("favoritesCsv")..];
        let b = &b[..b.find("\n}").expect("favoritesCsv end")];
        // BOM + header row, the same columns in the same row order.
        assert!(
            b.contains(r#""\uFEFFnreg,title,kind,created_at,text\n""#),
            "BOM + the favorites header row"
        );
        // Every field ships through the arrivals escaper (shared quoting)
        // separated by commas: titles with quotes, commas or newlines are
        // wrapped and their quotes doubled (GUI csv_esc parity).
        assert!(
            b.contains("arrCsvEsc(r.nreg) + \",\" + arrCsvEsc(r.title)"),
            "nreg + title columns"
        );
        assert!(
            b.contains("arrCsvEsc(r.kind) + \",\" + arrCsvEsc(r.created_at)"),
            "kind + created_at columns"
        );
        assert!(b.contains("arrCsvEsc(r.text)"), "text column");
        // Row order: the builder iterates the given rows verbatim — no
        // sort, no filter in here.
        assert!(b.contains("for (const r of rows)"), "rows appended in the given order");
        assert!(!b.contains(".sort("), "the builder never sorts");
        let e = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function arrCsvEsc(s) {")
            .expect("arrCsvEsc")..];
        let e = &e[..e.find("\n}").expect("arrCsvEsc end")];
        assert!(e.contains(r#"/["\n\r,]/.test(v)"#), "quote, comma and line breaks trigger quoting");
        assert!(e.contains(r#".replace(/"/g, "\"\"")"#), "quotes are doubled inside the field");
        // The download path hands the builder's BOM'd text to the arrivals
        // Blob / object-URL idiom under the fixed favorites.csv name (the
        // novy-dokumenty.csv pattern).
        let x = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function exportFavCsv() {")
            .expect("exportFavCsv")..];
        let x = &x[..x.find("\n}").expect("exportFavCsv end")];
        assert!(x.contains("favoritesCsv(visibleFavorites());"), "the builder feeds the download");
        assert!(
            x.contains("new Blob([csv], { type: \"text/csv;charset=utf-8\" });"),
            "the BOM'd text ships as the Blob content"
        );
        assert!(x.contains("URL.createObjectURL(blob)"));
        assert!(x.contains("a.download = \"favorites.csv\";"), "fixed file name in the arrivals idiom");
        assert!(x.contains("a.click();"));
        assert!(x.contains("a.remove();"));
        assert!(x.contains("URL.revokeObjectURL(url);"));
    }

    #[test]
    fn ui_page_has_favorites_csv_button() {
        // «експорт .csv» chip on the favorites controls row right after
        // «експорт .txt» (the arrivals chip CSS language, plain label, no
        // emoji): the click runs the shared exportFavCsv entry point over
        // the VISIBLE list, and the chip stays disabled while nothing is
        // on screen.
        assert!(
            ui::INDEX_HTML.contains("<button id=\"fav-export-csv\" type=\"button\" disabled"),
            "the button starts disabled (the page loads empty)"
        );
        assert!(ui::INDEX_HTML.contains(">експорт .csv</button>"));
        // Same toolbar row as the favorites header controls: everything
        // between #fav-refresh and the kind-chip row.
        let row = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("id=\"fav-refresh\"")
            .expect("fav-refresh")..ui::INDEX_HTML
            .find("<div class=\"row\" id=\"fav-filters\">")
            .expect("fav filters row")];
        for id in ["fav-export", "btn-fav-export-txt", "fav-export-csv", "fav-import"] {
            assert!(row.contains(&format!("id=\"{id}\"")), "{id}: lives on the favorites controls row");
        }
        let txt = row.find("id=\"btn-fav-export-txt\"").expect("txt chip");
        let csv = row.find("id=\"fav-export-csv\"").expect("csv chip");
        assert!(txt < csv, "the csv chip trails «експорт .txt»");
        // Chip CSS reuses the arrivals export chip language, hover included.
        assert!(ui::INDEX_HTML.contains("#fav-export-csv {"), "chip CSS reuses the arrivals chip language");
        assert!(ui::INDEX_HTML.contains("#fav-export-csv:hover {"), "accent hover like the arrivals chip");
        assert!(
            ui::INDEX_HTML.contains("$(\"fav-export-csv\").onclick = () => exportFavCsv();"),
            "click goes through the shared export entry point"
        );
        // The VISIBLE list, not the raw one: the exporter runs the exact
        // «показано X з Y» pipeline — favSorted + the active kind chip +
        // favRowMatches over the #fav-filter box — and never touches the
        // raw favItems array itself.
        assert!(ui::INDEX_HTML.contains("function visibleFavorites()"));
        let v = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function visibleFavorites() {")
            .expect("visibleFavorites")..];
        let v = &v[..v.find("\n}").expect("visibleFavorites end")];
        assert!(v.contains("favSorted(favItems, favSort)"), "the shared sort");
        assert!(v.contains(".filter(a => !favKind || a.kind === favKind)"), "the active kind chip");
        assert!(v.contains(".filter(a => favRowMatches(a, needle))"), "the shared text filter");
        assert!(v.contains("$(\"fav-filter\").value"), "reads the #fav-filter box");
        let x = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function exportFavCsv() {")
            .expect("exportFavCsv")..];
        let x = &x[..x.find("\n}").expect("exportFavCsv end")];
        assert!(x.contains("favoritesCsv(visibleFavorites());"), "exports the filtered list");
        assert!(!x.contains("favItems"), "the exporter never reads the raw list");
        // Empty-case gate: renderFavorites re-gates the chip on the very
        // array the «показано X з Y» count uses (nothing visible ⇒ the
        // chip dims and cannot fire a download).
        let r = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("function renderFavorites() {")
            .expect("renderFavorites")..];
        let r = &r[..r.find("\n}").expect("renderFavorites end")];
        let gate = r
            .find("$(\"fav-export-csv\").disabled = !items.length;")
            .expect("renderFavorites gates on the visible list");
        let items = r.find("const items = sorted").expect("visible items");
        assert!(items < gate, "the gate reads the filtered items");
    }

    // ---- «Документи за роками» per-year list (`/api/yeardocs`) -----------------

    #[test]
    fn yeardocs_endpoint_orders_filters_and_validates() {
        // Seeded temp index in the fresh_state/years_state idiom: four 2024
        // rows (two sharing a date, so the nreg tie rule shows), one other
        // year and three mis-dated rows (short, non-digit, empty) that must
        // stay out. Unique dir keeps parallel tests from colliding.
        let dir = std::env::temp_dir().join("rada-web-yeardocs-test-main");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create test dir");
        let st = Arc::new(State {
            db_path: dir.join("test.sqlite"),
            client: RadaClient::with_pacing(Duration::from_secs(3600)),
            pace_secs: 3600,
            jobs: Mutex::new(HashMap::new()),
            job_seq: AtomicU64::new(0),
        });
        let conn = db_open(&st.db_path).expect("open test index");
        conn.execute_batch(
            "INSERT INTO docs(nreg, nazva, org_date) VALUES
             ('9101-02', 'Грудень',      '20241231'),
             ('9102-03', 'Травень',      '20240501'),
             ('9107-08', 'Травень друг', '20240501'),
             ('9100-01', 'Січень',       '20240115'),
             ('9103-04', 'Інший рік',    '20231231'),
             ('9104-05', 'Короткий',     '2024'),
             ('9105-06', 'Нецифра',      '2O240501'),
             ('9106-07', 'Порожньо',     '');",
        )
        .expect("seed docs");
        drop(conn);
        // `GET /api/yeardocs?{query}` through `route` (query parsed exactly
        // as `handle_request` does), answering status + JSON body.
        let yeardocs_get = |query: &str| -> (u16, Value) {
            let mut req: Request = TestRequest::new()
                .with_method(Method::Get)
                .with_path("/api/yeardocs")
                .into();
            let resp = route(&Method::Get, "/api/yeardocs", &parse_query(query), &mut req, &st)
                .expect("route");
            let code = resp.status_code().0;
            let v = serde_json::from_slice(&resp.into_reader().into_inner())
                .unwrap_or(Value::Null);
            (code, v)
        };
        // Newest adoption date first, same-day rows tie on the nreg, the
        // requested year echoes back, titles ride along.
        let (code, v) = yeardocs_get("year=2024");
        assert_eq!(code, 200);
        assert_eq!(v["year"], "2024");
        let docs = v["docs"].as_array().unwrap();
        let nregs: Vec<&str> = docs.iter().map(|d| d["nreg"].as_str().unwrap()).collect();
        assert_eq!(nregs, ["9101-02", "9102-03", "9107-08", "9100-01"], "{v}");
        let dates: Vec<&str> = docs.iter().map(|d| d["date"].as_str().unwrap()).collect();
        assert_eq!(dates, ["20241231", "20240501", "20240501", "20240115"]);
        let titles: Vec<&str> = docs.iter().map(|d| d["title"].as_str().unwrap()).collect();
        assert_eq!(titles, ["Грудень", "Травень", "Травень друг", "Січень"]);
        // Other years and mis-dated rows never surface (the 8-digit GLOB
        // anchor with the year prefix keeps them all out).
        let raw = v.to_string();
        assert!(!raw.contains("9103-04"), "other year excluded: {raw}");
        assert!(!raw.contains("9104-05"), "short date excluded: {raw}");
        assert!(!raw.contains("9105-06"), "non-digit date excluded: {raw}");
        assert!(!raw.contains("9106-07"), "empty date excluded: {raw}");
        // The limit is respected (and the order kept on the truncated page);
        // the clamp floors a zero at one row, the arrivals/fresh idiom.
        let (code, v2) = yeardocs_get("year=2024&limit=2");
        assert_eq!(code, 200);
        let docs2 = v2["docs"].as_array().unwrap();
        assert_eq!(docs2.len(), 2);
        assert_eq!(docs2[0]["nreg"], "9101-02");
        assert_eq!(docs2[1]["nreg"], "9102-03");
        let (code, v3) = yeardocs_get("year=2024&limit=0");
        assert_eq!(code, 200);
        assert_eq!(v3["docs"].as_array().unwrap().len(), 1, "limit floors at 1");
        // A year is exactly 4 ASCII digits: short, non-digit and missing
        // values are all 400s in the file's json_err error family.
        assert_eq!(yeardocs_get("year=26").0, 400);
        assert_eq!(yeardocs_get("year=2O24").0, 400);
        assert_eq!(yeardocs_get("year=").0, 400);
        assert_eq!(yeardocs_get("").0, 400);
        // Only GET is routed.
        assert_eq!(call(&st, Method::Post, "/api/yeardocs", "").0, 405);
    }

    #[test]
    fn yeardocs_page_integration() {
        // Histogram bar rows are clickable rows now: the #fresh-list row
        // affordance (cursor + hover) on every #years-list row.
        let css = &ui::INDEX_HTML[ui::INDEX_HTML
            .find("#years-list li {")
            .expect("year row style present")..];
        let css = &css[..css.find("}").expect("year row style end")];
        assert!(css.contains("cursor: pointer"), "rows invite a click");
        assert!(
            ui::INDEX_HTML.contains("#years-list li:hover"),
            "rows answer with the shared hover tint"
        );
        // The pure builder: empty in → empty out, the heading names the
        // year off the newest row's date, and rows carry the #fresh-list
        // row shape (data-i, dd.mm.yyyy date, nreg, title).
        let start = ui::INDEX_HTML
            .find("function yearDocsHtml(docs) {")
            .expect("pure builder present");
        let body = &ui::INDEX_HTML[start..];
        let body = &body[..body.find("\n}").expect("yearDocsHtml end")];
        assert!(body.contains("return \"\""), "empty docs paint nothing");
        assert!(body.contains("Документи "), "the builder owns the heading");
        assert!(body.contains(" року:"), "the heading names the year");
        assert!(body.contains("data-i=\""), "rows matched by index, not raw nregs");
        assert!(body.contains("fmtYmd8(d.date)"), "dates reuse the 8-digit fmt");
        // The painter: binds every bar row to the per-year list, fetches
        // the endpoint through the loading line, paints via the pure
        // builder, rebinds rows to the reader-open idiom, toggles the same
        // year closed and fences stale answers with the generation counter.
        let start = ui::INDEX_HTML
            .find("async function renderYearsHome() {")
            .expect("histogram painter present");
        let home = &ui::INDEX_HTML[start..];
        let home = &home[..home.find("\n}").expect("renderYearsHome end")];
        assert!(home.contains("renderYearDocs(r.year)"), "bar rows open the year list");
        assert!(home.contains("rows.slice(-15)"), "binding matches the shown window");
        let start = ui::INDEX_HTML
            .find("async function renderYearDocs(year) {")
            .expect("year painter present");
        let body = &ui::INDEX_HTML[start..];
        let body = &body[..body.find("\n}").expect("renderYearDocs end")];
        assert!(body.contains("завантаження…"), "a loading line shows while fetching");
        assert!(
            body.contains("/api/yeardocs?year="),
            "fetches the per-year endpoint"
        );
        assert!(body.contains("yearDocsHtml(docs)"), "rows built by the pure builder");
        assert!(
            body.contains("row.onclick = () => openDoc(doc.nreg);"),
            "rows open through the reader-open idiom"
        );
        assert!(body.contains("yearsDocsYear === y"), "same-year click toggles");
        assert!(body.contains("yearsDocsSeq"), "a generation counter fences late answers");
        assert!(body.contains("holder.remove()"), "a failed fetch leaves no placeholder");
        assert!(body.contains("yearsHomeDismissed"), "a searched-away card stays retired");
    }

    // ---- date-range bounds (`date_from`/`date_to`, CLI --date-from/--date-to
    // parity on the «Пошук» page) --------------------------------------------

    #[test]
    fn search_endpoint_date_range_bounds() {
        // `/api/search` honors the CLI's inclusive date bounds through the
        // same rada_db::search `org_date >= / <=` conditions: bounds equal
        // to the seeded adoption dates still match (inclusive endpoints),
        // rows outside the window drop out, single-sided bounds work, and
        // an out-of-format value stays a lenient 200 (search_get asserts
        // the status) — validation is the UI's job: parseYmd8 skips the
        // bad bound and paints the muted hint, so the page never sends
        // anything the endpoint would have to reject.
        let st = sorted_state("date-range");
        // Inclusive endpoints: the bounds ARE the oldest/newest seeded dates.
        let v = search_get(&st, "date_from=20010101&date_to=20030303");
        assert_eq!(v["total"], 3, "{v}");
        // Window around the middle doc only: both other rows are excluded.
        let v = search_get(&st, "date_from=20010102&date_to=20030302");
        assert_eq!(hit_nregs(&v), vec!["2000-02"]);
        // Lower bound only (date_desc: newest first).
        let v = search_get(&st, "date_from=20020101");
        assert_eq!(hit_nregs(&v), vec!["3000-03", "2000-02"]);
        // Upper bound only, inclusive on the boundary date itself.
        let v = search_get(&st, "date_to=20020202");
        assert_eq!(hit_nregs(&v), vec!["2000-02", "1000-01"]);
        // Garbage bound: no crash and no 4xx (200 asserted by search_get);
        // the raw string comparison finds nothing, and the UI never lets a
        // value like this out (doSearch sends only toYmd8-validated bounds).
        let v = search_get(&st, "date_from=31.12.2001");
        assert_eq!(v["total"], 0, "{v}");
    }

    #[test]
    fn ui_page_date_range_inputs_deep_link() {
        // The date bounds are «дд.мм.рррр» text inputs with «з від:»/«до:»
        // labels; the pure parseYmd8 (the inverse direction of fmtYmd8)
        // folds them into 8-digit bounds, doSearch sends only validated
        // values (an invalid bound is skipped, never blocks the search,
        // and reveals the muted hint), resetAllFilters zeroes both fields,
        // and the shareable hash grows optional from=/to= keys.
        assert!(ui::INDEX_HTML.contains(">з від:<"));
        assert!(ui::INDEX_HTML.contains(">до:<"));
        assert!(ui::INDEX_HTML.contains("id=\"date-from\" placeholder=\"дд.мм.рррр\""));
        assert!(ui::INDEX_HTML.contains("id=\"date-to\" placeholder=\"дд.мм.рррр\""));
        // Pure parser: digit-guarded dd.mm.yyyy with month/day ranges,
        // reordering to РРРРММДД, plus the canonicalizer that also lets an
        // already-8-digit value through (legacy shareable links).
        assert!(ui::INDEX_HTML.contains("function parseYmd8(s)"));
        assert!(ui::INDEX_HTML.contains(r##"/^([0-9]{2})\.([0-9]{2})\.([0-9]{4})$/"##));
        assert!(ui::INDEX_HTML.contains("if (mm < 1 || mm > 12 || dd < 1 || dd > 31) return null;"));
        assert!(ui::INDEX_HTML.contains("return m[3] + m[2] + m[1];"));
        assert!(ui::INDEX_HTML.contains("function toYmd8(s)"));
        assert!(ui::INDEX_HTML.contains("if (/^[0-9]{8}$/.test(t)) return t;"));
        // doSearch: the validated bound goes out as the CLI's parameter and
        // a malformed one only flips the hint (skip-with-hint design).
        assert!(ui::INDEX_HTML.contains(
            "const y = toYmd8(v);\n    if (y) p.set(param, y);\n    else badDate = true;"
        ));
        assert!(ui::INDEX_HTML.contains("$(\"date-hint\").hidden = !badDate;"));
        // «скинути фільтри» / «скинути все» zero both date fields too.
        assert!(ui::INDEX_HTML.contains(
            "\"date-from\", \"date-to\"]) {\n    $(id).value = \"\";"
        ));
        // Hash pre-fill: optional from=/to= (8-digit only) land in the
        // fields through fmtYmd8, and doSearch writes them back for sharing.
        assert!(ui::INDEX_HTML.contains("key !== \"from\" && key !== \"to\""));
        assert!(ui::INDEX_HTML.contains("$(\"date-from\").value = fmtYmd8(h.from);"));
        assert!(ui::INDEX_HTML.contains("$(\"date-to\").value = fmtYmd8(h.to);"));
        assert!(ui::INDEX_HTML.contains("+ \"from=\" + hf;"));
        assert!(ui::INDEX_HTML.contains("+ \"to=\" + ht;"));
    }

    // ---- landing dictionary quick-search («Пошук у словниках», the CLI
    // `dicts` twin) -------------------------------------------------------------

    #[test]
    fn dicts_endpoint_filters_kind_and_limit() {
        // Seeded temp index in the yeardocs idiom: dict rows across the three
        // kinds incl. a capital-М «Міністер» and a «МІНІСТЕРСТВО …» row to
        // pin the Rust-side case-insensitivity (SQLite LOWER() folds ASCII
        // only, so the folding must happen through to_lowercase on the Rust
        // side — the CLI `dicts` reasoning), and 205 typs rows to pin the
        // limit clamp. Unique dir keeps parallel tests from colliding.
        let dir = std::env::temp_dir().join("rada-web-dicts-test-main");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create test dir");
        let st = Arc::new(State {
            db_path: dir.join("test.sqlite"),
            client: RadaClient::with_pacing(Duration::from_secs(3600)),
            pace_secs: 3600,
            jobs: Mutex::new(HashMap::new()),
            job_seq: AtomicU64::new(0),
        });
        let conn = db_open(&st.db_path).expect("open test index");
        let typs = (1..=205)
            .map(|i| format!("('typs', {i}, 'Тип {i}')"))
            .collect::<Vec<_>>()
            .join(",");
        conn.execute_batch(&format!(
            "INSERT INTO dict(kind, id, label) VALUES
             ('organs', 3, 'Міністер'),
             ('organs', 7, 'МІНІСТЕРСТВО юстиції'),
             ('organs', 11, 'Верховна Рада'),
             ('stan', 5, 'Чинний'),
             {typs};"
        ))
        .expect("seed dicts");
        drop(conn);
        // `GET /api/dicts?{query}` through `route` (query parsed exactly
        // as `handle_request` does), answering status + JSON body.
        let dicts_get = |query: &str| -> (u16, Value) {
            let mut req: Request = TestRequest::new()
                .with_method(Method::Get)
                .with_path("/api/dicts")
                .into();
            let resp = route(&Method::Get, "/api/dicts", &parse_query(query), &mut req, &st)
                .expect("route");
            let code = resp.status_code().0;
            let v = serde_json::from_slice(&resp.into_reader().into_inner())
                .unwrap_or(Value::Null);
            (code, v)
        };
        // A lowercase Cyrillic needle matches the capital-М «Міністер» AND
        // «МІНІСТЕРСТВО …» (Rust-side to_lowercase folding), rows come back
        // id ASC, and every non-matching label stays out.
        let (code, v) = dicts_get("kind=organs&q=міністер");
        assert_eq!(code, 200);
        assert_eq!(v["kind"], "organs");
        let ids: Vec<i64> = v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_i64().unwrap())
            .collect();
        assert_eq!(ids, [3, 7], "{v}");
        assert_eq!(v["rows"][0]["label"], "Міністер");
        // The uppercase needle folds the same pair the other way round.
        let (code, v) = dicts_get("kind=organs&q=МІНІСТЕР");
        assert_eq!(code, 200);
        assert_eq!(v["rows"].as_array().unwrap().len(), 2, "{v}");
        // A whitespace-only needle matches every label (the CLI
        // `dict_matches` rule), and the kind stays scoped: no stan row
        // surfaces through an organs query.
        let (code, v) = dicts_get("kind=organs&q=%20");
        assert_eq!(code, 200);
        assert_eq!(v["rows"].as_array().unwrap().len(), 3);
        let raw = v.to_string();
        assert!(!raw.contains("Чинний"), "organ rows only: {raw}");
        // The stan dictionary answers through the same filter.
        let (code, v) = dicts_get("kind=stan&q=чин");
        assert_eq!(code, 200);
        assert_eq!(v["rows"][0]["id"], 5);
        assert_eq!(v["rows"][0]["label"], "Чинний");
        // Limits: default 50, floor 1, ceiling 200 (the arrivals/fresh clamp
        // idiom), pinned against the 205 seeded typs rows, order id ASC.
        let (code, v) = dicts_get("kind=typs");
        assert_eq!(code, 200);
        assert_eq!(v["rows"].as_array().unwrap().len(), 50, "default limit 50");
        let (code, v) = dicts_get("kind=typs&limit=0");
        assert_eq!(code, 200);
        assert_eq!(v["rows"].as_array().unwrap().len(), 1, "limit floors at 1");
        let (code, v) = dicts_get("kind=typs&limit=9999");
        assert_eq!(code, 200);
        let rows = v["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 200, "limit caps at 200");
        assert_eq!(rows[0]["id"], 1);
        assert_eq!(rows[199]["id"], 200, "ORDER BY id ASC survives the cap");
        // Kind is required and one of the three canonical tokens: missing,
        // empty and unknown values are all 400s in the file's json_err
        // error family.
        assert_eq!(dicts_get("").0, 400);
        assert_eq!(dicts_get("q=чин").0, 400);
        let (code, v) = dicts_get("kind=bogus");
        assert_eq!(code, 400);
        assert!(v["error"].as_str().unwrap().contains("unknown kind"), "{v}");
        // Only GET is routed.
        assert_eq!(call(&st, Method::Post, "/api/dicts", "").0, 405);
    }

    #[test]
    fn ui_page_has_dicts_home_search() {
        // The landing row under the years card: «Пошук у словниках:» label,
        // the text input and three tiny kind chips carrying the canonical
        // kind tokens.
        assert!(ui::INDEX_HTML.contains("id=\"dicts-home\""));
        assert!(ui::INDEX_HTML.contains(">Пошук у словниках:<"));
        assert!(ui::INDEX_HTML.contains("id=\"dicts-q\""));
        for kind in ["stan", "typs", "organs"] {
            assert!(
                ui::INDEX_HTML.contains(&format!("data-kind=\"{kind}\"")),
                "kind chip for {kind}"
            );
        }
        // The pure builder: empty in → empty out, «id · label» rows with
        // data-i indexes so the painter rebinds without raw labels.
        let start = ui::INDEX_HTML
            .find("function dictHomeHtml(rows) {")
            .expect("pure builder present");
        let body = &ui::INDEX_HTML[start..];
        let body = &body[..body.find("\n}").expect("dictHomeHtml end")];
        assert!(body.contains("return \"\""), "empty rows paint nothing");
        assert!(body.contains("data-i=\""), "rows matched by index, not raw labels");
        assert!(body.contains(" · "), "rows read «id · label»");
        assert!(body.contains("escHtml("), "labels escape through the shared helper");
        // The painter: fetches the endpoint, paints through the pure
        // builder, binds rows to the filter-apply flow, skips silently and
        // stays retired once a search has run.
        let start = ui::INDEX_HTML
            .find("async function renderDictsHome() {")
            .expect("painter present");
        let body = &ui::INDEX_HTML[start..];
        let body = &body[..body.find("\n}").expect("renderDictsHome end")];
        assert!(body.contains("/api/dicts?kind="), "fetches the dicts endpoint");
        assert!(body.contains("dictHomeHtml(rows)"), "rows built by the pure builder");
        assert!(body.contains("applyDictRow(dictsHomeKind, r)"), "rows apply the filter on click");
        assert!(body.contains("dictsHomeDismissed"), "a searched-away card stays retired");
        // Click-to-filter: the kind→select map (the three fillDict targets),
        // the option matched by its fillDict-cached label, the select-set +
        // doSearch idiom, and the plain-search fallback for a label no
        // option carries.
        let start = ui::INDEX_HTML
            .find("function applyDictRow(kind, row) {")
            .expect("filter-apply present");
        let body = &ui::INDEX_HTML[start..];
        let body = &body[..body.find("\n}").expect("applyDictRow end")];
        assert!(body.contains("\"f-status\""), "stan rows target the status select");
        assert!(body.contains("\"f-typ\""), "typs rows target the type select");
        assert!(body.contains("\"f-org\""), "organs rows target the organs select");
        assert!(body.contains("optionLabels[selectId]"), "options matched by the fillDict-cached label");
        assert!(body.contains("sel.value = hit;"), "the select is set on a label match");
        assert!(
            body.contains(r##"$("q").value = String(row.label);"##),
            "a label no option carries falls back to a plain search"
        );
        assert!(body.contains("doSearch(0);"), "the click runs the shared search entry point");
        // Lifetime and wiring: retired from doSearch like the fresh/years
        // cards, painted from init, fetched on Enter (the resolve-input
        // idiom — no fetch per keystroke), chips switch the dictionary and
        // organs is the documented default.
        assert!(ui::INDEX_HTML
            .contains("dictsHomeDismissed = true;\n    $(\"dicts-home\").hidden = true;"));
        assert!(
            ui::INDEX_HTML.contains("renderDictsHome();\n  const dl = readDeepLink();"),
            "the card paints from init like the fresh/years cards"
        );
        assert!(ui::INDEX_HTML
            .contains("if (ev.key === \"Enter\") { ev.preventDefault(); renderDictsHome(); }"));
        assert!(ui::INDEX_HTML.contains("selectDictKind(b.dataset.kind)"));
        assert!(ui::INDEX_HTML.contains("let dictsHomeKind = \"organs\";"), "organs is the default kind");
    }
}
