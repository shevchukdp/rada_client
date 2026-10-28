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
            "resolve" => {
                if method == &Method::Get {
                    resolve_handler(st, q)
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
    let last = last_import(&conn).unwrap_or_default();
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
/// document bodies (`docs_text`), Unicode case-insensitive, ranked.
fn cache_handler(method: &Method, rest: &str, q: &[(String, String)], st: &State) -> Resp {
    if method != &Method::Get {
        return json_err(405, "method not allowed");
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
            let text = match edition {
                Some(ed) => st.client.show_edition_text(nreg, ed),
                None => st.client.show_text(nreg),
            };
            match text {
                Ok(t) => json_ok(&json!({
                    "text": t,
                    "edition": edition,
                })),
                Err(e) => json_err(502, &format!("text error: {e}")),
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
    fn ui_page_print_passes_theme() {
        // The «Друк» button appends the current theme id so the print view
        // restyles headings with the theme accent (per-theme print CSS).
        assert!(ui::INDEX_HTML.contains("rada-theme"));
        assert!(ui::INDEX_HTML.contains("\"?print=1&theme=\""));
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
}
