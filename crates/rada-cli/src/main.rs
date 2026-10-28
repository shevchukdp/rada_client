//! `rada-client` — CLI for the local Ukrainian-legislation index.
//!
//! Subcommands:
//! - `init`      build the full index from the official `doc.txt` dump;
//! - `sync`      incremental update from the updated-documents feed;
//! - `search`    local title search (FTS5 + filters);
//! - `card`      live card for a document (human summary, `--json` for raw JSON);
//! - `text`      clean text of a document (current or a given edition);
//! - `dict`      load reference dictionaries (stan/typs/organs) from the live API;
//! - `arrivals`  live updated-documents feed with a diff against the saved snapshot;
//! - `snapshot`  create / restore gzip snapshots;
//! - `status`    index statistics;
//! - `stats`     offline report on the local index (file, dicts, text cache,
//!               arrivals snapshot; `--json` for machine-readable output);
//! - `export`    save a document (card + text) as txt / md / html;
//! - `web`       local web UI (search, reader, annotations, export).

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rada_core::RadaClient;

#[derive(Parser)]
#[command(
    name = "rada-client",
    about = "Local index + client for the Ukrainian legislation database (zakon.rada.gov.ua)",
    version
)]
struct Cli {
    /// Path to the SQLite index (default: ./data/zakon-index.sqlite).
    #[arg(long, global = true, default_value = "data/zakon-index.sqlite")]
    db: PathBuf,

