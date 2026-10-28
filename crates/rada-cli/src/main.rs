//! `rada-client` — CLI for the local Ukrainian-legislation index.
//!
//! Subcommands:
//! - `init`      build the full index from the official `doc.txt` dump;
//! - `sync`      incremental update from the updated-documents feed;
//! - `search`    local title search (FTS5 + filters);
//! - `card`      live card for a document (human summary, `--json` for raw JSON);
//! - `text`      clean text of a document (current or a given edition);
//! - `backfill`  fetch and cache the act texts of the top hits of a local
//!               title search, so later reading works offline
//!               (`--dry-run` lists cached/missing without fetching);
//! - `dict`      load reference dictionaries (stan/typs/organs) from the live API,
//!               seed one offline from a file (`--from-file`) or report
//!               local freshness (`--fresh`);
//! - `arrivals`  live updated-documents feed with a diff against the saved snapshot;
//! - `snapshot`  create / restore gzip snapshots;
//! - `status`    index statistics;
//! - `stats`     offline report on the local index (file, dicts, text cache,
//!               arrivals snapshot; `--json` for machine-readable output);
//! - `export`    offline export of a cached document body as md / txt / html
//!               (text cache only — never touches the network);
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
        /// Write the result table (or the `--json` body) to this file
        /// instead of stdout (parent dirs created). Plain table gets a
        /// UTF-8 BOM (Excel-friendly); with `--json` the file is written
        /// BOM-less. `--facets` still prints to stdout: the file receives
        /// only the results.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Show the live card of a document (human-readable summary;
    /// `--json` prints the full JSON card).
    Card {
        nreg: String,
        /// Emit the full JSON card instead of the summary.
        #[arg(long)]
        json: bool,
        /// Write to this file instead of stdout (parent dirs created).
        /// Plain summary table gets a UTF-8 BOM (Excel-friendly); with
        /// `--json` the file is written BOM-less.
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
        /// With `--search`: emit the JSON body
        /// (`{"term":…,"hits":[{nreg,title,hits,snippet},…]}`) instead of
        /// the table.
        #[arg(long)]
        json: bool,
        /// With `--search`: write the exact stdout table (or the `--json`
        /// body) to this file instead of stdout (parent dirs created).
        /// Plain table gets a UTF-8 BOM (Excel-friendly); with `--json` the
        /// file is written BOM-less. Zero hits write the BOM alone; the
        /// «збігів … немає» message stays on stdout.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Restrict to one document.
        nreg: Option<String>,
    },
    /// Fetch and cache the act texts of the top hits of a local title search,
    /// so later reading works offline. Best-effort: one document failing does
    /// not abort the run. `--dry-run` lists cached/missing without network.
    Backfill {
        /// Title term for the local search (like `search`).
        #[arg(long)]
        query: String,
        /// Max hits to consider (default 10, clamped to 1..=25).
        #[arg(long, default_value_t = 10)]
        limit: usize,
        /// List which documents are cached/missing only: no network, no writes.
        #[arg(long)]
        dry_run: bool,
        /// Emit JSON instead of the progress lines.
        #[arg(long)]
        json: bool,
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
        /// Keep only docs whose derived edition date is within the last N
        /// days (today inclusive): the `edYYYYMMDD` in the `link`, else the
        /// first `YYYYMMDD` in the publication text. Docs with no derivable
        /// date are KEPT (better too many than silently dropped). The filter
        /// runs after the fetch, so `--json` and `--out` honor it too.
        #[arg(long)]
        since: Option<u32>,
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
        /// Seed ONE dictionary from a local file instead of the network: the
        /// raw portal dump exactly as `RadaClient::dictionary(k)` returns it —
        /// `id<TAB>label` lines in Windows-1251 (not JSON). Requires `--kind`;
        /// the loaded rows then render through the normal (offline) path.
        #[arg(long, alias = "from_file")]
        from_file: Option<PathBuf>,
        /// Offline freshness report: print each requested kind's row count
        /// («порожньо» when its table is empty) and never fetch a non-empty
        /// kind; with `--json` emits `{"fresh": {kind: n, …}}`.
        #[arg(long)]
        fresh: bool,
        /// Write the plain listing (or the `--json` body) to this file
        /// instead of stdout (parent dirs created). Plain table gets a
        /// UTF-8 BOM (Excel-friendly); with `--json` the file is written
        /// BOM-less. `--fresh` still prints its report to stdout: the file
        /// receives only the normal kind listing (written after any
        /// live refresh).
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Resolve a human name to an nreg: offline-first via the local index
    /// (exact nreg / title search), then the live `find/a` endpoint only when
    /// the index is silent. Prints the nreg when the query is unambiguous,
    /// else the candidate table.
    Resolve {
        /// Document name / abbreviation to look up.
        #[arg(required = true)]
        text: Vec<String>,
        /// Max candidates to print (default 10, capped at 50).
        #[arg(long, default_value_t = 10)]
        limit: usize,
        /// Output JSON instead of the plain table.
        #[arg(long)]
        json: bool,
        /// Write the local candidate table to this file instead of stdout
        /// (parent dirs created). Plain table gets a UTF-8 BOM
        /// (Excel-friendly); with `--json` the file is written BOM-less.
        /// The live fallback path keeps printing to stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// List «related documents» — links to other acts referenced by this one,
    /// parsed from its live portal page (one paced request).
    Related {
        nreg: String,
        /// Output JSON instead of a plain list.
        #[arg(long)]
        json: bool,
        /// Write to this file instead of stdout (parent dirs created).
        /// Plain table gets a UTF-8 BOM (Excel-friendly); with `--json` the
        /// file is written BOM-less.
        #[arg(long)]
        out: Option<PathBuf>,
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
    /// Offline export of a CACHED document body as md / txt / html: the
    /// text comes from the local text cache only — the command never
    /// touches the network. Export the current edition; read the document
    /// once (`text <nreg>`) while online to fill the cache.
    Export {
        nreg: String,
        /// Output format: md (default) | txt | html (case-insensitive).
        #[arg(long = "format", alias = "fmt", default_value = "md")]
        fmt: String,
        /// Write to this file instead of stdout (parent dirs created).
        /// Human document: always written UTF-8 WITH a BOM; stdout output
        /// is the same bytes without the BOM.
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
        /// Write the annotation table (or the `--json` body) to this file
        /// instead of stdout (parent dirs created). Plain table gets a
        /// UTF-8 BOM (Excel-friendly); with `--json` the file is written
        /// BOM-less. The «немає анотацій» message stays on stdout: an
        /// empty result writes only the BOM, like `search --out`.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Add an annotation; `kind` is bookmark | highlight | comment.
    Add {
        nreg: String,
        kind: String,
        /// Annotation text (not needed for bookmark). Exactly one of this
        /// positional text and `--text-file` may supply the text.
        text: Option<String>,
        /// Read the annotation text from this UTF-8 file instead of the
        /// positional argument (surrounding whitespace is trimmed); blank
        /// files are an error. Mutually exclusive with the positional text.
        #[arg(long, alias = "text_file")]
        text_file: Option<PathBuf>,
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
            out,
        } => cmd_search(&cli.db, term, org, typ, status, date_from, date_to, number, limit, offset, sort, json, facets, out.as_deref()),
        Cmd::Card { nreg, json, out } => cmd_card(&nreg, json, out.as_deref(), cli.pace),
        Cmd::Text { nreg, edition, out } => {
            cmd_text(&cli.db, &nreg, edition.as_deref(), out.as_deref(), cli.pace)
        }
        Cmd::Cache { purge, search, limit, nreg, json, out } => {
            cmd_cache(&cli.db, purge, search.as_deref(), limit, nreg.as_deref(), json, out.as_deref())
        }
        Cmd::Backfill { query, limit, dry_run, json } => {
            cmd_backfill(&cli.db, &query, limit, dry_run, json, cli.pace)
        }
        Cmd::Stats { json } => cmd_stats(&cli.db, json),
        Cmd::Arrivals {
            save,
            json,
            limit,
            since,
            out,
        } => cmd_arrivals(&cli.db, save, json, limit, since, out.as_deref(), cli.pace),
        Cmd::Dict {
            kind,
            search,
            json,
            from_file,
            fresh,
            out,
        } => cmd_dict(
            &cli.db,
            cli.pace,
            kind.as_deref(),
            search.as_deref(),
            json,
            from_file.as_deref(),
            fresh,
            out.as_deref(),
        ),
        Cmd::Resolve { text, limit, json, out } => {
            cmd_resolve(&cli.db, &text.join(" "), limit, json, out.as_deref(), cli.pace)
        }
        Cmd::Related { nreg, json, out } => {
            cmd_related(&cli.db, &nreg, json, out.as_deref(), cli.pace)
        }
        Cmd::Snapshot { what } => cmd_snapshot(&cli.db, what),
        Cmd::Status => cmd_status(&cli.db),
        Cmd::Annot { what } => cmd_annot(&cli.db, what),
        Cmd::Export { nreg, fmt, out } => {
            cmd_export(&cli.db, &nreg, &fmt, out.as_deref())
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

/// Reference dictionaries (stan / typs / organs): lookup modes are
/// offline-first, a plain `dict` refreshes all three live. With `--out`
/// the exact stdout body of the normal listing goes to the file (parent
/// dirs created) and nothing prints to stdout: the plain table is written
/// UTF-8 with a BOM (Excel-friendly, like arrivals/resolve/related), the
/// `--json` body BOM-less; one `[dict] {kind}: записано …` note per kind
/// goes to stderr. `--fresh` keeps its report on stdout even with `--out`
/// (the `search --facets` rule): the file receives only the kind listing.
fn cmd_dict(
    db: &PathBuf,
    pace: u64,
    kind: Option<&str>,
    search: Option<&str>,
    json: bool,
    from_file: Option<&Path>,
    fresh: bool,
    out: Option<&Path>,
) -> Result<()> {
    // Offline seed works on exactly one kind, so --kind is mandatory there.
    if from_file.is_some() && kind.is_none() {
        return Err(anyhow::anyhow!("--from_file потребує --kind"));
    }
    let mut conn = rada_db::open(db)?;
    let kinds: Vec<String> = match kind {
        Some(k) => vec![k.to_string()],
        None => vec!["stan".into(), "typs".into(), "organs".into()],
    };
    // `--from-file`: load ONE dictionary from a local dump (the raw bytes the
    // portal serves, `id<TAB>label` in Windows-1251 — exactly what
    // `RadaClient::dictionary` returns) and skip the network entirely; the
    // rows render through the normal table / `--json` path below.
    if let Some(path) = from_file {
        let k = kind.expect("--kind checked above");
        let raw = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
        let n = rada_db::dict::load_simple_dict(&mut conn, k, &raw)?;
        eprintln!("[dict] {k}: {n} entries (з файлу)");
    }
    // Lookup modes (`--search` / `--json` / `--fresh` / `--from-file`) are
    // offline-first: only kinds whose local table is still empty get a live
    // fetch. A plain `dict` refreshes all three live as always.
    let lookup = search.is_some() || json || fresh || from_file.is_some();
    if from_file.is_none() {
        let c = client(pace);
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
    }
    // `--fresh`: pure row-count report over what the steps above produced.
    // The report is stdout-only even with `--out` (the `search --facets`
    // rule); only with `--out` does execution continue into the normal
    // listing below, which then goes to the file.
    if fresh {
        let counts: Vec<(&str, i64)> = kinds
            .iter()
            .map(|k| (k.as_str(), rada_db::dict::all_labels(&conn, k).len() as i64))
            .collect();
        if json {
            let mut fresh_map = serde_json::Map::new();
            for (k, n) in &counts {
                fresh_map.insert((*k).to_string(), serde_json::json!(n));
            }
            let report = serde_json::Value::Object(
                [("fresh".to_string(), serde_json::Value::Object(fresh_map))]
                    .into_iter()
                    .collect(),
            );
            println!(
                "{}",
                serde_json::to_string_pretty(&report).map_err(anyhow::Error::from)?
            );
        } else {
            for line in dict_fresh_lines(&counts) {
                println!("{line}");
            }
        }
        if out.is_none() {
            return Ok(());
        }
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
        let body = serde_json::to_string_pretty(&serde_json::json!({ "dicts": dicts }))
            .map_err(anyhow::Error::from)?;
        match out {
            Some(p) => {
                // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                write_out_file(p, format!("{body}\n").as_bytes())?;
                eprintln!("[dict] записано JSON → {}", p.display());
            }
            None => println!("{body}"),
        }
        return Ok(());
    }
    let lines = dict_plain_lines(&groups, kind.is_none(), search.is_some());
    match out {
        Some(p) => {
            // UTF-8 BOM prefix: the same Excel-friendly convention as
            // arrivals/resolve/related; the bytes are exactly the stdout
            // listing, so an empty body (no rows printed) carries no
            // trailing newline either.
            let text = if lines.is_empty() {
                "\u{FEFF}".to_string()
            } else {
                format!("\u{FEFF}{}\n", lines.join("\n"))
            };
            write_out_file(p, text.as_bytes())?;
            // one note per requested kind, in the requested order
            for (k, rows) in &groups {
                eprintln!("{}", dict_out_note(k, rows.len(), p));
            }
        }
        None => {
            for line in lines {
                println!("{line}");
            }
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

/// Aligned `dict --fresh` report lines: one per kind in input order,
/// `  {k}: {n} записів` for a populated table and `  {k}: порожньо` for an
/// empty one; an empty slice renders no lines.
fn dict_fresh_lines(counts: &[(&str, i64)]) -> Vec<String> {
    counts
        .iter()
        .map(|(k, n)| {
            if *n == 0 {
                format!("  {k}: порожньо")
            } else {
                format!("  {k}: {n} записів")
            }
        })
        .collect()
}

/// The plain-listing body of `dict` (shared by stdout and `--out`), one
/// string per printed line: a `{k} (caption):` header before every
/// non-empty group in all-dicts mode, then `  {id}  {label}` rows; empty
/// groups print nothing. When `--search` filtered every group to zero
/// rows (`searched`), the single «нічого не знайдено» line replaces the
/// (empty) listing, exactly as the historical stdout did.
fn dict_plain_lines(
    groups: &[(String, Vec<(i64, String)>)],
    multi: bool,
    searched: bool,
) -> Vec<String> {
    if searched && groups.iter().all(|(_, rows)| rows.is_empty()) {
        return vec!["нічого не знайдено".to_string()];
    }
    let mut lines = Vec::new();
    for (k, rows) in groups {
        if rows.is_empty() {
            continue;
        }
        if multi {
            // group header only in all-dicts mode, only before non-empty groups
            lines.push(format!("{k} ({}):", dict_caption(k)));
        }
        for (id, label) in rows {
            lines.push(format!("  {id}  {label}"));
        }
    }
    lines
}

/// The stderr note printed after `dict --out` has written the plain table
/// to `file`: one line per requested kind with that kind's filtered row
/// count (`[dict] stan: записано 10 записів → out\d.txt`).
fn dict_out_note(kind: &str, n: usize, file: &Path) -> String {
    format!("[dict] {kind}: записано {n} записів → {}", file.display())
}

/// Offline FTS5 title search with filters. Default output is the hit table
/// on stdout (`--json` the compact `{"total":…,"hits":[…]}` body). With
/// `--out` the exact stdout body goes to the file (parent dirs created)
/// and nothing prints to stdout: the plain table is written UTF-8 with a
/// BOM (Excel-friendly, like arrivals/resolve/related), the `--json` body
/// BOM-less. `--facets` is an analysis report, not the result list: it
/// still prints to stdout even with `--out` (embedded in the JSON body
/// when `--json` is given).
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
    out: Option<&Path>,
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
        let mut value = serde_json::json!({ "total": total, "hits": hits });
        if let Some(f) = &facet_counts {
            value["facets"] = serde_json::json!({
                "status": f.status,
                "typ": f.typ,
                "org": f.org,
            });
        }
        let body = value.to_string();
        match out {
            Some(p) => {
                // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                write_out_file(p, format!("{body}\n").as_bytes())?;
                eprintln!("[search] записано JSON → {}", p.display());
            }
            None => println!("{body}"),
        }
        return Ok(());
    }
    let body = search_table(&hits);
    match out {
        Some(p) => {
            // UTF-8 BOM prefix: the same Excel-friendly convention as
            // arrivals; the bytes are exactly the stdout table, so an
            // empty result (no rows) carries no trailing newline either.
            let text = if body.is_empty() {
                "\u{FEFF}".to_string()
            } else {
                format!("\u{FEFF}{body}\n")
            };
            write_out_file(p, text.as_bytes())?;
            eprintln!("{}", search_out_note(hits.len(), p));
        }
        None => {
            // Empty result: the per-row renderer printed nothing, stay byte-identical.
            if !body.is_empty() {
                println!("{body}");
            }
        }
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
    Ok(())
}

/// The plain-table body of `search` (shared by stdout and `--out`): one
/// `{nreg}  {nazva}  [{status}]  №{num} от {date}  (ред. {edition})` row
/// per hit joined with newlines (search-style formatting, title truncated
/// to 70 chars, dates through [`fmt_date`]). The renderer prints no total
/// or header line, so an empty result renders as an empty string.
fn search_table(hits: &[rada_db::SearchHit]) -> String {
    hits.iter()
        .map(|h| {
            let status = rada_core::models::StatusId(h.status).label();
            format!(
                "{:<16} {}  [{status}]  №{} от {}  (ред. {})",
                h.nreg,
                truncate(&h.nazva, 70),
                h.org_num,
                fmt_date(&h.org_date),
                fmt_date(&h.poddat),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The stderr note printed after `search --out` has written the plain
/// table (`n` hits) to `file`.
fn search_out_note(n: usize, file: &Path) -> String {
    format!("[search] записано {n} результатів → {}", file.display())
}

/// Live card of one document: human-readable field table on stdout by
/// default, the full pretty JSON card with `--json`. With `--out` the exact
/// stdout body goes to the file (parent dirs created) and nothing prints to
/// stdout: the human table is written UTF-8 with a BOM (Excel-friendly, like
/// search/arrivals/resolve/related), the `--json` body BOM-less. The fetch
/// stays live (one paced request): a fetch failure is a hard error (the
/// card has no offline cache) and so is a write failure («запис {file}»).
fn cmd_card(nreg: &str, json: bool, out: Option<&Path>, pace: u64) -> Result<()> {
    let c = client(pace);
    let j = c.card(nreg)?;
    let body = card_body(&j, json)?;
    match out {
        Some(p) => {
            if json {
                // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                write_out_file(p, format!("{body}\n").as_bytes())?;
            } else {
                // human table: UTF-8 BOM prefix, the same Excel-friendly
                // convention as search/arrivals/resolve/related
                write_out_file(p, format!("\u{FEFF}{body}\n").as_bytes())?;
            }
            eprintln!("{}", card_out_note(nreg, p));
            Ok(())
        }
        None => {
            println!("{body}");
            Ok(())
        }
    }
}

/// The card body (shared by stdout and `--out` so the two cannot drift):
/// the pretty JSON card with `--json`, otherwise the human-readable field
/// table from [`card_summary`]. Both modes print exactly this string plus a
/// trailing newline on stdout, and the `--out` file receives the same bytes
/// (plus the BOM in summary mode).
fn card_body(j: &rada_core::models::LawCardJson, json: bool) -> Result<String> {
    if json {
        serde_json::to_string_pretty(j).map_err(anyhow::Error::from)
    } else {
        Ok(card_summary(j))
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

/// The stderr note printed after `card --out` has written `file`, shared by
/// both the `--json` and the human-table mode (`[card] {nreg} → {file}`).
fn card_out_note(nreg: &str, file: &Path) -> String {
    format!("[card] {nreg} → {}", file.display())
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

/// Validate the `--format` value; returns the canonical format string.
fn normalize_fmt(fmt: &str) -> Result<&'static str> {
    match fmt.trim().to_ascii_lowercase().as_str() {
        "txt" => Ok("txt"),
        "md" => Ok("md"),
        "html" => Ok("html"),
        _ => Err(anyhow::anyhow!(
            "невідомий формат {fmt} (md|txt|html)"
        )),
    }
}

/// Offline export of a CACHED document body: the text is read from the
/// local text cache under the current-edition key (`""`) and the title
/// from the offline index — the command never builds a paced client and
/// never touches the network. An uncached `nreg` is a hard error pointing
/// at `text` (the cache warmer). With `--out` the rendered document is
/// written UTF-8 WITH a BOM (human document, parent dirs created), stdout
/// stays silent and one stderr note names the file; without `--out` the
/// exact same bytes minus the BOM go to stdout.
fn cmd_export(db: &PathBuf, nreg: &str, fmt: &str, out: Option<&Path>) -> Result<()> {
    let fmt = normalize_fmt(fmt)?;
    let conn = rada_db::open(db)?;
    let body = rada_db::textcache::get(&conn, nreg, "")?.ok_or_else(|| {
        anyhow::anyhow!("документ {nreg} не кешовано — спершу прочитайте його (text {nreg})")
    })?;
    // Offline title lookup; an nreg missing from `docs` exports under its own nreg.
    let title = rada_db::title_by_nreg(&conn, nreg).unwrap_or_else(|| nreg.to_string());
    let doc = match fmt {
        "md" => export_markdown(&title, nreg, &body),
        "txt" => export_plain(&title, nreg, &body),
        _ => export_html(&title, nreg, &body),
    };
    match out {
        Some(p) => {
            // human document: UTF-8 with a BOM, like the plain tables
            write_out_file(p, format!("\u{FEFF}{doc}").as_bytes())?;
            eprintln!("{}", export_out_note(fmt, nreg, p));
        }
        None => print!("{doc}"),
    }
    Ok(())
}

/// Markdown rendering of an exported document: `# {title}` header, a
/// `> {nreg} — rada-client {version}` quote line, a blank line, then the
/// cached body with a trailing newline.
fn export_markdown(title: &str, nreg: &str, body: &str) -> String {
    format!(
        "# {title}\n> {nreg} — rada-client {}\n\n{body}\n",
        env!("CARGO_PKG_VERSION")
    )
}

/// Plain-text rendering of an exported document: `{title} ({nreg})` line,
/// a 60-character `=` rule, a blank line, then the cached body with a
/// trailing newline.
fn export_plain(title: &str, nreg: &str, body: &str) -> String {
    format!(
        "{title} ({nreg})\n{}\n\n{body}\n",
        "=".repeat(60)
    )
}

/// Escape the HTML-significant characters (`& < > "`) so cached bodies
/// and index titles can never break out of their element.
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// Minimal standalone HTML page for an exported document: doctype, UTF-8
/// meta, escaped `<title>`/`<h1>`, a `<p class="meta">` line with the
/// escaped nreg and app version, then the body split on blank-line
/// boundaries into escaped `<p>` paragraphs (multi-line paragraphs keep
/// their internal newlines).
fn export_html(title: &str, nreg: &str, body: &str) -> String {
    let t = html_escape(title);
    let mut out = String::new();
    out.push_str("<!doctype html>\n");
    out.push_str("<meta charset=\"utf-8\">\n");
    out.push_str(&format!("<title>{t}</title>\n"));
    out.push_str(&format!("<h1>{t}</h1>\n"));
    out.push_str(&format!(
        "<p class=\"meta\">{} · rada-client {}</p>\n",
        html_escape(nreg),
        env!("CARGO_PKG_VERSION")
    ));
    let mut lines: Vec<&str> = Vec::new();
    let mut paras: Vec<String> = Vec::new();
    for line in body.lines() {
        if line.trim().is_empty() {
            if !lines.is_empty() {
                paras.push(lines.join("\n"));
                lines.clear();
            }
        } else {
            lines.push(line);
        }
    }
    if !lines.is_empty() {
        paras.push(lines.join("\n"));
    }
    for p in paras {
        out.push_str(&format!("<p>{}</p>\n", html_escape(&p)));
    }
    out
}

/// The stderr note printed after `export --out` has written `file`
/// (`[export] {nreg} ({fmt}) → {file}`).
fn export_out_note(fmt: &str, nreg: &str, file: &Path) -> String {
    format!("[export] {nreg} ({fmt}) → {}", file.display())
}

/// Offline text cache: `--purge` deletes cached bodies (all or one `nreg`),
/// `--search TERM` scans the cached current texts offline (ranked by
/// occurrence count), and the bare form prints cache statistics. With
/// `--search --out` the exact stdout body goes to the file (parent dirs
/// created) and nothing prints to stdout: the plain table is written UTF-8
/// with a BOM (Excel-friendly, like search/arrivals/resolve/related), the
/// `--json` body BOM-less; zero hits write the BOM alone (n=0 note). The
/// «у кешованому тексті збігів … немає» line is stdout-only (the
/// `search --facets` rule): it never enters the file. `--out` and `--json`
/// apply to `--search` only.
fn cmd_cache(
    db: &PathBuf,
    purge: bool,
    search: Option<&str>,
    limit: Option<usize>,
    nreg: Option<&str>,
    json: bool,
    out: Option<&Path>,
) -> Result<()> {
    let conn = rada_db::open(db)?;
    if purge {
        let n = rada_db::textcache::purge(&conn, nreg)?;
        eprintln!("[cache] видалено рядків: {n}");
        return Ok(());
    }
    if let Some(term) = search {
        let hits = rada_db::textcache::search_bodies(&conn, term, limit.unwrap_or(30).max(1))?;
        // Titles come from the offline index (the historical stdout shape);
        // an nreg missing from `docs` renders the empty title, as `unwrap_or_default`.
        let rows: Vec<(rada_db::textcache::CacheHit, String)> = hits
            .into_iter()
            .map(|h| {
                let title = rada_db::title_by_nreg(&conn, &h.nreg).unwrap_or_default();
                (h, title)
            })
            .collect();
        if json {
            let value = serde_json::json!({
                "term": term,
                "hits": rows
                    .iter()
                    .map(|(h, title)| serde_json::json!({
                        "nreg": h.nreg,
                        "title": title,
                        "hits": h.hits,
                        "snippet": h.snippet,
                    }))
                    .collect::<Vec<_>>(),
            });
            let body = serde_json::to_string_pretty(&value).map_err(anyhow::Error::from)?;
            match out {
                Some(p) => {
                    // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                    write_out_file(p, format!("{body}\n").as_bytes())?;
                    eprintln!("[cache] записано JSON → {}", p.display());
                }
                None => println!("{body}"),
            }
            return Ok(());
        }
        let lines = cache_search_lines(term, &rows);
        match out {
            Some(p) => {
                // UTF-8 BOM prefix: the same Excel-friendly convention as
                // search/arrivals/resolve/related; the bytes are exactly the
                // stdout table, so an empty result (zero hits) carries the
                // BOM alone and no trailing newline.
                let text = if lines.is_empty() {
                    "\u{FEFF}".to_string()
                } else {
                    format!("\u{FEFF}{}\n", lines.join("\n"))
                };
                write_out_file(p, text.as_bytes())?;
                eprintln!("{}", cache_search_out_note(rows.len(), p));
            }
            None => {
                if lines.is_empty() {
                    // stdout-only zero-hit message, byte-identical to history.
                    println!("у кешованому тексті збігів для {term:?} немає");
                }
                for line in lines {
                    println!("{line}");
                }
            }
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

/// The plain-table body of `cache --search` (shared by stdout and `--out`
/// so the two cannot drift): «{n} документів(а) з «{term}»:» header, then
/// two lines per hit — `  {nreg}  ×{hits}  {title}` and the indented
/// `      {snippet}` context window. `CacheHit` carries no title, so the
/// caller resolves it from the local index first and passes `(hit, title)`
/// pairs. Zero hits render no lines: the `--out` file then carries the BOM
/// alone and the stdout-only «збігів … немає» message never enters it.
fn cache_search_lines(
    term: &str,
    hits: &[(rada_db::textcache::CacheHit, String)],
) -> Vec<String> {
    if hits.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::with_capacity(1 + 2 * hits.len());
    lines.push(format!("{} документів(а) з «{term}»:", hits.len()));
    for (h, title) in hits {
        lines.push(format!("  {}  ×{}  {title}", h.nreg, h.hits));
        lines.push(format!("      {}", h.snippet));
    }
    lines
}

/// The stderr note printed after `cache --search --out` has written the
/// plain table (`n` matched documents) to `file`.
fn cache_search_out_note(n: usize, file: &Path) -> String {
    format!("[cache] записано {n} збігів → {}", file.display())
}

/// Final summary line of `backfill` (one line, pure): counts of the run plus
/// the cache row total afterwards (`textcache::stats`).
fn backfill_summary(fetched: usize, cached_already: usize, failed: usize, total_rows: i64) -> String {
    format!(
        "отримано {fetched}, вже в кеші {cached_already}, помилок {failed}; кеш тепер: {total_rows} тіл"
    )
}

/// Fetch and cache the act texts of the top hits of a local title search so
/// later reading is offline. The hits come from `rada_db::search::search`
/// (`nazva` only, `limit` clamped to 1..=25); already-cached documents are
/// skipped, each missing one goes through the paced client `show_text` and is
/// put into the cache under the current edition (`""`), like `text` does.
/// A single fetch failure prints a muted line and never aborts the loop —
/// the command is best-effort and exits 0 even when every fetch fails.
/// `--dry-run` only lists the cached/missing state: no network, no writes.
fn cmd_backfill(
    db: &PathBuf,
    query: &str,
    limit: usize,
    dry_run: bool,
    json: bool,
    pace: u64,
) -> Result<()> {
    let limit = limit.clamp(1, 25);
    let conn = rada_db::open(db)?;
    let q = rada_db::search::SearchQuery {
        nazva: Some(query.to_string()),
        limit,
        offset: 0,
        ..Default::default()
    };
    let (hits, _total) = rada_db::search::search(&conn, &q)?;
    if hits.is_empty() {
        println!("нічого не знайдено");
        return Ok(());
    }
    let n = hits.len();
    // The paced client is built only for a real run — `--dry-run` never touches
    // the network.
    let c = if dry_run { None } else { Some(client(pace)) };
    let mut fetched = 0usize;
    let mut cached_already = 0usize;
    let mut failed = 0usize;
    for (i, h) in hits.iter().enumerate() {
        let i = i + 1;
        if rada_db::textcache::cached(&conn, &h.nreg).unwrap_or(false) {
            cached_already += 1;
            if !json {
                println!("  [{i}/{n}] {} — уже в кеші", h.nreg);
            }
            continue;
        }
        if dry_run {
            if !json {
                println!("  [{i}/{n}] {} — не в кеші", h.nreg);
            }
            continue;
        }
        let c = c.as_ref().expect("client built above for a real run");
        match c.show_text(&h.nreg) {
            Ok(text) => {
                // Same cache-refresh pattern as `text`: current edition, best-effort write.
                let _ = rada_db::textcache::put(&conn, &h.nreg, "", &text);
                fetched += 1;
                if !json {
                    println!("  [{i}/{n}] {} — {} симв.", h.nreg, text.chars().count());
                }
            }
            Err(e) => {
                // One bad document must not abort the run: muted stderr line, continue.
                failed += 1;
                eprintln!("  [{i}/{n}] {} — помилка: {e}", h.nreg);
            }
        }
    }
    let (rows, _bytes) = rada_db::textcache::stats(&conn).unwrap_or((0, 0));
    if json {
        let out = serde_json::json!({
            "fetched": fetched,
            "cached_already": cached_already,
            "failed": failed,
            "rows": rows,
        });
        println!("{}", serde_json::to_string_pretty(&out).map_err(anyhow::Error::from)?);
        return Ok(());
    }
    let summary = backfill_summary(fetched, cached_already, failed, rows);
    if dry_run {
        println!("{summary} (сухий прогін)");
    } else {
        println!("{summary}");
    }
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
/// persists the feed regardless of `--out` (after the diff, before returning)
/// and regardless of `--since` (the snapshot always stores the full feed).
/// With `--since N` only the docs whose derived date falls within the last
/// N days (today inclusive) or that have no derivable date are rendered;
/// the diff, `--json` and `--out` all honor the filter.
fn cmd_arrivals(
    db: &PathBuf,
    save: bool,
    json: bool,
    limit: usize,
    since: Option<u32>,
    out: Option<&Path>,
    pace: u64,
) -> Result<()> {
    let c = client(pace);
    let full_feed = c
        .updated_docs()
        .with_context(|| "failed to fetch the updated-documents feed (r.tsv)")?;

    // `--since`: filter AFTER the fetch, BEFORE the diff / rendering /
    // serialization, so the table, `--json` and `--out` all honor it.
    // `--save` still persists the FULL feed (the snapshot is the diff state).
    let feed = match since {
        Some(days) => {
            let today = today_yyyymmdd();
            let pairs: Vec<(&str, &str)> = full_feed
                .iter()
                .map(|d| (d.link.as_str(), d.publics.as_str()))
                .collect();
            let kept = filter_since(&pairs, days, &today);
            eprintln!("{}", since_note(full_feed.len(), kept.len(), days));
            kept.into_iter().map(|i| full_feed[i].clone()).collect()
        }
        None => full_feed.clone(),
    };

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
        rada_db::sync::save_arrivals(&conn, &full_feed)?;
        eprintln!(
            "[arrivals] збережено {} документів — наступний запуск порівняє з цим фідом",
            full_feed.len()
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

/// Derive the YYYYMMDD edition date of an arrivals row the way the web UI
/// does: the 8 digits right after `/ed` in `link` win; otherwise the first
/// well-formed `YYYYMMDD` token in the publication text `publics` (a token
/// not glued to other digits, e.g. `від 14.11.2024` or `20241115`); `None`
/// when neither carries a plausible calendar date.
fn arrivals_date_key(link: &str, publics: &str) -> Option<String> {
    fn valid(d: &str) -> bool {
        let b = d.as_bytes();
        b.len() == 8
            && b.iter().all(|c| c.is_ascii_digit())
            && ymd_to_days(d).is_some()
    }
    // link: the last `/edYYYYMMDD` marker (the edition segment of the URL).
    if let Some(pos) = link.rfind("/ed") {
        let cand = &link[pos + 3..];
        if valid(&cand[..cand.len().min(8)]) {
            return Some(cand[..8].to_string());
        }
    }
    // publics fallback: first standalone 8-digit run forming a real date.
    let bytes = publics.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i - start == 8 && valid(&publics[start..i]) {
                return Some(publics[start..i].to_string());
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Indices of `docs` (as `(link, publics)` pairs) whose derived date is
/// within the last `days` days counted back from `today` (YYYYMMDD, today
/// inclusive); rows with no derivable date are KEPT (better too many than
/// silently dropped). Pure — the caller maps feed rows to pairs so the
/// filter is testable without the network.
fn filter_since(docs: &[(&str, &str)], days: u32, today: &str) -> Vec<usize> {
    let today_days = ymd_to_days(today);
    docs.iter()
        .enumerate()
        .filter(|(_, (link, publics))| match arrivals_date_key(link, publics) {
            None => true,
            Some(key) => match (&today_days, ymd_to_days(&key)) {
                (Some(t), Some(d)) => t.saturating_sub(d) <= days as i64,
                // unparseable today: keep everything rather than drop docs
                _ => true,
            },
        })
        .map(|(i, _)| i)
        .collect()
}

/// The muted stderr line printed after `arrivals --since` has filtered the
/// feed: kept / total / window.
fn since_note(total: usize, kept: usize, days: u32) -> String {
    format!("[arrivals] {kept} з {total} за останні {days} дн.")
}

/// Today's date as `YYYYMMDD` (local time via `SystemTime` + days-since-epoch
/// math — this crate has no chrono dependency).
fn today_yyyymmdd() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    days_to_ymd(secs.div_euclid(86_400))
}

/// Proleptic-Gregorian `YYYYMMDD` → days since 1970-01-01 (Howard Hinnant's
/// `days_from_civil`); `None` for non-8-digit input or an impossible
/// calendar date (bad month, out-of-range day, non-leap Feb 29).
fn ymd_to_days(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() != 8 || !b.iter().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let y: i64 = s[0..4].parse().ok()?;
    let m: i64 = s[4..6].parse().ok()?;
    let d: i64 = s[6..8].parse().ok()?;
    if !(1..=12).contains(&m) || d < 1 {
        return None;
    }
    let days_in_month = |y: i64, m: i64| -> i64 {
        match m {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            _ => {
                if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                    29
                } else {
                    28
                }
            }
        }
    };
    if d > days_in_month(y, m) {
        return None;
    }
    let a = (14 - m) / 12;
    let yy = y + 4800 - a;
    let mm = m + 12 * a - 3;
    Some(
        d - 1
            + 365 * yy
            + yy / 4
            - yy / 100
            + yy / 400
            + (367 * mm - 719) / 12
            - 32075,
    )
}

/// Days since 1970-01-01 → `YYYYMMDD` (inverse of [`ymd_to_days`]).
fn days_to_ymd(z: i64) -> String {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}{m:02}{d:02}")
}

/// Name → nreg resolver. Offline-first: the local index is consulted before
/// any network call (exact `nreg` lookup for nreg-shaped queries, then the
/// FTS5/LIKE title search). One or more local candidates mean the network is
/// never touched — a muted `[resolve] локально` line marks the offline path.
/// Only when the index stays silent does the live `find/a` fallback run,
/// with its original behaviour (print the nreg / error).
fn cmd_resolve(
    db: &PathBuf,
    text: &str,
    limit: usize,
    json: bool,
    out: Option<&Path>,
    pace: u64,
) -> Result<()> {
    let limit = limit.clamp(1, 50);
    let hits = local_candidates(db, text, limit);
    if !hits.is_empty() {
        eprintln!("[resolve] локально");
        let hits = &hits[..hits.len().min(limit)];
        if json {
            let body = serde_json::to_string_pretty(&serde_json::json!({ "candidates": hits }))
                .map_err(anyhow::Error::from)?;
            match out {
                Some(p) => {
                    write_out_file(p, format!("{body}\n").as_bytes())?;
                    eprintln!("[resolve] записано JSON → {}", p.display());
                }
                None => println!("{body}"),
            }
            return Ok(());
        }
        let body = resolve_table(hits);
        match out {
            Some(p) => {
                // UTF-8 BOM prefix: the same Excel-friendly convention as arrivals.
                write_out_file(p, format!("\u{FEFF}{body}\n").as_bytes())?;
                eprintln!("{}", resolve_out_note(hits.len(), p));
            }
            None => {
                // Unambiguous local hit keeps the historical stdout shape: a
                // bare nreg line, byte-for-byte as `find/a` printed it.
                if hits.len() == 1 {
                    println!("{}", hits[0].nreg);
                } else {
                    println!("{body}");
                }
            }
        }
        return Ok(());
    }
    // Live fallback (unchanged): only reached when the local index is empty.
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

/// Candidate rows from the local index: exact `nreg` hit for nreg-shaped
/// queries (`254к/96-вр`, `984_006-06`, …), otherwise the FTS5/LIKE title
/// search ranked by relevance. A missing index or a failed query is not an
/// error — it simply means "no local candidates" and the caller goes live.
fn local_candidates(db: &Path, text: &str, limit: usize) -> Vec<rada_db::SearchHit> {
    let attempt = rada_db::open(db).and_then(|conn| {
        if rada_db::looks_like_nreg(text) {
            if let Some(hit) = rada_db::by_nreg(&conn, text)? {
                return Ok(vec![hit]);
            }
        }
        let q = rada_db::SearchQuery {
            nazva: Some(text.to_string()),
            sort: rada_db::search::Sort::Relevance,
            limit,
            offset: 0,
            ..Default::default()
        };
        Ok(rada_db::search::search(&conn, &q)?.0)
    });
    match attempt {
        Ok(hits) => hits,
        Err(e) => {
            eprintln!("[resolve] локальний пошук не вдався ({e:#}); піду в live");
            Vec::new()
        }
    }
}

/// Candidate table: «{n} варіантів:» header, then aligned
/// `  {nreg}  {nazva}  [{status_label}]  {org_num}/{org_date}` rows
/// (search-style formatting, dates through [`fmt_date`]).
fn resolve_table(hits: &[rada_db::SearchHit]) -> String {
    let mut lines = vec![format!("{} варіантів:", hits.len())];
    for h in hits {
        let status = rada_core::models::StatusId(h.status).label();
        lines.push(format!(
            "  {:<16} {}  [{status}]  {}/{}",
            h.nreg,
            h.nazva,
            h.org_num,
            fmt_date(&h.org_date)
        ));
    }
    lines.join("\n")
}

/// Create the parent directories of `p` and write `bytes` to it; failures
/// carry the usual «запис {file}» context.
fn write_out_file(p: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = p.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create dir {}", parent.display()))?;
        }
    }
    std::fs::write(p, bytes).with_context(|| format!("запис {}", p.display()))
}

/// The stderr note printed after `resolve --out` has written the candidate
/// table (`n` variants) to `file`.
fn resolve_out_note(n: usize, file: &Path) -> String {
    format!("[resolve] записано {n} варіантів → {}", file.display())
}

/// List «related documents» for `nreg`: links parsed from the live portal
/// page (one paced request), titles preferred from the offline index.
/// With `--out` the exact stdout body goes to the file (parent dirs created)
/// and nothing prints to stdout: the plain table is written UTF-8 with a BOM
/// (Excel-friendly, like arrivals/resolve), the `--json` body BOM-less.
fn cmd_related(
    db: &PathBuf,
    nreg: &str,
    json: bool,
    out: Option<&Path>,
    pace: u64,
) -> Result<()> {
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
        let body = serde_json::to_string_pretty(&serde_json::json!({ "items": items }))
            .map_err(anyhow::Error::from)?;
        match out {
            Some(p) => {
                // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                write_out_file(p, format!("{body}\n").as_bytes())?;
                eprintln!("[related] записано JSON → {}", p.display());
            }
            None => println!("{body}"),
        }
        return Ok(());
    }
    let body = related_table(&list);
    match out {
        Some(p) => {
            // UTF-8 BOM prefix: the same Excel-friendly convention as arrivals.
            write_out_file(p, format!("\u{FEFF}{body}\n").as_bytes())?;
            eprintln!("{}", related_out_note(list.len(), p));
        }
        None => println!("{body}"),
    }
    Ok(())
}

/// The plain-table body of `related` (shared by stdout and `--out`):
/// «{n} пов'язаних документів:» header plus `  {nreg}  {title}` rows, or
/// «посилань на інші акти не знайдено» when the list is empty.
fn related_table(list: &[rada_core::related::RelatedDoc]) -> String {
    if list.is_empty() {
        return "посилань на інші акти не знайдено".to_string();
    }
    let mut lines = Vec::with_capacity(list.len() + 1);
    lines.push(format!("{} пов'язаних документів:", list.len()));
    for r in list {
        lines.push(format!("  {}  {}", r.nreg, r.title));
    }
    lines.join("\n")
}

/// The stderr note printed after `related --out` has written the plain
/// table (`n` links) to `file`.
fn related_out_note(n: usize, file: &Path) -> String {
    format!("[related] записано {n} посилань → {}", file.display())
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
        AnnCmd::List { nreg, json, out } => {
            let out = out.as_deref();
            let items = match &nreg {
                Some(n) => rada_db::annotations::list(&conn, n)?, // already newest first
                None => {
                    let mut v = rada_db::annotations::all(&conn)?;
                    v.sort_by(|a, b| b.id.cmp(&a.id));
                    v
                }
            };
            if json {
                let body = serde_json::to_string_pretty(&items)?;
                match out {
                    Some(p) => {
                        // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                        write_out_file(p, format!("{body}\n").as_bytes())?;
                        eprintln!("[annot] записано JSON → {}", p.display());
                    }
                    None => println!("{body}"),
                }
                return Ok(());
            }
            let lines = annot_table_lines(&items);
            match out {
                Some(p) => {
                    // UTF-8 BOM prefix: the same Excel-friendly convention as
                    // search/arrivals/resolve/related; the bytes are exactly
                    // the stdout table, so an empty result (no rows) carries
                    // no trailing newline either — just the BOM.
                    let text = if lines.is_empty() {
                        "\u{FEFF}".to_string()
                    } else {
                        format!("\u{FEFF}{}\n", lines.join("\n"))
                    };
                    write_out_file(p, text.as_bytes())?;
                    eprintln!("{}", annot_list_out_note(items.len(), p));
                }
                None => {
                    if lines.is_empty() {
                        // stdout-only «no annotations» message (the
                        // `search --facets` rule): it never enters the file.
                        println!("немає анотацій");
                    }
                    for line in lines {
                        println!("{line}");
                    }
                }
            }
        }
        AnnCmd::Add { nreg, kind, text, text_file, start, end } => {
            // Exactly one of the positional text / `--text-file` may supply
            // the text; the no-flag case keeps the historical empty-text
            // behaviour (bookmarks need none).
            let text = match (text, text_file.as_deref()) {
                (Some(_), Some(_)) => {
                    return Err(anyhow::anyhow!("вкажіть текст або --text-file, не обидва"))
                }
                (None, Some(p)) => {
                    let raw = std::fs::read_to_string(p)
                        .with_context(|| format!("читання {}", p.display()))?;
                    let text = raw.trim();
                    if text.is_empty() {
                        return Err(anyhow::anyhow!("файл тексту порожній"));
                    }
                    eprintln!("{}", annot_text_file_note(text.chars().count(), p));
                    text.to_string()
                }
                (Some(t), None) => t,
                (None, None) => String::new(),
            };
            let id = rada_db::annotations::insert(
                &conn,
                &nreg,
                &kind,
                &text,
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

/// The plain-table body of `annot list` (shared by stdout and `--out`):
/// one `#{id:<4} {kind:<9} {nreg:<12} {created_at}  {span}{text}` row per
/// annotation — span ` [{start}..{end}]` only when both ends are present,
/// text truncated to 60 chars. The stdout-only «немає анотацій» message is
/// not part of this body: an empty result renders as no lines, so the
/// `--out` file carries the BOM alone (the established `search --out` rule).
fn annot_table_lines(items: &[rada_db::Annotation]) -> Vec<String> {
    items
        .iter()
        .map(|a| {
            let span = match (a.span_start, a.span_end) {
                (Some(s), Some(e)) => format!(" [{s}..{e}]"),
                _ => String::new(),
            };
            let t: String = a.text.chars().take(60).collect();
            format!(
                "#{:<4} {:<9} {:<12} {}  {}{}",
                a.id, a.kind, a.nreg, a.created_at, span, t
            )
        })
        .collect()
}

/// The stderr note printed after `annot list --out` has written the plain
/// table (`n` annotations) to `file`.
fn annot_list_out_note(n: usize, file: &Path) -> String {
    format!("[annot] записано {n} анотацій → {}", file.display())
}

/// The stderr note printed after `annot export --out` has written `file`.
fn annot_out_note(n: usize, file: &Path) -> String {
    format!("[annot] записано {n} анотацій → {}", file.display())
}

/// The muted stderr note printed when `annot add --text-file` supplies the
/// text from `file` (`chars` characters after trimming).
fn annot_text_file_note(chars: usize, file: &Path) -> String {
    format!("[annot] текст {chars} симв. з {}", file.display())
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
        annot_list_out_note, annot_out_note, annot_text_file_note, arrivals_date_key,
        arrivals_out_note, backfill_summary, cache_search_out_note, card_out_note, dict_fresh_lines,
        dict_matches, dict_out_note, export_html,
        export_markdown, export_out_note, export_plain, facet_lines, filter_since, new_nregs,
        normalize_fmt, out_note, related_out_note, resolve_out_note, search_out_note, since_note,
        stats_lines, today_yyyymmdd, StatsReport,
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
    fn card_out_note_formats_nreg_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//c.json");
        let line = card_out_note("2755-17", file);
        assert_eq!(line, format!("[card] 2755-17 → {}", file.display()));
        assert!(line.starts_with("[card] "));
        assert!(line.contains("2755-17"));
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
    fn annot_list_out_note_formats_count_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//a.txt");
        let line = annot_list_out_note(5, file);
        assert!(line.starts_with("[annot] записано "));
        assert!(line.contains("5"));
        assert!(line.contains("анотацій"));
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
    fn annot_text_file_note_formats_chars_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//t.txt");
        let line = annot_text_file_note(4242, file);
        assert!(line.starts_with("[annot] текст "));
        assert!(line.contains("4242"));
        assert!(line.contains("симв."));
        assert!(line.contains("з "));
        assert!(line.contains(&file.display().to_string()));
    }

    #[test]
    fn resolve_out_note_formats_count_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//r.txt");
        let line = resolve_out_note(3, file);
        assert!(line.starts_with("[resolve] записано "));
        assert!(line.contains("3"));
        assert!(line.contains("варіантів"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
    }

    #[test]
    fn related_out_note_formats_count_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//r.txt");
        let line = related_out_note(9, file);
        assert!(line.starts_with("[related] записано "));
        assert!(line.contains("9"));
        assert!(line.contains("посилань"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
    }

    #[test]
    fn search_out_note_formats_count_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//s.txt");
        let line = search_out_note(12, file);
        assert!(line.starts_with("[search] записано "));
        assert!(line.contains("12"));
        assert!(line.contains("результатів"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
    }

    #[test]
    fn cache_search_out_note_formats_count_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//c.txt");
        let line = cache_search_out_note(8, file);
        assert!(line.starts_with("[cache] записано "));
        assert!(line.contains("8"));
        assert!(line.contains("збігів"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
    }

    #[test]
    fn dict_out_note_formats_kind_count_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//d.txt");
        let line = dict_out_note("stan", 10, file);
        assert!(line.starts_with("[dict] stan: записано "));
        assert!(line.contains("10"));
        assert!(line.contains("записів"));
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
    fn export_markdown_plain_headers() {
        // Markdown: exact header lines, blank line, body + trailing newline.
        let md = export_markdown("Податковий кодекс", "2755-17", "Стаття 1.\nТекст");
        let mut lines = md.lines();
        assert_eq!(lines.next().unwrap(), "# Податковий кодекс");
        assert_eq!(
            lines.next().unwrap(),
            format!(
                "> 2755-17 — rada-client {}",
                env!("CARGO_PKG_VERSION")
            )
        );
        assert_eq!(lines.next().unwrap(), "");
        assert_eq!(lines.next().unwrap(), "Стаття 1.");
        assert_eq!(lines.next().unwrap(), "Текст");
        assert_eq!(lines.next(), None);
        assert!(md.ends_with('\n'), "body carries a trailing newline");
        // Plain: title (nreg), 60 '=' rule, blank, body + trailing newline.
        let txt = export_plain("Податковий кодекс", "2755-17", "Стаття 1.\nТекст");
        let mut lines = txt.lines();
        assert_eq!(lines.next().unwrap(), "Податковий кодекс (2755-17)");
        assert_eq!(lines.next().unwrap(), "=".repeat(60));
        assert_eq!(lines.next().unwrap(), "");
        assert_eq!(lines.next().unwrap(), "Стаття 1.");
        assert_eq!(lines.next().unwrap(), "Текст");
        assert_eq!(lines.next(), None);
        assert!(txt.ends_with('\n'), "body carries a trailing newline");
    }

    #[test]
    fn export_html_escapes_and_wraps_paragraphs() {
        let html = export_html(
            "Акт <A> & \"B\"",
            "1-1<b>",
            "Перший & абзац.\nДругий рядок.\n\nДругий <абзац> \"лапки\".",
        );
        // standalone page skeleton
        assert!(html.starts_with("<!doctype html>\n"), "{html}");
        assert!(html.contains("<meta charset=\"utf-8\">"));
        // title escaped in <title> and <h1>, raw angle brackets gone
        let esc_title = "Акт &lt;A&gt; &amp; &quot;B&quot;";
        assert!(html.contains(&format!("<title>{esc_title}</title>")));
        assert!(html.contains(&format!("<h1>{esc_title}</h1>")));
        // meta line carries the escaped nreg
        assert!(html.contains("<p class=\"meta\">1-1&lt;b&gt;"));
        assert!(html.contains(&format!(
            "· rada-client {}</p>",
            env!("CARGO_PKG_VERSION")
        )));
        // blank-line split: two escaped paragraphs, internal newline kept
        assert!(html.contains("<p>Перший &amp; абзац.\nДругий рядок.</p>"));
        assert!(html.contains("<p>Другий &lt;абзац&gt; &quot;лапки&quot;.</p>"));
        assert!(!html.contains("<абзац>"), "body angle brackets are escaped");
        // no paragraph for blank-line runs at the edges
        let only_blank = export_html("T", "n", "\n \n\n");
        assert!(!only_blank.contains("<p>"));
    }

    #[test]
    fn export_out_note_formats_fmt_nreg_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//e.md");
        let line = export_out_note("md", "2755-17", file);
        assert_eq!(
            line,
            format!("[export] 2755-17 (md) → {}", file.display())
        );
        assert!(line.starts_with("[export] "));
        assert!(line.contains("(md)"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
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

    #[test]
    fn dict_fresh_lines_renders_counts_empty_and_order() {
        // non-zero renders the count, zero renders «порожньо», input order is
        // preserved even when it is not the default stan/typs/organs order
        assert_eq!(
            dict_fresh_lines(&[("organs", 4213), ("stan", 8), ("typs", 0)]),
            vec![
                "  organs: 4213 записів".to_string(),
                "  stan: 8 записів".to_string(),
                "  typs: порожньо".to_string(),
            ]
        );
        // every kind empty ⇒ every line «порожньо»
        assert_eq!(
            dict_fresh_lines(&[("stan", 0), ("typs", 0), ("organs", 0)]),
            vec![
                "  stan: порожньо".to_string(),
                "  typs: порожньо".to_string(),
                "  organs: порожньо".to_string(),
            ]
        );
        // empty slice ⇒ empty vec
        assert!(dict_fresh_lines(&[]).is_empty());
    }

    #[test]
    fn backfill_summary_renders_counts_and_cache_total() {
        // all-nonzero render: exact wording, comma/semicolon separators
        assert_eq!(
            backfill_summary(3, 5, 1, 1234),
            "отримано 3, вже в кеші 5, помилок 1; кеш тепер: 1234 тіл"
        );
        // zeros render the zeros (never omitted)
        assert_eq!(
            backfill_summary(0, 0, 0, 0),
            "отримано 0, вже в кеші 0, помилок 0; кеш тепер: 0 тіл"
        );
        // wording check: the four count labels and the cache tail are present
        let line = backfill_summary(2, 1, 0, 42);
        assert!(line.starts_with("отримано 2"));
        assert!(line.contains("вже в кеші 1"));
        assert!(line.contains("помилок 0"));
        assert!(line.contains("; кеш тепер: 42 тіл"));
    }

    #[test]
    fn arrivals_date_key_prefers_link_then_publics() {
        // the /edYYYYMMDD in the link wins over any date in publics
        assert_eq!(
            arrivals_date_key(
                "https://zakon.rada.gov.ua/laws/show/1178-2022-%D0%BF/ed20241115#Text",
                "Відомості Верховної Ради (ВРУ), 2022, № 30, ст.180 від 20220701"
            )
            .as_deref(),
            Some("20241115")
        );
        // no /ed marker ⇒ the first plausible 8-digit run in publics wins
        assert_eq!(
            arrivals_date_key(
                "https://zakon.rada.gov.ua/go/1178-2022-п",
                "Урядовий кур'єр, 2024, № 211; 20241115 опубліковано"
            )
            .as_deref(),
            Some("20241115")
        );
        // an /ed marker with garbage does NOT shadow a good publics date
        assert_eq!(
            arrivals_date_key("https://zakon.rada.gov.ua/go/x/edabcd", "20231231").as_deref(),
            Some("20231231")
        );
        // neither side carries a date ⇒ None
        assert_eq!(
            arrivals_date_key("https://zakon.rada.gov.ua/go/1178-2022-п", "без дати").as_deref(),
            None
        );
        // digit runs that are not 8 long or not a real calendar date are no dates
        assert_eq!(arrivals_date_key("", "2024 № 30, ст.180").as_deref(), None);
        assert_eq!(arrivals_date_key("", "123456789").as_deref(), None);
        assert_eq!(arrivals_date_key("", "20230230").as_deref(), None); // Feb 30
        assert_eq!(arrivals_date_key("", "13.45.6789").as_deref(), None);
    }

    #[test]
    fn filter_since_keeps_recent_and_undated() {
        let today = "20261028";
        let docs: Vec<(&str, &str)> = vec![
            ("/ed20261028", ""),           // 0: today — kept
            ("/ed20261025", ""),           // 1: boundary day (3 days back) — kept
            ("/ed20261024", ""),           // 2: one day older — dropped
            ("", "опубліковано 20261026"), // 3: publics date, inside — kept
            ("", "опубліковано 20200101"), // 4: publics date, old — dropped
            ("no-date-here", "нічого"),    // 5: undated — kept
        ];
        assert_eq!(filter_since(&docs, 3, today), vec![0, 1, 3, 5]);
        // days = 0 keeps only today plus undated
        assert_eq!(filter_since(&docs, 0, today), vec![0, 5]);
        // a wide window keeps everything, feed order preserved
        assert_eq!(filter_since(&docs, 3650, today), vec![0, 1, 2, 3, 4, 5]);
        // empty input ⇒ empty output
        assert!(filter_since(&[], 30, today).is_empty());
        // the note the CLI prints after filtering (kept / total / window)
        assert_eq!(
            since_note(6, 4, 3),
            "[arrivals] 4 з 6 за останні 3 дн."
        );
        // today's-date helper: a real YYYYMMDD the filter itself accepts
        let t = today_yyyymmdd();
        assert_eq!(t.len(), 8);
        assert!(t.chars().all(|c| c.is_ascii_digit()));
        assert!(super::ymd_to_days(&t).is_some());
    }
}
