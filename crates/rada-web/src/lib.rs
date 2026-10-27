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

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use rada_core::models::{LawCardJson, StatusId};
use rada_core::RadaClient;
use rada_db::dict::all_labels;
use rada_db::search::{search, SearchQuery};
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

struct State {
    db_path: PathBuf,
    client: RadaClient,
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

fn handle_request(req: Request, st: &State) -> Result<()> {
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

fn route(method: &Method, path: &str, q: &[(String, String)], req: &mut Request, st: &State) -> Result<Resp> {
    if method == &Method::Get && path == "/" {
        return Ok(html_response());
    }
    let Some(rest) = path.strip_prefix("/api/") else {
        return Ok(json_err(404, &format!("unknown path {path}")));
    };
    let result = if rest.starts_with("doc/") {
        doc_handler(method, rest, q, st)
    } else if rest.starts_with("export/") {
        export_handler(method, rest, q, st, req)
    } else if rest == "annotations" || rest.starts_with("annotations/") {
        annotations_handler(method, rest, q, st, req)
    } else if rest.starts_with("dict/") {
        dict_handler(method, rest, st)
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
    json_ok(&json!({
        "docs": docs,
        "last_import": last,
        "db_path": st.db_path.display().to_string(),
        "annotations": annotations,
        "dicts": dicts,
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
        limit: qget(q, "limit")
            .and_then(|s| s.parse().ok())
            .unwrap_or(50)
            .min(200),
        offset: qget(q, "offset").and_then(|s| s.parse().ok()).unwrap_or(0),
    };
    match open_db(st) {
        Ok(conn) => match search(&conn, &q) {
            Ok((hits, total)) => json_ok(&json!({ "total": total, "hits": hits })),
            Err(e) => json_err(500, &format!("search error: {e:#}")),
        },
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
/// `DELETE /api/annotations/{id}`.
fn annotations_handler(
    method: &Method,
    rest: &str,
    q: &[(String, String)],
    st: &State,
    req: &mut Request,
) -> Resp {
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
        let fmt = qget(q, "fmt").unwrap_or("txt").to_string();
        let edition = qget(q, "edition").filter(|s| !s.is_empty()).map(str::to_string);
        match build_export(st, tail, &fmt, edition.as_deref()) {
            Ok((body, ext, filename)) => file_response(&body, &ext, &filename),
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
    Response::from_string(ui::INDEX_HTML.to_string())
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
    }

    #[test]
    fn fmt_yyyymmdd_passthrough() {
        assert_eq!(fmt_yyyymmdd(20261015), "15.10.2026");
        assert_eq!(fmt_yyyymmdd(42), "42");
    }
}