    /// Minimum pause between API requests, seconds (server asks for 5–7).
    #[arg(long, global = true, default_value_t = 5u64)]
    pace: u64,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Build (or rebuild) the full index from the official doc.txt dump.
    Init {
        /// Use an already-downloaded dump instead of fetching.
        #[arg(long)]
        dump: Option<PathBuf>,
    },
    /// Incremental sync: dictionaries + updated documents.
    Sync,
    /// Search the local index by title with optional filters.
    Search {
        /// Title term (multi-word → FTS5).
        term: Vec<String>,
        /// Publisher id.
        #[arg(long)]
        org: Option<i64>,
        /// Document type id.
        #[arg(long)]
        typ: Option<i64>,
        /// Status id (5 = Чинний, 1 = Втратив чинність, ...).
        #[arg(long)]
        status: Option<i64>,
        /// Adoption date from (YYYYMMDD).
        #[arg(long)]
        date_from: Option<String>,
        /// Adoption date to (YYYYMMDD).
        #[arg(long)]
        date_to: Option<String>,
        /// Document number substring.
        #[arg(long)]
        number: Option<String>,
        /// Page size.
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// Offset.
        #[arg(long, default_value_t = 0)]
        offset: usize,
        /// Sort: date_desc (default) | date_asc | nazva_asc | nazva_desc | podd_desc | relevance.
        #[arg(long, default_value = "date_desc")]
        sort: String,
        /// Emit JSON (default: table).
        #[arg(long)]
        json: bool,
        /// Print facet counts (status / type / publisher) for the query.
        #[arg(long)]
        facets: bool,
    },
    /// Show the live card of a document (human-readable summary;
    /// `--json` prints the full JSON card).
    Card {
        nreg: String,
        /// Emit the full JSON card instead of the summary.
        #[arg(long)]
        json: bool,
        /// Write to this file instead of stdout (parent dirs created).
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Show the clean text of a document (offline cache fallback when live fails).
    Text {
        nreg: String,
        /// Specific edition date YYYYMMDD (default: current).
        #[arg(long)]
        edition: Option<String>,
        /// Write to this file instead of stdout (parent dirs created).
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Offline text cache: statistics, purge (`--purge [nreg]`), or a
    /// full-text scan of cached bodies (`--search TERM`) — offline, Unicode
    /// case-insensitive, ranked by occurrence count.
    Cache {
        /// Delete cached texts (all, or only the given nreg).
        #[arg(long)]
        purge: bool,
        /// Search the cached document texts for this substring.
        #[arg(long)]
        search: Option<String>,
        /// Max results for `--search` (default 30).
        #[arg(long)]
        limit: Option<usize>,
        /// Restrict to one document.
        nreg: Option<String>,
    },
    /// Offline report on the local index: file, documents, last import,
    /// dictionaries, text cache, saved arrivals snapshot (`--json` for
    /// machine-readable output).
    Stats {
        /// Emit JSON instead of the aligned report.
        #[arg(long)]
        json: bool,
    },
    /// Show the live updated-documents feed (r.tsv) and diff it against the
    /// last saved snapshot; `--save` persists the feed for the next run.
    Arrivals {
        /// Persist the fetched feed so the next run diffs against it.
        #[arg(long)]
        save: bool,
        /// Output JSON instead of a plain list.
        #[arg(long)]
        json: bool,
        /// Max rows to print (default 30, capped at 1000).
        #[arg(long, default_value_t = 30)]
        limit: usize,
        /// Write to this file instead of stdout (parent dirs created).
        /// Plain table gets a UTF-8 BOM (Excel-friendly); with `--json` the
        /// file is written BOM-less.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Load the reference dictionaries (stan / typs / organs) from the live API
    /// and print them as `id  label` rows; `--search TERM` filters the printed
    /// labels by a case-insensitive substring (zero matches is not an error).
    Dict {
        /// Load only this kind (`stan`, `typs`, `organs`). Default: all three.
        #[arg(long)]
        kind: Option<String>,
        /// Print only labels containing this term (case-insensitive).
        #[arg(long)]
        search: Option<String>,
        /// Emit JSON instead of the plain listing.
        #[arg(long)]
        json: bool,
    },
    /// Resolve a human name to an nreg via the live `find/a` endpoint.
    /// Prints the nreg when the query is unambiguous, else an error.
    Resolve {
        /// Document name / abbreviation to look up.
        #[arg(required = true)]
        text: Vec<String>,
    },
    /// List «related documents» — links to other acts referenced by this one,
    /// parsed from its live portal page (one paced request).
    Related {
        nreg: String,
        /// Output JSON instead of a plain list.
        #[arg(long)]
        json: bool,
    },
    /// Snapshot helpers.
    Snapshot {
        #[command(subcommand)]
        what: SnapCmd,
    },
    /// Print index statistics.
    Status,
    /// User annotations (bookmarks / highlights / comments) stored in the index.
    Annot {
        #[command(subcommand)]
        what: AnnCmd,
    },
    /// Export a document (card + text) as txt / md / html.
    /// Text prefers the offline cache; the card is fetched live.
    Export {
        nreg: String,
        /// Output format: txt | md | html.
        #[arg(long, default_value = "txt")]
        fmt: String,
        /// Specific edition date YYYYMMDD (default: current).
        #[arg(long)]
        edition: Option<String>,
        /// Write to this file instead of stdout (parent dirs created).
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Start the local web UI (search, reader, annotations, export).
    Web {
        /// Address to bind.
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Port to bind.
        #[arg(long, default_value_t = 8742)]
        port: u16,
        /// Open a browser automatically.
        #[arg(long)]
        open: bool,
    },
}

#[derive(Subcommand)]
enum SnapCmd {
    /// Create a gzip snapshot of the index.
    Create { out: PathBuf },
    /// Restore a gzip snapshot into a new database file.
    Restore { snapshot: PathBuf, to: PathBuf },
    /// Download the published snapshot from the GitHub release and
    /// restore it into a local database file (the fast offline path).
    Fetch {
        /// Exact asset name (default: the latest `*.sqlite.gz` asset).
        #[arg(long)]
        name: Option<String>,
        /// Output DB path (default: the global `--db` path).
        #[arg(long)]
        to: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum AnnCmd {
    /// List annotations (all documents, or only for `nreg`).
    List {
        /// Filter to one document.
        nreg: Option<String>,
        /// Emit JSON.
        #[arg(long)]
        json: bool,
    },
    /// Add an annotation; `kind` is bookmark | highlight | comment.
    Add {
        nreg: String,
        kind: String,
        /// Annotation text (not needed for bookmark).
        text: Option<String>,
        /// Highlight span: start char (optional).
        #[arg(long)]
        start: Option<i64>,
        /// Highlight span: end char (optional).
        #[arg(long)]
        end: Option<i64>,
    },
    /// Delete an annotation by id.
    Del { id: i64 },
    /// Export all annotations as a portable JSON file (default: stdout).
    Export {
        /// Legacy positional target file (same effect as `--out`).
        file: Option<PathBuf>,
        /// Write the JSON to this file instead of stdout (parent dirs created).
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Import annotations from a JSON file (array or {"annotations":[...]}).
    Import { file: PathBuf },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Init { dump } => cmd_init(&cli.db, dump.as_deref(), cli.pace),
        Cmd::Sync => cmd_sync(&cli.db, cli.pace),
        Cmd::Search {
            term,
            org,
            typ,
            status,
            date_from,
            date_to,
            number,
            limit,
            offset,
            sort,
            json,
            facets,
        } => cmd_search(&cli.db, term, org, typ, status, date_from, date_to, number, limit, offset, sort, json, facets),
        Cmd::Card { nreg, json, out } => cmd_card(&nreg, json, out.as_deref(), cli.pace),
        Cmd::Text { nreg, edition, out } => {
            cmd_text(&cli.db, &nreg, edition.as_deref(), out.as_deref(), cli.pace)
        }
        Cmd::Cache { purge, search, limit, nreg } => {
            cmd_cache(&cli.db, purge, search.as_deref(), limit, nreg.as_deref())
        }
        Cmd::Stats { json } => cmd_stats(&cli.db, json),
        Cmd::Arrivals {
            save,
            json,
            limit,
            out,
        } => cmd_arrivals(&cli.db, save, json, limit, out.as_deref(), cli.pace),
        Cmd::Dict { kind, search, json } => {
            cmd_dict(&cli.db, cli.pace, kind.as_deref(), search.as_deref(), json)
        }
        Cmd::Resolve { text } => cmd_resolve(&text.join(" "), cli.pace),
        Cmd::Related { nreg, json } => cmd_related(&cli.db, &nreg, json, cli.pace),
        Cmd::Snapshot { what } => cmd_snapshot(&cli.db, what),
        Cmd::Status => cmd_status(&cli.db),
        Cmd::Annot { what } => cmd_annot(&cli.db, what),
        Cmd::Export { nreg, fmt, edition, out } => {
            cmd_export(&cli.db, &nreg, &fmt, edition.as_deref(), out.as_deref(), cli.pace)
        }
        Cmd::Web { host, port, open } => cmd_web(&cli.db, &host, port, open, cli.pace),
    }
}

fn client(pace: u64) -> RadaClient {
    RadaClient::with_pacing(std::time::Duration::from_secs(pace))
}

fn cmd_init(db: &PathBuf, dump: Option<&std::path::Path>, pace: u64) -> Result<()> {
    let t = Instant::now();
    let raw = match dump {
        Some(p) => {
            eprintln!("[init] reading dump from {}", p.display());
            std::fs::read(p)?
        }
        None => {
            eprintln!("[init] downloading full doc.txt (~51 MB) ...");
            let c = client(pace);
            let bytes = c.full_dump()?;
            eprintln!("[init] got {} bytes", bytes.len());
            bytes
        }
    };
    let mut conn = rada_db::open(db)?;
    let n = rada_db::build::import_dump(&mut conn, &raw)?;
    let _ = conn.execute_batch("ANALYZE;");
    let size = std::fs::metadata(db).map(|m| m.len()).unwrap_or(0);
    eprintln!(
        "[init] done: {n} docs, db {} ({:.1} MB) in {:?}",
        db.display(),
        size as f64 / 1_048_576.0,
        t.elapsed()
    );
    Ok(())
}

fn cmd_sync(db: &PathBuf, pace: u64) -> Result<()> {
    let c = client(pace);
    let mut conn = rada_db::open(db)?;

    eprintln!("[sync] refreshing dictionaries ...");
    let failed = rada_db::sync::refresh_dicts(&mut conn, |kind| Ok(c.dictionary(kind)?))?;
    if !failed.is_empty() {
        eprintln!("[sync] dictionaries not loaded: {}", failed.join(", "));
    }

    eprintln!("[sync] fetching updated-documents feed ...");
    let updates = c.updated_docs()?;
    eprintln!("[sync] {} updated documents", updates.len());

    // The feed carries nreg + title + edition link; refresh metadata via the
    // JSON card when possible, otherwise fall back to the feed row itself.
    let (applied, failed) =
        rada_db::sync::apply_updates(&mut conn, &updates, |nreg| {
            // card() returns a full JSON card; map to a DocCard for the upsert.
            match c.card(nreg) {
                Ok(j) => Ok(rada_core::models::DocCard {
                    dokid: 0, // unknown from card; keep existing via conflict path
                    nreg: j.nreg.clone(),
                    nazva: j.nazva.clone(),
                    status: j.status,
                    types: j.types.clone(),
                    organs: format!(
                        "{}:{}:{}",
                        j.org,
                        j.orgdat.map(|d| d.to_string()).unwrap_or_default(),
                        j.orgnum.clone().unwrap_or_default()
                    ),
                    minjust: j.minjust.clone().unwrap_or_default(),
                    npix: 0,
                    poddat: j.poddat.map(|d| d.to_string()).unwrap_or_default(),
                }),
                Err(e) => Err(anyhow::Error::from(e)),
            }
        })?;
    println!(
        "[sync] applied {applied}, failed {failed} (of {} updated)",
        updates.len()
    );
    Ok(())
}

fn cmd_dict(
    db: &PathBuf,
    pace: u64,
    kind: Option<&str>,
    search: Option<&str>,
    json: bool,
) -> Result<()> {
    let c = client(pace);
    let mut conn = rada_db::open(db)?;
    let kinds: Vec<String> = match kind {
        Some(k) => vec![k.to_string()],
        None => vec!["stan".into(), "typs".into(), "organs".into()],
    };
    // Lookup mode (`--search`/`--json`) is offline-first: only kinds whose
    // local table is still empty get a live fetch. A plain `dict` refreshes
    // all three live as always.
    let lookup = search.is_some() || json;
    for k in &kinds {
        if lookup && !rada_db::dict::all_labels(&conn, k).is_empty() {
            eprintln!("[dict] {k}: локально");
            continue;
        }
        eprintln!("[dict] fetching {k} ...");
        let raw = c.dictionary(k)?;
        let n = rada_db::dict::load_simple_dict(&mut conn, k, &raw)?;
        eprintln!("[dict] {k}: {n} entries");
    }
    // Read the freshly loaded rows back from the index and apply the optional
    // `--search` filter; zero matches is a normal (exit-0) empty filter result.
    let groups: Vec<(String, Vec<(i64, String)>)> = kinds
        .iter()
        .map(|k| {
            let rows = rada_db::dict::all_labels(&conn, k);
            let rows = match search {
                Some(term) => rows
                    .into_iter()
                    .filter(|(_, label)| dict_matches(label, term))
                    .collect(),
                None => rows,
            };
            (k.clone(), rows)
        })
        .collect();
    if json {
        let dicts: Vec<serde_json::Value> = groups
            .iter()
            .map(|(k, rows)| {
                serde_json::json!({
                    "kind": k,
                    "items": rows
                        .iter()
                        .map(|(id, label)| serde_json::json!({ "id": id, "label": label }))
                        .collect::<Vec<_>>(),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({ "dicts": dicts }))
                .map_err(anyhow::Error::from)?
        );
        return Ok(());
    }
    let multi = kind.is_none();
    if search.is_some() && groups.iter().all(|(_, rows)| rows.is_empty()) {
        println!("нічого не знайдено");
        return Ok(());
    }
    for (k, rows) in &groups {
        if rows.is_empty() {
            continue;
        }
        if multi {
            // group header only in all-dicts mode, only before non-empty groups
            println!("{k} ({}):", dict_caption(k));
        }
        for (id, label) in rows {
            println!("  {id}  {label}");
        }
    }
    Ok(())
}

/// Case-insensitive substring test behind `dict --search`. The needle is
/// trimmed once (`" чинн "` behaves as `"чинн"`); an empty or whitespace-only
/// needle matches every label; otherwise the plain `to_lowercase` forms are
/// compared, which folds Ukrainian letters (І/і, Ї/ї, Є/є, ...).
fn dict_matches(label: &str, needle: &str) -> bool {
    let needle = needle.trim();
    if needle.is_empty() {
        return true;
    }
    label.to_lowercase().contains(&needle.to_lowercase())
}

/// Ukrainian group caption printed in the all-dicts `--search` header
/// (`stan (стани):` / `typs (типи):` / `organs (органи):`); unknown kinds
/// repeat their own name.
fn dict_caption(kind: &str) -> &str {
    match kind {
        "stan" => "стани",
        "typs" => "типи",
        "organs" => "органи",
        other => other,
    }
}

fn cmd_search(
    db: &PathBuf,
    term: Vec<String>,
    org: Option<i64>,
    typ: Option<i64>,
    status: Option<i64>,
    date_from: Option<String>,
    date_to: Option<String>,
    number: Option<String>,
    limit: usize,
    offset: usize,
    sort: String,
    json: bool,
    facets: bool,
) -> Result<()> {
    let conn = rada_db::open(db)?;
    let q = rada_db::search::SearchQuery {
        nazva: if term.is_empty() {
            None
        } else {
            Some(term.join(" "))
        },
        org_id: org,
        typ,
        status,
        date_from,
        date_to,
        doc_number: number,
        sort: rada_db::search::Sort::parse(&sort),
        limit,
        offset,
    };
    let t = Instant::now();
    let (hits, total) = rada_db::search::search(&conn, &q)?;
    eprintln!(
        "[search] {} matches (showing {}–{}), {:?}",
        total,
        offset + 1,
        offset + hits.len(),
        t.elapsed()
    );
    // Facets use the same SearchQuery as the hits; a failure is reported
    // muted and never fails the command.
    let facet_counts = if facets {
        match rada_db::facets(&conn, &q) {
            Ok(f) => Some(f),
            Err(e) => {
                eprintln!("[facets] помилка: {e:#}");
                None
            }
        }
    } else {
        None
    };
    if json {
        let mut out = serde_json::json!({ "total": total, "hits": hits });
        if let Some(f) = &facet_counts {
            out["facets"] = serde_json::json!({
                "status": f.status,
                "typ": f.typ,
                "org": f.org,
            });
        }
        println!("{out}");
    } else {
        for h in &hits {
            let status = rada_core::models::StatusId(h.status).label();
            println!(
                "{:<16} {}  [{status}]  №{} от {}  (ред. {})",
                h.nreg,
                truncate(&h.nazva, 70),
                h.org_num,
                fmt_date(&h.org_date),
                fmt_date(&h.poddat),
            );
        }
        if let Some(f) = &facet_counts {
            println!("Фасети:");
            let stan = rada_db::dict::all_labels(&conn, "stan");
            let typs = rada_db::dict::all_labels(&conn, "typs");
            let organs = rada_db::dict::all_labels(&conn, "organs");
            for line in facet_lines(&f.status, &stan) {
                println!("  Статус   {line}");
            }
            for line in facet_lines(&f.typ, &typs) {
                println!("  Тип      {line}");
            }
            for line in facet_lines(&f.org, &organs) {
                println!("  Видавник {line}");
            }
        }
    }
    Ok(())
}

/// Live card of one document: human-readable summary by default, the full
/// JSON card with `--json`. With `--out` the exact stdout body is written to
/// the file (UTF-8, parent dirs created) and nothing is printed to stdout;
/// a live fetch failure is a hard error (the card has no offline cache).
fn cmd_card(nreg: &str, json: bool, out: Option<&Path>, pace: u64) -> Result<()> {
    let c = client(pace);
    let j = c.card(nreg)?;
    let body = if json {
        serde_json::to_string_pretty(&j)?
    } else {
        card_summary(&j)
    };
    match out {
        Some(p) => {
            if let Some(parent) = p.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("create dir {}", parent.display()))?;
                }
            }
            std::fs::write(p, format!("{body}\n"))
                .with_context(|| format!("запис {}", p.display()))?;
            if json {
                eprintln!("{}", card_out_note(p));
            } else {
                eprintln!(
                    "[card] записано {} симв. → {}",
                    body.chars().count(),
                    p.display()
                );
            }
            Ok(())
        }
        None => {
            println!("{body}");
            Ok(())
        }
    }
}

/// Human-readable summary lines of a live JSON card (the default `card`
/// stdout). Optional fields print only when present; dates go through
/// [`fmt_date`].
fn card_summary(j: &rada_core::models::LawCardJson) -> String {
    let mut lines = vec![
        j.nazva.trim().to_string(),
        format!("nreg    : {}", j.nreg),
        format!(
            "status  : {} ({})",
            rada_core::models::StatusId(j.status).label(),
            j.status
        ),
    ];
    if let Some(n) = j.orgnum.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        lines.push(format!("number  : {n}"));
    }
    if let Some(d) = j.orgdat {
        lines.push(format!("adopted : {}", fmt_date(&d.to_string())));
    }
    if let Some(d) = j.poddat {
        lines.push(format!("edition : {}", fmt_date(&d.to_string())));
    }
    if let Some(d) = j.pridat {
        lines.push(format!("inforce : {}", fmt_date(&d.to_string())));
    }
    if let Some(p) = j.pidstava.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        lines.push(format!("basis   : {p}"));
    }
    if !j.types.trim().is_empty() {
        lines.push(format!("types   : {}", j.types));
    }
    if j.org != 0 {
        lines.push(format!("org     : {}", j.org));
    }
    if let Some(m) = j.minjust.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        lines.push(format!("minjust : {m}"));
    }
    if let Some(sz) = j.size {
        lines.push(format!("size    : {sz} bytes"));
    }
    if j.is_archive == Some(true) {
        lines.push("archive : так".to_string());
    }
    lines.push(format!("editions: {}", j.eds.len()));
    lines.join("\n")
}

/// The stderr note printed after `card --out --json` has written `file`
/// (the summary mode prints the out_note-style char-count line instead).
fn card_out_note(file: &Path) -> String {
    format!("[card] записано JSON → {}", file.display())
}

fn cmd_text(
    db: &PathBuf,
    nreg: &str,
    edition: Option<&str>,
    out: Option<&Path>,
    pace: u64,
) -> Result<()> {
    let c = client(pace);
    let key = edition.unwrap_or("");
    let fetched = match edition {
        Some(ed) => c.show_edition_text(nreg, ed),
        None => c.show_text(nreg),
    };
    match fetched {
        Ok(text) => {
            // Refresh the offline cache on every successful fetch.
            if let Ok(conn) = rada_db::open(db) {
                let _ = rada_db::textcache::put(&conn, nreg, key, &text);
            }
            emit_text(&text, out)
        }
        Err(e) => {
            // Offline fallback: serve the cached copy when we have one.
            let hit = rada_db::open(db)
                .ok()
                .and_then(|conn| rada_db::textcache::get(&conn, nreg, key).ok().flatten());
            match hit {
                Some(text) => {
                    eprintln!("[text] live fetch failed ({e}); served from offline cache");
                    emit_text(&text, out)
                }
                None => Err(anyhow::anyhow!("text {nreg}: {e} (no cached copy)")),
            }
        }
    }
}

/// Print `text` to stdout, or (with `--out`) write it as UTF-8 to the file,
/// creating parent directories; a write failure is a hard error.
fn emit_text(text: &str, out: Option<&Path>) -> Result<()> {
    match out {
        Some(p) => {
            if let Some(parent) = p.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("create dir {}", parent.display()))?;
                }
            }
            std::fs::write(p, text).with_context(|| format!("запис {}", p.display()))?;
            eprintln!("{}", out_note(text.chars().count(), p));
            Ok(())
        }
        None => {
            println!("{text}");
            Ok(())
        }
    }
}

/// The stderr note printed after `text --out` has written `file`.
fn out_note(chars: usize, file: &Path) -> String {
    format!("[text] записано {chars} симв. → {}", file.display())
}

/// Validate the `--fmt` value; returns the canonical format string.
fn normalize_fmt(fmt: &str) -> Result<&'static str> {
    match fmt.trim().to_ascii_lowercase().as_str() {
        "txt" => Ok("txt"),
        "md" => Ok("md"),
        "html" => Ok("html"),
        _ => Err(anyhow::anyhow!(
            "unknown export format {fmt:?} (expected txt | md | html)"
        )),
    }
}

fn cmd_export(
    db: &PathBuf,
    nreg: &str,
    fmt: &str,
    edition: Option<&str>,
    out: Option<&Path>,
    pace: u64,
) -> Result<()> {
    let fmt = normalize_fmt(fmt)?;
    let key = edition.unwrap_or("");
    let c = client(pace);

    // Card: live only (there is no offline card cache).
    let card = c.card(nreg);

    // Text: prefer the offline cache, fall back to a live fetch and refresh it.
    let cached = rada_db::open(db)
        .ok()
        .and_then(|conn| rada_db::textcache::get(&conn, nreg, key).ok().flatten());
    let text = match cached {
        Some(text) => text,
        None => {
            let fetched = match edition {
                Some(ed) => c.show_edition_text(nreg, ed),
                None => c.show_text(nreg),
            };
            match fetched {
                Ok(text) => {
                    if let Ok(conn) = rada_db::open(db) {
                        let _ = rada_db::textcache::put(&conn, nreg, key, &text);
                    }
                    text
                }
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "text {nreg}: {e} (no cached copy — run `rada-client text {nreg}` once online)"
                    ));
                }
            }
        }
    };

    let (body, _) = match card {
        Ok(card) => rada_web::render_export(nreg, &card, &text, fmt),
        Err(e) => {
            // Offline-friendly fallback: plain text from the cache, name unknown.
            eprintln!("[export] card {nreg} failed ({e}); exporting txt from offline cache (name unknown)");
            (
                format!("# {nreg} — назва невідома (офлайн-кеш)\n\n{text}\n"),
                "txt".to_string(),
            )
        }
    };

    match out {
        Some(p) => {
            if let Some(parent) = p.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("create dir {}", parent.display()))?;
                }
            }
            std::fs::write(p, &body).with_context(|| format!("write {}", p.display()))?;
            println!("wrote {} ({} bytes)", p.display(), body.as_bytes().len());
        }
        None => print!("{body}"),
    }
    Ok(())
}

fn cmd_cache(db: &PathBuf, purge: bool, search: Option<&str>, limit: Option<usize>, nreg: Option<&str>) -> Result<()> {
    let conn = rada_db::open(db)?;
    if purge {
        let n = rada_db::textcache::purge(&conn, nreg)?;
        eprintln!("[cache] видалено рядків: {n}");
        return Ok(());
    }
    if let Some(term) = search {
        let hits = rada_db::textcache::search_bodies(&conn, term, limit.unwrap_or(30).max(1))?;
        if hits.is_empty() {
            println!("у кешованому тексті збігів для {term:?} немає");
            return Ok(());
        }
        println!("{} документів(а) з «{term}»:", hits.len());
        for h in &hits {
            let title = rada_db::title_by_nreg(&conn, &h.nreg).unwrap_or_default();
            println!("  {}  ×{}  {title}\n      {}", h.nreg, h.hits, h.snippet);
        }
        return Ok(());
    }
    let (rows, bytes) = rada_db::textcache::stats(&conn)?;
    let docs: i64 = match nreg {
        Some(n) => conn
            .query_row(
                "SELECT COUNT(*) FROM docs_text WHERE nreg = ?1",
                [n],
                |r| r.get(0),
            )
            .unwrap_or(0),
        None => rows,
    };
    println!(
        "rows={docs}  total_rows={rows}  size={:.2} MB",
        bytes as f64 / 1_048_576.0
    );
    Ok(())
}

/// Plain-data snapshot behind `stats`: everything read from the local index,
/// ready for both renderers (aligned lines / JSON).
struct StatsReport {
    db: String,
    docs: i64,
    size_bytes: u64,
    last_import: Option<String>,
    /// Newest `org_date` (YYYYMMDD) among indexed cards.
    max_date: Option<String>,
    dict_stan: i64,
    dict_typs: i64,
    dict_organs: i64,
    cache_rows: i64,
    cache_bytes: i64,
    arrivals_at: Option<String>,
    arrivals_docs: i64,
}

/// Aligned `key : value` report lines for `stats` (human mode), card_summary
/// style: empty dict tables and an empty text cache render as «порожньо»,
/// missing timestamps as «—», dates go through [`fmt_date`].
fn stats_lines(r: &StatsReport) -> Vec<String> {
    let mb = |b: u64| format!("{:.1}", b as f64 / 1_048_576.0);
    let dict = |n: i64| if n == 0 { "порожньо".to_string() } else { n.to_string() };
    let cache = if r.cache_rows == 0 {
        "порожньо".to_string()
    } else {
        format!("{} тіл · {} MB", r.cache_rows, mb(r.cache_bytes as u64))
    };
    let rows: Vec<(String, String)> = vec![
        ("файл індексу".into(), r.db.clone()),
        (
            "документів".into(),
            format!("{}  ·  розмір: {} MB", r.docs, mb(r.size_bytes)),
        ),
        (
            "останній імпорт/синк".into(),
            r.last_import.clone().unwrap_or_else(|| "—".into()),
        ),
        (
            "найновіші дані".into(),
            r.max_date.as_deref().map(fmt_date).unwrap_or_else(|| "—".into()),
        ),
        (
            "словники".into(),
            format!(
                "stan {} · typs {} · organs {}",
                dict(r.dict_stan),
                dict(r.dict_typs),
                dict(r.dict_organs)
            ),
        ),
        ("кеш текстів".into(), cache),
        (
            "знімок оновлень".into(),
            format!(
                "{} ({} документів)",
                r.arrivals_at.clone().unwrap_or_else(|| "немає".into()),
                r.arrivals_docs
            ),
        ),
    ];
    let w = rows.iter().map(|(k, _)| k.chars().count()).max().unwrap_or(0);
    rows.into_iter().map(|(k, v)| format!("{k:<w$} : {v}")).collect()
}

/// Offline index report (`stats`): never touches the network. A missing
/// index file is a normal exit-0 report («індекс відсутній: …»); a file that
/// exists but fails to open is a hard error.
fn cmd_stats(db: &PathBuf, json: bool) -> Result<()> {
    if !db.exists() {
        println!("індекс відсутній: {}", db.display());
        return Ok(());
    }
    let conn = rada_db::open(db).with_context(|| format!("open db {}", db.display()))?;
    let docs = rada_db::doc_count(&conn)?;
    let size_bytes = std::fs::metadata(db).map(|m| m.len()).unwrap_or(0);
    let max_date = conn
        .query_row("SELECT MAX(NULLIF(org_date,'')) FROM docs", [], |r| {
            r.get::<_, Option<String>>(0)
        })
        .ok()
        .flatten();
    let dict_count = |kind: &str| -> i64 {
        conn.query_row("SELECT COUNT(*) FROM dict WHERE kind=?1", [kind], |r| r.get(0))
            .unwrap_or(0)
    };
    let (cache_rows, cache_bytes) = rada_db::textcache::stats(&conn).unwrap_or((0, 0));
    let (arrivals_at, arrivals_docs) = match rada_db::sync::load_arrivals(&conn) {
        Some((snap_docs, at)) => {
            let at = if at.trim().is_empty() { None } else { Some(at) };
            (at, snap_docs.len() as i64)
        }
        None => (None, 0),
    };
    let report = StatsReport {
        db: db.display().to_string(),
        docs,
        size_bytes,
        last_import: rada_db::last_import(&conn),
        max_date,
        dict_stan: dict_count("stan"),
        dict_typs: dict_count("typs"),
        dict_organs: dict_count("organs"),
        cache_rows,
        cache_bytes,
        arrivals_at,
        arrivals_docs,
    };
    if json {
        let out = serde_json::json!({
            "db": report.db,
            "docs": report.docs,
            "size_bytes": report.size_bytes,
            "last_import": report.last_import,
            "max_date": report.max_date,
            "dicts": {
                "stan": report.dict_stan,
                "typs": report.dict_typs,
                "organs": report.dict_organs,
            },
            "cache_rows": report.cache_rows,
            "cache_bytes": report.cache_bytes,
            "arrivals_at": report.arrivals_at,
            "arrivals_docs": report.arrivals_docs,
        });
        println!("{}", serde_json::to_string_pretty(&out).map_err(anyhow::Error::from)?);
        return Ok(());
    }
    for line in stats_lines(&report) {
        println!("{line}");
    }
    Ok(())
}

/// Docs in `live` whose nreg is absent from `seen`: feed order preserved,
/// duplicates collapsed, empty `seen` ⇒ everything is new.
fn new_nregs(live: &[String], seen: &[String]) -> Vec<String> {
    let seen: std::collections::HashSet<&str> = seen.iter().map(String::as_str).collect();
    let mut emitted: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for n in live {
        if !seen.contains(n.as_str()) && emitted.insert(n.as_str()) {
            out.push(n.clone());
        }
    }
    out
}

/// Live updated-documents feed (r.tsv) diffed against the saved snapshot.
/// With `--out` the exact stdout body goes to the file (parent dirs created)
/// and nothing prints to stdout: the plain table is written UTF-8 with a BOM
/// (Excel-friendly, like the GUI CSV), the `--json` body BOM-less. `--save`
/// persists the feed regardless of `--out` (after the diff, before returning).
fn cmd_arrivals(
    db: &PathBuf,
    save: bool,
    json: bool,
    limit: usize,
    out: Option<&Path>,
    pace: u64,
) -> Result<()> {
    let c = client(pace);
    let feed = c
        .updated_docs()
        .with_context(|| "failed to fetch the updated-documents feed (r.tsv)")?;

    // Diff against the last saved snapshot BEFORE persisting the new one.
    let snapshot = rada_db::open(db)
        .ok()
        .and_then(|conn| rada_db::sync::load_arrivals(&conn));
    let first_run = snapshot.is_none();
    let seen: Vec<String> = snapshot
        .as_ref()
        .map(|(docs, _)| docs.iter().map(|d| d.nreg.clone()).collect())
        .unwrap_or_default();
    let live_nregs: Vec<String> = feed.iter().map(|d| d.nreg.clone()).collect();
    let new = new_nregs(&live_nregs, &seen);

    if save {
        let conn = rada_db::open(db)
            .with_context(|| format!("open db {}", db.display()))?;
        rada_db::sync::save_arrivals(&conn, &feed)?;
        eprintln!(
            "[arrivals] збережено {} документів — наступний запуск порівняє з цим фідом",
            feed.len()
        );
    }

    if json {
        let body = serde_json::to_string_pretty(&serde_json::json!({
            "items": feed,
            "new_nregs": new,
        }))
        .map_err(anyhow::Error::from)?;
        match out {
            Some(p) => {
                if let Some(parent) = p.parent() {
                    if !parent.as_os_str().is_empty() {
                        std::fs::create_dir_all(parent)
                            .with_context(|| format!("create dir {}", parent.display()))?;
                    }
                }
                std::fs::write(p, format!("{body}\n"))
                    .with_context(|| format!("запис {}", p.display()))?;
                eprintln!("[arrivals] записано JSON → {}", p.display());
            }
            None => println!("{body}"),
        }
        return Ok(());
    }

    if first_run {
        eprintln!("збереженого феду ще немає — усі документи вважаються новими");
    }
    let limit = limit.clamp(1, 1000);
    let rows = feed.len().min(limit);
    let newset: std::collections::HashSet<&str> = new.iter().map(String::as_str).collect();
    let mut lines: Vec<String> = Vec::with_capacity(rows + 2);
    lines.push(format!("{} документів, нових: {}", feed.len(), new.len()));
    for d in feed.iter().take(limit) {
        let marker = if newset.contains(d.nreg.as_str()) { "* " } else { "  " };
        lines.push(format!("{marker}{}  [{}]  {}", d.nreg, d.status, d.nazva));
    }
    if feed.len() > limit {
        lines.push(format!("… ще {} див. --limit", feed.len() - limit));
    }
    let body = lines.join("\n");
    match out {
        Some(p) => {
            if let Some(parent) = p.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("create dir {}", parent.display()))?;
                }
            }
            // UTF-8 BOM prefix: the same Excel-friendly CSV convention as the GUI.
            std::fs::write(p, format!("\u{FEFF}{body}\n"))
                .with_context(|| format!("запис {}", p.display()))?;
            eprintln!("{}", arrivals_out_note(rows, p));
        }
        None => println!("{body}"),
    }
    Ok(())
}

/// The stderr note printed after `arrivals --out` has written the plain
/// table (`rows` document rows) to `file`.
fn arrivals_out_note(rows: usize, file: &Path) -> String {
    format!("[arrivals] записано {rows} рядків → {}", file.display())
}

fn cmd_resolve(text: &str, pace: u64) -> Result<()> {
    let c = client(pace);
    match c.resolve_name(text) {
        Some(nreg) => {
            println!("{nreg}");
            Ok(())
        }
        None => Err(anyhow::anyhow!(
            "no unambiguous match for {text:?} (the API returned a candidate list)"
        )),
    }
}

fn cmd_related(db: &PathBuf, nreg: &str, json: bool, pace: u64) -> Result<()> {
    let c = client(pace);
    let mut list = c.related(nreg)?;
    // Prefer offline index titles over the page's anchor text (the CLI has
    // the index at hand; empty anchors look like bare numbers otherwise).
    if let Ok(conn) = rada_db::open(db) {
        for r in list.iter_mut() {
            if let Some(t) = rada_db::title_by_nreg(&conn, &r.nreg) {
                if !t.trim().is_empty() {
                    r.title = t;
                }
            }
        }
    }
    if json {
        let items: Vec<serde_json::Value> = list
            .iter()
            .map(|r| serde_json::json!({ "nreg": r.nreg, "title": r.title }))
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({ "items": items }))
                .map_err(anyhow::Error::from)?
        );
        return Ok(());
    }
    if list.is_empty() {
        println!("посилань на інші акти не знайдено");
        return Ok(());
    }
    println!("{} пов'язаних документів:", list.len());
    for r in &list {
        println!("  {}  {}", r.nreg, r.title);
    }
    Ok(())
}

fn cmd_snapshot(db: &PathBuf, what: SnapCmd) -> Result<()> {
    match what {
        SnapCmd::Fetch { name, to } => {
            let out = to.clone().unwrap_or_else(|| db.clone());
            cmd_snapshot_fetch(name.as_deref(), &out)
        }
        SnapCmd::Create { out } => {
            let size = rada_db::snapshot::create(db, &out)?;
            println!(
                "snapshot {} ({:.1} MB)",
                out.display(),
                size as f64 / 1_048_576.0
            );
            Ok(())
        }
        SnapCmd::Restore { snapshot, to } => {
            let p = rada_db::snapshot::restore(&snapshot, &to)?;
            println!("restored {}", p.display());
            Ok(())
        }
    }
}

fn cmd_web(db: &PathBuf, host: &str, port: u16, open: bool, pace: u64) -> Result<()> {
    if !db.exists() {
        eprintln!(
            "[web] hint: no index at {} — run `rada-client init` first",
            db.display()
        );
    }
    rada_web::serve(&rada_web::WebConfig {
        db_path: db.clone(),
        host: host.to_string(),
        port,
        pace_secs: pace,
        open_browser: open,
    })
}

fn cmd_snapshot_fetch(name: Option<&str>, out: &Path) -> Result<()> {
    let gh = rada_core::GitHub::default();
    let url = gh.latest_snapshot_url("shevchukdp", "rada_client", name)?;
    eprintln!("[fetch] {url}");
    let gz = out.with_extension("sqlite.gz.tmp");
    let size = gh.download(&url, &gz)?;
    eprintln!(
        "[fetch] {} MB downloaded; restoring to {}",
        size / (1 << 20),
        out.display()
    );
    let restored = rada_db::snapshot::restore(&gz, out)?;
    let _ = std::fs::remove_file(&gz);
    println!("restored {}", restored.display());
    Ok(())
}

fn cmd_annot(db: &PathBuf, what: AnnCmd) -> Result<()> {
    let conn = rada_db::open(db)?;
    match what {
        AnnCmd::List { nreg, json } => {
            let items = match &nreg {
                Some(n) => rada_db::annotations::list(&conn, n)?, // already newest first
                None => {
                    let mut v = rada_db::annotations::all(&conn)?;
                    v.sort_by(|a, b| b.id.cmp(&a.id));
                    v
                }
            };
            if json {
                println!("{}", serde_json::to_string_pretty(&items)?);
                return Ok(());
            }
            if items.is_empty() {
                println!("немає анотацій");
                return Ok(());
            }
            for a in items {
                let span = match (a.span_start, a.span_end) {
                    (Some(s), Some(e)) => format!(" [{s}..{e}]"),
                    _ => String::new(),
                };
                let t: String = a.text.chars().take(60).collect();
                println!("#{:<4} {:<9} {:<12} {}  {}{}", a.id, a.kind, a.nreg, a.created_at, span, t);
            }
        }
        AnnCmd::Add { nreg, kind, text, start, end } => {
            let id = rada_db::annotations::insert(
                &conn,
                &nreg,
                &kind,
                &text.unwrap_or_default(),
                start,
                end,
            )?;
            println!("анотація #{id} додана до {nreg}");
        }
        AnnCmd::Del { id } => {
            if rada_db::annotations::remove(&conn, id)? {
                println!("анотація #{id} видалена");
            } else {
                println!("анотація #{id} не знайдена");
            }
        }
        AnnCmd::Export { file, out } => {
            let json = rada_db::annotations::export_json(&conn)?;
            if let Some(p) = out.as_deref() {
                // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                if let Some(parent) = p.parent() {
                    if !parent.as_os_str().is_empty() {
                        std::fs::create_dir_all(parent)
                            .with_context(|| format!("create dir {}", parent.display()))?;
                    }
                }
                std::fs::write(p, &json).with_context(|| format!("запис {}", p.display()))?;
                let n = rada_db::annotations::all(&conn)?.len();
                eprintln!("{}", annot_out_note(n, p));
            } else if let Some(p) = file.as_deref() {
                // legacy positional form: unchanged behaviour
                if let Some(parent) = p.parent() {
                    if !parent.as_os_str().is_empty() {
                        std::fs::create_dir_all(parent).ok();
                    }
                }
                std::fs::write(p, json)?;
                let n = rada_db::annotations::all(&conn)?.len();
                println!("експортовано {n} анотацій у {}", p.display());
            } else {
                println!("{json}");
            }
        }
        AnnCmd::Import { file } => {
            let raw = std::fs::read_to_string(&file)
                .with_context(|| format!("read {}", file.display()))?;
            let n = rada_db::annotations::import_json(&conn, &raw)?;
            println!("імпортовано {n} анотацій з {}", file.display());
        }
    }
    Ok(())
}

/// The stderr note printed after `annot export --out` has written `file`.
fn annot_out_note(n: usize, file: &Path) -> String {
    format!("[annot] записано {n} анотацій → {}", file.display())
}

fn cmd_status(db: &PathBuf) -> Result<()> {
    if !db.exists() {
        eprintln!("no index at {} (run `rada-client init` first)", db.display());
        return Ok(());
    }
    let conn = rada_db::open(db)?;
    let n = rada_db::doc_count(&conn)?;
    let last = rada_db::last_import(&conn);
    let size = std::fs::metadata(db).map(|m| m.len()).unwrap_or(0);
    println!("index      : {}", db.display());
    println!("documents  : {n}");
    println!("last import: {}", last.unwrap_or_else(|| "unknown".into()));
    println!(
        "db size    : {:.1} MB",
        size as f64 / 1_048_576.0
    );
    for kind in ["stan", "typs", "organs"] {
        let c: i64 = conn
            .query_row("SELECT COUNT(*) FROM dict WHERE kind=?1", [kind], |r| {
                r.get(0)
            })
            .unwrap_or(0);
        println!("dict {kind:<7}: {c} entries");
    }
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    let c: Vec<char> = s.chars().collect();
    if c.len() <= max {
        s.to_string()
    } else {
        format!("{}…", c[..max].iter().collect::<String>())
    }
}

/// Top-6 `label  (count)` strings for one facet column, input order
/// (count desc, then id asc) preserved. `id 0` renders as «немає»; ids
/// missing from `dict` fall back to `id {n}`.
fn facet_lines(counts: &[(i64, usize)], dict: &[(i64, String)]) -> Vec<String> {
    counts
        .iter()
        .take(6)
        .map(|(id, count)| {
            let label = if *id == 0 {
                "немає".to_string()
            } else {
                dict.iter()
                    .find(|(did, _)| did == id)
                    .map(|(_, l)| l.clone())
                    .unwrap_or_else(|| format!("id {id}"))
            };
            format!("{label}  ({count})")
        })
        .collect()
}

/// `YYYYMMDD` → `DD.MM.YYYY`; passthrough when not 8 digits.
fn fmt_date(d: &str) -> String {
    if d.len() == 8 && d.chars().all(|c| c.is_ascii_digit()) {
        format!(
            "{}.{}.{}",
            &d[6..8],
            &d[4..6],
            &d[0..4]
        )
    } else {
        d.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        annot_out_note, arrivals_out_note, card_out_note, dict_matches, facet_lines, new_nregs,
        normalize_fmt, out_note, stats_lines, StatsReport,
    };

    #[test]
    fn stats_lines_renders_aligned_keys_and_placeholders() {
        // Full report: every key present, values formatted, keys column-aligned.
        let full = StatsReport {
            db: "data/zakon-index.sqlite".into(),
            docs: 295340,
            size_bytes: 104_857_600, // exactly 100 MB
            last_import: Some("2026-10-28 07:30:00".into()),
            max_date: Some("20261027".into()),
            dict_stan: 8,
            dict_typs: 40,
            dict_organs: 4213,
            cache_rows: 1234,
            cache_bytes: 10_485_760, // exactly 10 MB
            arrivals_at: Some("20261028-0730".into()),
            arrivals_docs: 25,
        };
        let lines = stats_lines(&full);
        assert_eq!(lines.len(), 7);
        // keys are padded to one width: the separator ':' sits at the same
        // character column in every line (byte offsets would differ — keys
        // are multi-byte Cyrillic)
        let colon_col = |l: &str| l.chars().position(|c| c == ':').unwrap();
        assert_eq!(colon_col(&lines[0]), "останній імпорт/синк".chars().count() + 1);
        assert!(lines[0].contains("файл індексу"));
        assert!(lines.iter().all(|l| colon_col(l) == colon_col(&lines[0])));
        assert!(lines[0].starts_with("файл індексу") && lines[0].contains("data/zakon-index.sqlite"));
        assert!(lines[1].starts_with("документів") && lines[1].contains("295340"));
        assert!(lines[1].contains("розмір: 100.0 MB"));
        assert!(lines[2].starts_with("останній імпорт/синк") && lines[2].contains("2026-10-28 07:30:00"));
        // YYYYMMDD goes through fmt_date → DD.MM.YYYY
        assert!(lines[3].starts_with("найновіші дані") && lines[3].contains("27.10.2026"));
        assert!(lines[4].starts_with("словники") && lines[4].contains("stan 8 · typs 40 · organs 4213"));
        assert!(lines[5].starts_with("кеш текстів") && lines[5].contains("1234 тіл · 10.0 MB"));
        assert!(lines[6].starts_with("знімок оновлень") && lines[6].contains("20261028-0730"));
        assert!(lines[6].contains("25 документів"));
        // Empty report: zero dict tables and an empty cache render «порожньо»,
        // missing timestamps render «—», a missing snapshot renders «немає».
        let empty = StatsReport {
            db: "data/none.sqlite".into(),
            docs: 0,
            size_bytes: 0,
            last_import: None,
            max_date: None,
            dict_stan: 0,
            dict_typs: 0,
            dict_organs: 0,
            cache_rows: 0,
            cache_bytes: 0,
            arrivals_at: None,
            arrivals_docs: 0,
        };
        let lines = stats_lines(&empty);
        assert_eq!(lines.len(), 7);
        assert!(lines[2].contains('—'), "last_import None → «—»");
        assert!(lines[3].contains('—'), "max_date None → «—»");
        assert!(lines[4].contains("stan порожньо · typs порожньо · organs порожньо"));
        assert!(lines[5].contains("порожньо"), "empty cache → «порожньо»");
        assert!(lines[6].contains("немає (0 документів)"));
    }

    #[test]
    fn arrivals_out_note_formats_rows_arrow() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//a.csv");
        let line = arrivals_out_note(42, file);
        assert!(line.starts_with("[arrivals] записано "));
        assert!(line.contains("42"));
        assert!(line.contains("рядків"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
    }

    #[test]
    fn card_out_note_formats_json_written_line() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//c.json");
        let line = card_out_note(file);
        assert!(line.starts_with("[card] записано "));
        assert!(line.contains("JSON"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
    }

    #[test]
    fn out_note_formats_chars_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//a.txt");
        let line = out_note(1234, file);
        assert!(line.starts_with("[text] записано "));
        assert!(line.contains("1234"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
    }

    #[test]
    fn annot_out_note_formats_count_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//a.json");
        let line = annot_out_note(7, file);
        assert!(line.starts_with("[annot] записано "));
        assert!(line.contains("7"));
        assert!(line.contains("анотацій"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
    }

    #[test]
    fn facet_lines_resolves_labels_and_caps_at_six() {
        let dict = vec![
            (5, "Чинний".to_string()),
            (1, "Втратив чинність".to_string()),
        ];
        // known label, id 0 («немає»), unknown id (`id N`); count-desc order preserved
        assert_eq!(
            facet_lines(&[(5, 30), (0, 12), (42, 3)], &dict),
            vec![
                "Чинний  (30)".to_string(),
                "немає  (12)".to_string(),
                "id 42  (3)".to_string(),
            ]
        );
        // cap at the top 6, order preserved
        let counts: Vec<(i64, usize)> = (1..=9).map(|i| (i, 100 - i as usize)).collect();
        let lines = facet_lines(&counts, &[]);
        assert_eq!(lines.len(), 6);
        assert_eq!(lines[0], "id 1  (99)".to_string());
        assert_eq!(lines[5], "id 6  (94)".to_string());
        // empty input ⇒ no lines
        assert!(facet_lines(&[], &dict).is_empty());
    }

    #[test]
    fn normalize_fmt_accepts_known_formats() {
        assert_eq!(normalize_fmt("txt").unwrap(), "txt");
        assert_eq!(normalize_fmt("md").unwrap(), "md");
        assert_eq!(normalize_fmt("html").unwrap(), "html");
        // case- and whitespace-tolerant
        assert_eq!(normalize_fmt(" MD ").unwrap(), "md");
        assert_eq!(normalize_fmt("Html").unwrap(), "html");
    }

    #[test]
    fn normalize_fmt_rejects_unknown_formats() {
        assert!(normalize_fmt("pdf").is_err());
        assert!(normalize_fmt("").is_err());
        assert!(normalize_fmt("markdown").is_err());
    }

    #[test]
    fn dict_matches_is_case_insensitive_and_trims_needle_once() {
        // Ukrainian case folding: uppercase label vs lowercase partial needle
        assert!(dict_matches("Чинний", "чинн"));
        // …and the mirrored pair, plus full-string case-insensitive equality
        assert!(dict_matches("чинний", "Чинн"));
        assert!(dict_matches("ЧИННИЙ", "чинний"));
        // non-match (substring must be contiguous, case does not matter)
        assert!(!dict_matches("Чинний", "не чинн"));
        assert!(!dict_matches("Втратив чинність", "Орган"));
        // empty and whitespace-only needles match everything
        assert!(dict_matches("Чинний", ""));
        assert!(dict_matches("Чинний", "   \t "));
        // needle with surrounding spaces is trimmed once, then matched
        assert!(dict_matches("Чинний", "  чинн  "));
        // interior spaces survive the single trim ⇒ no match
        assert!(!dict_matches("Чинний", " чин ний "));
    }

    #[test]
    fn new_nregs_preserves_order_dedups_and_handles_empty_seen() {
        // feed order is preserved, seen nregs are skipped
        assert_eq!(
            new_nregs(&["b".into(), "a".into(), "c".into()], &["z".into()]),
            vec!["b".to_string(), "a".to_string(), "c".to_string()]
        );
        // duplicates in the live feed collapse into one entry
        assert_eq!(
            new_nregs(&["a".into(), "b".into(), "a".into(), "c".into()], &["a".into()]),
            vec!["b".to_string(), "c".to_string()]
        );
        // empty seen ⇒ everything is new (dedup still applies)
        assert_eq!(
            new_nregs(&["x".into(), "x".into(), "y".into()], &[]),
            vec!["x".to_string(), "y".to_string()]
        );
        // empty live ⇒ nothing new
        assert!(new_nregs(&[], &["a".into()]).is_empty());
    }
}
