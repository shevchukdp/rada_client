//! `rada-client` — CLI for the local Ukrainian-legislation index.
//!
//! Subcommands:
//! - `init`      build the full index from the official `doc.txt` dump;
//! - `sync`      incremental update from the updated-documents feed;
//! - `search`    local title search (FTS5 + filters);
//! - `card`      live card for a document (human summary, `--json` for raw JSON);
//! - `text`      clean text of a document (current or a given edition;
//!               `--grep TERM` greps the body with `--context N` windows);
//! - `backfill`  fetch and cache the act texts of the top hits of a local
//!               title search, so later reading works offline
//!               (`--dry-run` lists cached/missing without fetching;
//!               `--out` writes the final summary report, the progress
//!               lines stay on stdout);
//! - `dict`      load reference dictionaries (stan/typs/organs) from the live API,
//!               seed one offline from a file (`--from-file`) or report
//!               local freshness (`--fresh`);
//! - `dict-stats` offline dictionary coverage report: how populated each
//!               reference dictionary is, one line per kind
//!               (`--json` for machine-readable output, `--out` to write
//!               the report; informational — never exits nonzero);
//! - `dicts`     offline browse/search of ONE reference dictionary
//!               (stan / typs / organs) straight from the local `dict`
//!               table -- the CLI twin of the desktop home dictionary
//!               search panel (`--query` keeps labels containing a
//!               case-insensitive substring, `--json` for machine-readable
//!               output, `--out` to write the report; never touches the
//!               network -- the live loaders stay `dict` / `sync`);
//! - `arrivals`  live updated-documents feed with a diff against the saved snapshot;
//! - `snapshot`  create / restore gzip snapshots;
//! - `status`    index statistics;
//! - `stats`     offline report on the local index (file, documents,
//!               annotations, dicts, text cache; `--json` for
//!               machine-readable output, `--out` to write the report);
//! - `verify`    offline database health check (SQLite integrity,
//!               foreign keys, documents, FTS consistency, duplicate
//!               nregs, text cache; `--json` for machine-readable
//!               output, `--out` to write the report, exit 1 on
//!               any failed check);
//! - `dates`    offline document-date histogram by year of `org_date` —
//!               the same adoption-date column the
//!               `search --date-from/--date-to` filters use
//!               (`--json` for machine-readable output, `--out` to write
//!               the report; informational — never exits nonzero);
//! - `months`   offline month-by-month document-date histogram of
//!               `org_date` for one year (default: the newest one in the
//!               index) — the month-level companion of `dates`, which
//!               stops at year granularity
//!               (`--year YYYY`, `--json` for machine-readable output,
//!               `--out` to write the report; informational — never
//!               exits nonzero);
//! - `orgs`      offline top-publishers histogram on `org_num` — which
//!               «органи» issued the most documents, labels from the
//!               offline `organs` dictionary
//!               (`--year РРРР` restricts the count to documents whose
//!               `org_date` carries that year, `--json` for
//!               machine-readable output, `--out` to write the report;
//!               informational — exits 1 only on a malformed `--year`);
//! - `docs`      offline document list on the TEXT `org_num` / `org_date`
//!               columns of `docs` — the drill-down companion of `orgs`:
//!               the documents one publisher issued and/or one year
//!               holds, newest first
//!               (`--org` — the same publisher key `orgs` prints — and
//!               `--year РРРР` are both optional but at least one is
//!               required, `--json` for machine-readable output, `--out`
//!               to write the report; informational — exits 1 only on a
//!               missing index, neither filter given or a malformed
//!               `--year`);
//! - `annots`  offline per-document annotation list — the drill-down
//!               companion of `recent-annots`: the annotations of one
//!               document, newest first (`NREG` required, `--kind` keeps
//!               one kind — коментар | виділення | закладка, `--json` for
//!               machine-readable output, `--out` to write the report;
//!               informational — exits 1 only on a missing index or an
//!               unknown `--kind`);
//! - `statuses`  offline document-status histogram on `status` — how many
//!               documents are чинний / не чинний / …, labels from the
//!               offline `stan` dictionary
//!               (`--json` for machine-readable output, `--out` to write
//!               the report; informational — never exits nonzero);
//! - `export`    offline export of a cached document body as md / txt / html
//!               (text cache only — never touches the network);
//! - `toc`       offline outline (зміст) of a cached document: «Розділ/Глава»
//!               and «Стаття» headings from the text cache (`--json` for
//!               machine-readable output, `--out` to write the table);
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
    /// Offline one-shot export of the search result LIST (the CLI mirror
    /// of the GUI search «експорт списку»): the hits of the local FTS
    /// title query rendered as a numbered document list in plain text or
    /// markdown. Pure offline: the same `rada_db::search::search` path as
    /// `search`, never builds a client and never touches the network.
    /// No `--json`: the `--format txt|md` document axis is the point.
    Hits {
        /// Title term (multi-word → FTS5), same as `search`.
        term: Vec<String>,
        /// Max rows (default 50, clamped to 1..=500).
        #[arg(long, default_value_t = 50)]
        limit: usize,
        /// Output format: txt (default) | md (aliases `text` /
        /// `markdown`, case-insensitive).
        #[arg(long = "format", alias = "fmt", default_value = "txt")]
        fmt: String,
        /// Write to this file instead of stdout (parent dirs created).
        /// Human document: always written UTF-8 WITH a BOM; stdout output
        /// is the same bytes without the BOM. Zero hits write the BOM
        /// alone — the «нічого не знайдено» line stays on stdout (the
        /// house `search --out` zero-hit rule); the note goes to stderr.
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
        /// `--json` the file is written BOM-less. The live request runs
        /// first: a failed card fetch is a hard error and writes no file.
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
        /// The full body is written verbatim (no BOM, no added trailing
        /// newline — the exact cached text, unlike the BOM-prefixed
        /// `export --out` document); with `--grep` the file receives the
        /// grepped output (BOM-prefixed table) instead of the full body.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Grep the body: print only lines containing this substring
        /// (case-insensitive), each prefixed «N: line» with its 1-based line
        /// number; «збігів немає» on stdout when nothing matches. An empty
        /// string prints the whole body with a muted stderr warning.
        #[arg(long)]
        grep: Option<String>,
        /// With `--grep`: also print N context lines before/after each
        /// match (disjoint blocks separated by «--», like `grep -C`).
        #[arg(long, default_value_t = 0)]
        context: usize,
    },
    /// Offline text cache: statistics, purge (`--purge [nreg]`), a
    /// full-text scan of cached bodies (`--search TERM`) — offline, Unicode
    /// case-insensitive, ranked by occurrence count — or a bulk export of
    /// every cached body into a directory of plain-text files
    /// (`--dump DIR`, offline).
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
        /// Write the exact stdout body to this file instead of stdout
        /// (parent dirs created). With `--search`: the table (or the
        /// `--json` body) — the plain table gets a UTF-8 BOM
        /// (Excel-friendly), with `--json` the file is written BOM-less;
        /// zero hits write the BOM alone and the «збігів … немає» message
        /// stays on stdout. In the bare stats form: the one-line report
        /// with a UTF-8 BOM (it always renders, so never BOM-alone).
        /// `--purge` and `--dump` ignore `--out`.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Export every cached document body into this directory as a
        /// UTF-8 (WITH BOM) plain-text file: `{sanitized-nreg}.txt` for the
        /// current edition, `{sanitized-nreg}.{edition}.txt` for dated ones
        /// (body verbatim + trailing newline; parent dirs created).
        /// Offline; stdout prints only the final summary line and `--out`
        /// is not supported in this mode.
        #[arg(long)]
        dump: Option<PathBuf>,
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
        /// Write the final summary report (or the `--json` body) to this file
        /// instead of stdout (parent dirs created). Plain report gets a
        /// UTF-8 BOM (Excel-friendly); with `--json` the file is written
        /// BOM-less. The per-document progress lines are progress, not body:
        /// they stay on stdout even with `--out` (only the summary moves to
        /// the file). On «нічого не знайдено» the file receives the BOM
        /// alone and the report line stays on stdout (the house `search
        /// --out` zero-hit rule); the note goes to stderr.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Offline report on the local index: file, documents, annotations,
    /// dictionaries, text cache, last import (`--json` for machine-readable
    /// output). Pure offline: the command never touches the network.
    Stats {
        /// Emit JSON instead of the aligned report.
        #[arg(long)]
        json: bool,
        /// Write the exact stdout report (or the `--json` body) to this file
        /// instead of stdout (parent dirs created). Plain report gets a
        /// UTF-8 BOM (Excel-friendly); with `--json` the file is written
        /// BOM-less.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Offline database health check (no network, no client): SQLite
    /// integrity, foreign keys, document count, FTS consistency,
    /// duplicate nregs and the text cache. One line per check plus a
    /// verdict; exits 1 when any check fails (warnings alone stay
    /// exit 0).
    Verify {
        /// Emit JSON instead of the aligned check lines.
        #[arg(long)]
        json: bool,
        /// Write the exact stdout report (or the `--json` body) to this file
        /// instead of stdout (parent dirs created). Plain report gets a
        /// UTF-8 BOM (Excel-friendly); with `--json` the file is written
        /// BOM-less.
        #[arg(long)]
        out: Option<PathBuf>,
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
    /// Offline dictionary coverage report: how populated each reference
    /// dictionary is (stan / typs / organs), one line per kind with its
    /// row count — complements `verify` / `stats`. Pure offline: the
    /// command never touches the network and never exits nonzero on empty
    /// dictionaries (it is informational).
    DictStats {
        /// Emit JSON (`{"dicts":[{kind,label,rows},…],"total":N}`) instead
        /// of the aligned report.
        #[arg(long)]
        json: bool,
        /// Write the exact stdout report (or the `--json` body) to this file
        /// instead of stdout (parent dirs created). Plain report gets a
        /// UTF-8 BOM (Excel-friendly); with `--json` the file is written
        /// BOM-less.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Offline browse/search of ONE reference dictionary from the local
    /// `dict` table: the CLI twin of the desktop home dictionary-search
    /// panel. KIND is `stan`, `typs` or `organs` (case-insensitive; an
    /// unknown kind exits 1). `--query TEXT` keeps only labels containing
    /// TEXT as a case-insensitive substring (Ukrainian letters folded too).
    /// Rows print as `{id}  {label}` ordered by id, capped by `--limit`
    /// (default 100, clamped to 1..=1000). Pure offline: the command never
    /// touches the network (the live loaders stay `dict` / `sync`); zero
    /// matches is not an error (the empty report exits 0); only a missing
    /// index or an unknown KIND exits 1.
    Dicts {
        /// Dictionary kind: stan | typs | organs (required).
        kind: String,
        /// Keep only labels containing this text (case-insensitive).
        #[arg(long)]
        query: Option<String>,
        /// Max rows to print (default 100, clamped to 1..=1000).
        #[arg(long, default_value_t = 100)]
        limit: usize,
        /// Emit the JSON body
        /// (`{"count":N,"kind":...,"query":...,"rows":[{id,label},...]}`
        /// -- pretty, keys alphabetical, `count` counts the listed rows)
        /// instead of the report.
        #[arg(long)]
        json: bool,
        /// Write the exact stdout report (or the `--json` body) to this file
        /// instead of stdout (parent dirs created). Plain report gets a
        /// UTF-8 BOM (Excel-friendly); with `--json` the file is written
        /// BOM-less; the `[dicts] записано ...` note goes to stderr.
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
        /// file is written BOM-less. Zero links write only the BOM — the
        /// «не знайдено» line stays on stdout (the house `search --out`
        /// zero-hit rule); the note goes to stderr.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Snapshot helpers.
    Snapshot {
        #[command(subcommand)]
        what: SnapCmd,
    },
    /// Print index statistics.
    Status {
        /// Write the exact stdout report to this file instead of stdout
        /// (parent dirs created). The report gets a UTF-8 BOM
        /// (Excel-friendly, like `stats --out`) and the
        /// `[status] записано → {file}` note goes to stderr. No `--json`
        /// here: use `stats --json` for machine-readable output.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// User annotations (bookmarks / highlights / comments) stored in the index.
    Annot {
        #[command(subcommand)]
        what: AnnCmd,
    },
    /// Offline «what did I highlight lately»: the most recently saved
    /// annotations across all documents (`created_at` descending — the GUI
    /// favorites «новіші» sort). Pure offline: the command never touches
    /// the network. (Formerly spelled `recent`; renamed `recent-annots`
    /// when the freshest-documents-by-adoption-date report took the
    /// shorter `recent` name.)
    RecentAnnots {
        /// Max annotations to print (default 20, clamped to 1..=500).
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// Emit the JSON array (`[{nreg,kind,created_at,text,title},…]`)
        /// instead of the table.
        #[arg(long)]
        json: bool,
        /// Write the exact stdout table (or the `--json` body) to this file
        /// instead of stdout (parent dirs created). Plain table gets a
        /// UTF-8 BOM (Excel-friendly); with `--json` the file is written
        /// BOM-less. Zero annotations write the BOM alone; the «Останніх
        /// анотацій немає» line stays on stdout (the house `search --out`
        /// zero-hit rule); the note goes to stderr.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Offline «what did I annotate in this document»: the per-document
    /// annotation list — the drill-down companion of `recent-annots`
    /// (which streams the newest annotations across all documents). Rows
    /// share the `recent-annots` rendering and order (`created_at`
    /// descending, `id` descending as tie-break). Pure offline: the
    /// command never touches the network; informational — a document
    /// without annotations (or an unknown nreg) exits 0 with the empty
    /// report (header + «немає анотацій» + «разом: 0»); only a missing
    /// index or an unknown `--kind` exit 1.
    Annots {
        /// Document nreg to list (required).
        nreg: String,
        /// Keep only one kind: коментар | виділення | закладка (the
        /// English `comment` / `highlight` / `bookmark` and the plural
        /// Ukrainian forms are accepted too, case-insensitive). Omit to
        /// keep every kind.
        #[arg(long)]
        kind: Option<String>,
        /// Max annotations to print (default 100, clamped to 1..=500).
        #[arg(long, default_value_t = 100)]
        limit: usize,
        /// Emit the JSON body
        /// (`{"annots":[{created_at,kind,nreg,text,title},…],"doc":…,"total":N}`
        /// — pretty, keys alphabetical, `total` counts the listed rows)
        /// instead of the report.
        #[arg(long)]
        json: bool,
        /// Write the exact stdout report (or the `--json` body) to this file
        /// instead of stdout (parent dirs created). Plain report gets a
        /// UTF-8 BOM (Excel-friendly); with `--json` the file is written
        /// BOM-less; the `[annots] записано …` note goes to stderr.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Offline «Обрані анотації»: the full CLI mirror of the GUI/web
    /// Favorites list — every annotation across all documents, optionally
    /// filtered by kind, grouped under one header per document like the
    /// GUI grouped view. Pure offline: the command never touches the
    /// network (the chronological stream with `created_at DESC` stays the
    /// `recent-annots` command's niche).
    Favorites {
        /// Keep only one kind: comment | highlight | bookmark (Ukrainian
        /// aliases «коментар|виділення|закладка» accepted too,
        /// case-insensitive). Omit to keep every kind.
        #[arg(long)]
        kind: Option<String>,
        /// Row order: nreg (default — grouped by document, like the GUI
        /// grouped view) | title (by resolved title, then nreg) | new
        /// (`created_at` descending, flat).
        #[arg(long, default_value = "nreg")]
        sort: String,
        /// Emit the JSON body (`{"grouped":{nreg:[…]},"total":N}` for
        /// `--sort nreg`, `{"items":[…],"total":N}` otherwise) instead of
        /// the table.
        #[arg(long)]
        json: bool,
        /// Write the exact stdout table (or the `--json` body) to this file
        /// instead of stdout (parent dirs created). Plain table gets a
        /// UTF-8 BOM (Excel-friendly); with `--json` the file is written
        /// BOM-less. Zero annotations write the BOM alone; the «Обраних
        /// анотацій немає» line stays on stdout (the house `search --out`
        /// zero-hit rule); the note goes to stderr.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Offline «how fresh is my index»: document-date distribution by
    /// year of the primary date column `org_date` — the SAME adoption-date
    /// column the `search --date-from/--date-to` filters use, so the
    /// histogram and the search filters always agree. Rows whose `org_date`
    /// is missing or malformed count as «без дати». Pure offline: the
    /// command never touches the network; informational — always exits 0.
    Dates {
        /// Emit the JSON body
        /// (`{"years":[{year,count},…],"undated":N,"total":N,"newest":"YYYY"|null}`)
        /// instead of the report.
        #[arg(long)]
        json: bool,
        /// Write the exact stdout report (or the `--json` body) to this file
        /// instead of stdout (parent dirs created). Plain report gets a
        /// UTF-8 BOM (Excel-friendly); with `--json` the file is written
        /// BOM-less; the `[dates] записано …` note goes to stderr.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Offline «how is one year distributed»: month-by-month
    /// document-date histogram of `org_date` within a single year — the
    /// month-level companion of `dates` (which stops at year
    /// granularity). The year defaults to the newest one present in the
    /// index (the `dates` verdict rule); `--year YYYY` picks any. All
    /// twelve months ALWAYS render — a month without a single document
    /// shows `0` — so the report is a stable 12-line grid; only full
    /// 8-digit `org_date` values count (a row whose month or day part is
    /// malformed passes `dates`' year histogram but matches no month).
    /// Pure offline: the command never touches the network;
    /// informational — always exits 0, even when every month is empty
    /// (header + 12 zero rows + «разом: 0»).
    Months {
        /// Year to histogram: exactly 4 digits (e.g. 2026).
        /// Default: the newest year present in the index.
        #[arg(long)]
        year: Option<String>,
        /// Emit the JSON body
        /// (`{"months":[{count,month},…],"total":N,"year":"YYYY"}`)
        /// instead of the report.
        #[arg(long)]
        json: bool,
        /// Write the exact stdout report (or the `--json` body) to this file
        /// instead of stdout (parent dirs created). Plain report gets a
        /// UTF-8 BOM (Excel-friendly); with `--json` the file is written
        /// BOM-less; the `[months] записано …` note goes to stderr.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Offline «which years hold the documents»: document-date histogram by
    /// year of `org_date` over a CLOSED year range — the range-selected
    /// companion of `dates` (which always prints the whole histogram).
    /// `--from YYYY` / `--to YYYY` default to the oldest / newest year
    /// present in the index (the `dates` MIN/MAX rule). Unlike `months`
    /// the grid is NOT zero-filled: a year inside the range without a
    /// single document is simply omitted — only years the histogram has a
    /// row for render, ascending. Only rows whose four-digit year parses
    /// count; documents with a missing or malformed date report as
    /// «без дати» over the WHOLE index (the `dates` rule, independent of
    /// the range). Pure offline: the command never touches the network;
    /// informational — always exits 0, even when the range holds nothing
    /// (header + «немає документів за цей період» + «разом: 0» + the
    /// undated line). A malformed `--from`/`--to` or a `--from` later
    /// than `--to` is the only hard error (exit 1).
    Years {
        /// First year of the closed range: exactly 4 digits (e.g. 2020).
        /// Default: the oldest year present in the index.
        #[arg(long)]
        from: Option<String>,
        /// Last year of the closed range: exactly 4 digits (e.g. 2026).
        /// Default: the newest year present in the index.
        #[arg(long)]
        to: Option<String>,
        /// Emit the JSON body
        /// (`{"count_total":N,"from":"YYYY","rows":[{count,year},…],
        /// "to":"YYYY","undated":N}` — pretty, keys alphabetical) instead
        /// of the report.
        #[arg(long)]
        json: bool,
        /// Write the exact stdout report (or the `--json` body) to this file
        /// instead of stdout (parent dirs created). Plain report gets a
        /// UTF-8 BOM (Excel-friendly); with `--json` the file is written
        /// BOM-less; the `[years] записано …` note goes to stderr.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Offline «what is the freshest»: the newest documents by adoption
    /// date on the SAME primary date column `org_date` the
    /// `search --date-from/--date-to` filters use — the day-level
    /// companion of `months` / `dates`. The anchor is the freshest full
    /// 8-digit `org_date` present in the index (NEVER today's system
    /// date, so an idle index still reports its own newest documents);
    /// `--days N` (default 7, 1..=365) keeps the documents whose 8-digit
    /// `org_date` lies within the last N days ending at that anchor
    /// (inclusive), `--limit M` (default 100, 1..=500) caps the rendered
    /// rows. Rows order by `org_date` descending, `nreg` ascending. Pure
    /// offline: the command never touches the network; informational —
    /// always exits 0, even when no document qualifies (header +
    /// «немає документів за цей період» + «разом: 0»).
    Recent {
        /// Days to look back from the freshest indexed date
        /// (default 7, 1..=365).
        #[arg(long)]
        days: Option<String>,
        /// Max documents to print (default 100, 1..=500).
        #[arg(long)]
        limit: Option<String>,
        /// Emit the JSON body
        /// (`{"days":N,"docs":[{date,nreg,title},…],"from":"YYYYMMDD"|null,"to":"YYYYMMDD"|null}`)
        /// instead of the report.
        #[arg(long)]
        json: bool,
        /// Write the exact stdout report (or the `--json` body) to this file
        /// instead of stdout (parent dirs created). Plain report gets a
        /// UTF-8 BOM (Excel-friendly); with `--json` the file is written
        /// BOM-less; the `[recent] записано …` note goes to stderr.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Offline «who publishes the most»: top publishers histogram on the
    /// `org_num` column of `docs` — which «органи» issued the most
    /// documents, with labels resolved from the offline `organs`
    /// dictionary. Complements `dates` / `dict-stats` and helps pick the
    /// `search --org` filters. Pure offline: the command never touches the
    /// network; informational — always exits 0.
    Orgs {
        /// Restrict the histogram to documents whose `org_date` starts
        /// with these four digits and is a full 8-digit date (e.g.
        /// `2026`); the header becomes «Видавці документів РРРР року:»
        /// and the JSON body gains `"year":"yyyy"`. Anything else than
        /// four ASCII digits exits 1 («рік має бути РРРР (наприклад
        /// 2026)»); absent keeps the report exactly as today.
        #[arg(long)]
        year: Option<String>,
        /// Max publishers to print (default 20, clamped to 1..=200).
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// Emit the JSON body
        /// (`{"orgs":[{count,label,org_num},…],"total":N[,"year":"yyyy"]`)
        /// instead of the report.
        #[arg(long)]
        json: bool,
        /// Write the exact stdout report (or the `--json` body) to this file
        /// instead of stdout (parent dirs created). Plain report gets a
        /// UTF-8 BOM (Excel-friendly); with `--json` the file is written
        /// BOM-less; the `[orgs] записано …` note goes to stderr.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Offline «what documents are there»: the document list on the TEXT
    /// `org_num` / `org_date` columns of `docs` — the drill-down
    /// companion of `orgs` (feed an `orgs` row's `[org_num]` key into
    /// `--org`). `--org` matches `org_num` exactly; a numeric key also
    /// resolves its label from the offline `organs` dictionary and the
    /// header names the label when found («Документи видавця {label}:»),
    /// the raw key otherwise («Документи видавця {org}:»). `--org` and
    /// `--year` are both optional but at least one is required: with
    /// neither the run exits 1 («вкажіть --org або --year»); year-only
    /// headers as «Документи {yyyy} року:», org + year as
    /// «Документи видавця {label}: {yyyy} року:». Only documents whose
    /// `org_date` is a full 8-digit date are listed, ordered
    /// `org_date DESC, nreg ASC` (newest first); `--year РРРР` restricts
    /// them exactly like `orgs --year`. Rows render
    /// «{dd.mm.yyyy}  {nreg}  {nazva}» (the `recent` docs idiom) and the
    /// «разом: N» verdict counts the PRINTED rows (post-`--limit`), the
    /// `recent` rule. `--limit N` (default 50) clamps to 1..=500 — no
    /// hard error. Pure offline: the command never touches the network;
    /// informational — an unknown publisher or an empty year window
    /// exits 0 with the empty report (header + «немає документів за цей
    /// період» + «разом: 0»); only a missing index, neither filter
    /// given or a malformed `--year` exit 1.
    Docs {
        /// Publisher key to list (the `[org_num]` value `orgs` prints;
        /// optional — at least one of `--org` / `--year` is required,
        /// with neither the run exits 1 «вкажіть --org або --year»).
        #[arg(long)]
        org: Option<String>,
        /// Restrict the list to documents whose `org_date` starts with
        /// these four digits and is a full 8-digit date (e.g. `2026`)
        /// and name the year in the header («Документи {yyyy} року:»
        /// without `--org`); anything else than four ASCII digits exits
        /// 1 with the same error as `orgs --year`; absent lists all
        /// dated documents (of the publisher when `--org` is given).
        #[arg(long)]
        year: Option<String>,
        /// Max documents to print (default 50, clamped to 1..=500).
        #[arg(long, default_value_t = 50)]
        limit: usize,
        /// Emit the JSON body
        /// (`{"docs":[{date,nreg,title},…][,"org":…,"org_label":…],
        /// "total":N[,"year":"yyyy"]` — pretty, keys alphabetical,
        /// `org`/`org_label` only with `--org`, `total` counts the
        /// listed rows) instead of the report.
        #[arg(long)]
        json: bool,
        /// Write the exact stdout report (or the `--json` body) to this file
        /// instead of stdout (parent dirs created). Plain report gets a
        /// UTF-8 BOM (Excel-friendly); with `--json` the file is written
        /// BOM-less; the `[docs] записано …` note goes to stderr.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Offline «what condition is my index in»: document-status
    /// distribution on the `status` column of `docs` — how many documents
    /// are чинний / не чинний / …, with labels resolved from the offline
    /// `stan` dictionary. Complements `dates` / `orgs`, shows the index
    /// freshness mix, and helps pick the status filters. Pure offline: the
    /// command never touches the network; informational — always exits 0.
    Statuses {
        /// Emit the JSON body
        /// (`{"statuses":[{count,id,label},…],"total":N}`) instead of the
        /// report.
        #[arg(long)]
        json: bool,
        /// Write the exact stdout report (or the `--json` body) to this file
        /// instead of stdout (parent dirs created). Plain report gets a
        /// UTF-8 BOM (Excel-friendly); with `--json` the file is written
        /// BOM-less; the `[statuses] записано …` note goes to stderr.
        #[arg(long)]
        out: Option<PathBuf>,
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
    /// Offline outline (зміст) of a CACHED document: «Розділ/Глава …» and
    /// «Стаття …» headings parsed from the local text cache only — the
    /// command never touches the network. Read the document once
    /// (`text <nreg>`) while online to fill the cache.
    Toc {
        nreg: String,
        /// Emit JSON (`{"nreg":…,"entries":[{level,text},…]}`) instead of
        /// the indented table.
        #[arg(long)]
        json: bool,
        /// Write to this file instead of stdout (parent dirs created).
        /// Plain table gets a UTF-8 BOM (Excel-friendly); with `--json` the
        /// file is written BOM-less. Zero headings write the BOM alone;
        /// the header line stays on stdout (the house `search --out` rule).
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
    /// Cross-document annotation grep (offline): case-insensitive
    /// substring over the annotation text, kind and nreg — the three
    /// surfaces of the GUI favorites filter; insertion order preserved.
    Search {
        /// Term to look for (empty matches all).
        term: String,
        /// Emit JSON (the same array as `annot list --json`).
        #[arg(long)]
        json: bool,
        /// Write the match table (or the `--json` body) to this file
        /// instead of stdout (parent dirs created). Plain table gets a
        /// UTF-8 BOM (Excel-friendly); with `--json` the file is written
        /// BOM-less. Zero matches writes only the BOM — the header line
        /// stays on stdout (the house `search --out` zero-hit rule);
        /// the note goes to stderr.
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
        /// The file receives exactly the stdout bytes (the pretty JSON array
        /// with its trailing newline) BOM-less — machine data, the house
        /// JSON rule; the `[annot] записано {n} анотацій → {file}` note goes
        /// to stderr. No `--json` here: the export is already JSON.
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
        Cmd::Hits { term, limit, fmt, out } => {
            cmd_hits(&cli.db, &term, limit, &fmt, out.as_deref())
        }
        Cmd::Card { nreg, json, out } => cmd_card(&nreg, json, out.as_deref(), cli.pace),
        Cmd::Text {
            nreg,
            edition,
            out,
            grep,
            context,
        } => cmd_text(
            &cli.db,
            &nreg,
            edition.as_deref(),
            out.as_deref(),
            grep.as_deref(),
            context,
            cli.pace,
        ),
        Cmd::Cache { purge, search, limit, nreg, json, out, dump } => {
            cmd_cache(&cli.db, purge, search.as_deref(), limit, nreg.as_deref(), json, out.as_deref(), dump.as_deref())
        }
        Cmd::Backfill { query, limit, dry_run, json, out } => {
            cmd_backfill(&cli.db, &query, limit, dry_run, json, out.as_deref(), cli.pace)
        }
        Cmd::Stats { json, out } => cmd_stats(&cli.db, json, out.as_deref()),
        Cmd::Verify { json, out } => cmd_verify(&cli.db, json, out.as_deref()),
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
        Cmd::DictStats { json, out } => cmd_dict_stats(&cli.db, json, out.as_deref()),
        Cmd::Dicts {
            kind,
            query,
            limit,
            json,
            out,
        } => cmd_dicts(
            &cli.db,
            &kind,
            query.as_deref(),
            limit,
            json,
            out.as_deref(),
        ),
        Cmd::Resolve { text, limit, json, out } => {
            cmd_resolve(&cli.db, &text.join(" "), limit, json, out.as_deref(), cli.pace)
        }
        Cmd::Related { nreg, json, out } => {
            cmd_related(&cli.db, &nreg, json, out.as_deref(), cli.pace)
        }
        Cmd::Snapshot { what } => cmd_snapshot(&cli.db, what),
        Cmd::Status { out } => cmd_status(&cli.db, out.as_deref()),
        Cmd::Annot { what } => cmd_annot(&cli.db, what),
        Cmd::RecentAnnots { limit, json, out } => cmd_recent(&cli.db, limit, json, out.as_deref()),
        Cmd::Annots {
            nreg,
            kind,
            limit,
            json,
            out,
        } => cmd_annots(
            &cli.db,
            &nreg,
            kind.as_deref(),
            limit,
            json,
            out.as_deref(),
        ),
        Cmd::Favorites { kind, sort, json, out } => {
            cmd_favorites(&cli.db, kind.as_deref(), &sort, json, out.as_deref())
        }
        Cmd::Dates { json, out } => cmd_dates(&cli.db, json, out.as_deref()),
        Cmd::Months { year, json, out } => {
            cmd_months(&cli.db, year.as_deref(), json, out.as_deref())
        }
        Cmd::Years { from, to, json, out } => cmd_years(
            &cli.db,
            from.as_deref(),
            to.as_deref(),
            json,
            out.as_deref(),
        ),
        Cmd::Recent {
            days,
            limit,
            json,
            out,
        } => cmd_recent_docs(
            &cli.db,
            days.as_deref(),
            limit.as_deref(),
            json,
            out.as_deref(),
        ),
        Cmd::Orgs {
            year,
            limit,
            json,
            out,
        } => cmd_orgs(&cli.db, year.as_deref(), limit, json, out.as_deref()),
        Cmd::Docs {
            org,
            year,
            limit,
            json,
            out,
        } => cmd_docs(&cli.db, org.as_deref(), year.as_deref(), limit, json, out.as_deref()),
        Cmd::Statuses { json, out } => {
            cmd_statuses(&cli.db, json, out.as_deref())
        }
        Cmd::Export { nreg, fmt, out } => {
            cmd_export(&cli.db, &nreg, &fmt, out.as_deref())
        }
        Cmd::Toc { nreg, json, out } => {
            cmd_toc(&cli.db, &nreg, json, out.as_deref())
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
            println!("{}", dict_fresh_json(&counts)?);
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
        let body = dict_json_body(&groups)?;
        match out {
            Some(p) => {
                // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                write_out_file(p, format!("{body}\n").as_bytes())?;
                eprintln!("{}", dict_json_out_note(p));
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

/// The `--json` body of `dict` (shared by stdout and `--out` so the two
/// cannot drift): the pretty `{"dicts":[{kind,items:[{id,label},…]},…]}`
/// object over the (already `--search`-filtered) groups. stdout prints
/// exactly this string plus a trailing newline; the `--out` file receives
/// the same bytes BOM-less (machine data, the house JSON rule).
fn dict_json_body(groups: &[(String, Vec<(i64, String)>)]) -> Result<String> {
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
    serde_json::to_string_pretty(&serde_json::json!({ "dicts": dicts }))
        .map_err(anyhow::Error::from)
}

/// The stderr note printed after `dict --json --out` has written the JSON
/// body to `file` (`[dict] записано JSON → {file}` — the same wording as
/// the `search` / `cache` / `annot list` / `resolve` / `related` JSON notes).
fn dict_json_out_note(file: &Path) -> String {
    format!("[dict] записано JSON → {}", file.display())
}

/// The `dict --fresh --json` body (shared stdout builder so the report
/// renders identically from one run to the next): the pretty
/// `{"fresh": {kind: n, …}}` object over the (kind, count) pairs; empty
/// tables report `0` (the plain report renders those as «порожньо»).
fn dict_fresh_json(counts: &[(&str, i64)]) -> Result<String> {
    let mut fresh_map = serde_json::Map::new();
    for (k, n) in counts {
        fresh_map.insert((*k).to_string(), serde_json::json!(n));
    }
    let report = serde_json::Value::Object(
        [("fresh".to_string(), serde_json::Value::Object(fresh_map))]
            .into_iter()
            .collect(),
    );
    serde_json::to_string_pretty(&report).map_err(anyhow::Error::from)
}

/// One dictionary coverage row behind `dict-stats`: the portal kind name,
/// its Ukrainian caption and the number of rows that kind carries in the
/// shared `dict(kind, id, label)` table.
struct DictStat {
    kind: &'static str,
    label: &'static str,
    rows: i64,
}

/// The report body of `dict-stats` (shared by stdout and `--out` so the
/// two cannot drift): «словники довідників:» header, then one
/// `{label}: {rows} значень` line per kind in input order (the caller
/// passes stan, typs, organs). A zero-row kind renders the warn flavor
/// `{label}: 0 значень — словник порожній`; the report is informational,
/// so empty dictionaries never fail the run.
fn dict_stats_lines(stats: &[DictStat]) -> Vec<String> {
    let mut lines = vec!["словники довідників:".to_string()];
    for s in stats {
        if s.rows == 0 {
            lines.push(format!("{}: 0 значень — словник порожній", s.label));
        } else {
            lines.push(format!("{}: {} значень", s.label, s.rows));
        }
    }
    lines
}

/// The `--json` body of `dict-stats` (shared by stdout and `--out` so the
/// two cannot drift): the pretty `{"dicts":[{kind,label,rows},…],"total":N}`
/// object over the stats, built as a hand-rolled `serde_json::Value` like
/// the other commands' JSON bodies (`total` is the sum of all kinds).
/// stdout prints exactly this string plus a trailing newline; the `--out`
/// file receives the same bytes BOM-less (machine data, the house JSON
/// rule).
fn dict_stats_json(stats: &[DictStat]) -> String {
    let dicts: Vec<serde_json::Value> = stats
        .iter()
        .map(|s| {
            serde_json::json!({
                "kind": s.kind,
                "label": s.label,
                "rows": s.rows,
            })
        })
        .collect();
    let total: i64 = stats.iter().map(|s| s.rows).sum();
    let value = serde_json::json!({ "dicts": dicts, "total": total });
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string())
}

/// The stderr note printed after `dict-stats --out` has written `file`
/// (`total` values across all kinds): `[dict-stats] записано {total}
/// значень → {file}`.
fn dict_stats_out_note(total: i64, file: &Path) -> String {
    format!("[dict-stats] записано {total} значень → {}", file.display())
}

/// Offline dictionary coverage report (`dict-stats`): never builds a
/// client, never touches the network. A missing index file is a hard error
/// («індекс не знайдено: …», the `stats`/`verify` rule). The counts come
/// from `COUNT(*)` per kind over the shared `dict(kind, id, label)` table,
/// read through the same connection directly (the `verify` / `cache` dump
/// style). Informational: empty dictionaries render the «словник порожній»
/// flavor and the run still exits 0. With `--out` the exact stdout body
/// goes to the file (parent dirs created) and nothing prints to stdout:
/// the plain report is written UTF-8 with a BOM (Excel-friendly, like
/// search/arrivals/stats/verify), the `--json` body BOM-less; one
/// `[dict-stats] записано {total} значень → {file}` note goes to stderr.
fn cmd_dict_stats(db: &PathBuf, json: bool, out: Option<&Path>) -> Result<()> {
    if !db.exists() {
        return Err(anyhow::anyhow!("індекс не знайдено: {}", db.display()));
    }
    let conn = rada_db::open(db).with_context(|| format!("open db {}", db.display()))?;
    let mut stats: Vec<DictStat> = Vec::with_capacity(3);
    for (kind, label) in [("stan", "стани"), ("typs", "типи"), ("organs", "органи")] {
        let rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM dict WHERE kind = ?1",
                [kind],
                |r| r.get(0),
            )
            .unwrap_or(0);
        stats.push(DictStat { kind, label, rows });
    }
    let total: i64 = stats.iter().map(|s| s.rows).sum();
    if json {
        let body = dict_stats_json(&stats);
        match out {
            Some(p) => {
                // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                write_out_file(p, format!("{body}\n").as_bytes())?;
                eprintln!("{}", dict_stats_out_note(total, p));
            }
            None => println!("{body}"),
        }
        return Ok(());
    }
    let lines = dict_stats_lines(&stats);
    match out {
        Some(p) => {
            // UTF-8 BOM prefix: the same Excel-friendly convention as
            // search/arrivals/stats/verify; the bytes are exactly the
            // stdout report (the header always renders, so never
            // BOM-alone).
            write_out_file(p, format!("\u{FEFF}{}\n", lines.join("\n")).as_bytes())?;
            eprintln!("{}", dict_stats_out_note(total, p));
        }
        None => {
            for line in lines {
                println!("{line}");
            }
        }
    }
    Ok(())
}

/// Cap the `dicts --limit` option at the dictionary-row budget: `0`
/// renders one row and anything above `1000` is truncated to `1000`
/// (`usize::clamp`, the same house idiom as the [`docs_limit`] 1..=500
/// budget, raised to cover the largest dictionary); 1..=1000 passes
/// through untouched (no hard error).
fn dicts_limit(limit: usize) -> usize {
    limit.clamp(1, 1000)
}

/// Canonicalise the `dicts` KIND positional to the stored portal token:
/// the three kinds the shared `dict` table speaks (`stan`, `typs`,
/// `organs`), case-insensitive and trimmed. The GUI home dictionary
/// panel shows стани/типи/органи only as display hints next to the
/// canonical tokens, so no Ukrainian input aliases exist to accept. An
/// unknown kind is a clear error listing the accepted values (the
/// `normalize_annots_kind` kind-error family shape, exit 1 through
/// `main`).
fn normalize_dicts_kind(kind: &str) -> Result<&'static str> {
    match kind.trim().to_ascii_lowercase().as_str() {
        "stan" => Ok("stan"),
        "typs" => Ok("typs"),
        "organs" => Ok("organs"),
        _ => Err(anyhow::anyhow!(
            "невідомий словник {kind} (stan|typs|organs)"
        )),
    }
}

/// The report body of `dicts` (shared by stdout and `--out` so the two
/// cannot drift): «Словник {kind}:» header, or «Словник {kind}:
/// {query}» when `--query` filtered, then one `{id}  {label}` row per
/// entry in input order (the SQL orders `id ASC`), then the verdict
/// «разом: {shown}» over the RENDERED rows (post-`--limit`, the
/// `annots` rule). Informational: an empty dictionary or zero query
/// matches render the empty triple (header + «нічого не знайдено» +
/// «разом: 0») and the run still exits 0 (the `docs` zero-rows rule —
/// the header always renders, so the `--out` file is never BOM-alone).
fn dicts_lines(kind: &str, query: Option<&str>, rows: &[(i64, String)]) -> Vec<String> {
    let mut lines = vec![match query {
        Some(q) => format!("Словник {kind}: {q}"),
        None => format!("Словник {kind}:"),
    }];
    if rows.is_empty() {
        lines.push("нічого не знайдено".to_string());
    }
    for (id, label) in rows {
        lines.push(format!("{id}  {label}"));
    }
    lines.push(format!("разом: {}", rows.len()));
    lines
}

/// The `--json` body of `dicts` (shared by stdout and `--out` so the two
/// cannot drift): the pretty
/// `{"count":N,"kind":kind,"query":q-or-null,"rows":[{"id":N,"label":…}]}`
/// object over the rendered rows (the `docs`/`favorites` envelope
/// idiom); `query` is the verbatim filter or `null` when unfiltered.
/// Map keys sort, so «count» < «kind» < «query» < «rows» and the row
/// keys keep their alphabetical order inside every entry; `count`
/// counts the LISTED rows (the «разом» rule, post-`--limit`). stdout
/// prints exactly this string plus a trailing newline; the `--out` file
/// receives the same bytes BOM-less (machine data, the house JSON rule).
fn dicts_json(kind: &str, query: Option<&str>, rows: &[(i64, String)]) -> String {
    let items: Vec<serde_json::Value> = rows
        .iter()
        .map(|(id, label)| serde_json::json!({ "id": id, "label": label }))
        .collect();
    let value = serde_json::json!({
        "count": rows.len(),
        "kind": kind,
        "query": query,
        "rows": items,
    });
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string())
}

/// The stderr note printed after `dicts --out` has written `file`
/// (`n` rendered rows, the same wording in both modes, the
/// [`docs_list_out_note`] style): `[dicts] записано {n} записів →
/// {file}`. The `[dicts]` tag is deliberately distinct from the live
/// `dict` command's per-kind `[dict] {kind}: записано …` notes.
fn dicts_out_note(n: usize, file: &Path) -> String {
    format!("[dicts] записано {n} записів → {}", file.display())
}

/// Offline browse/search of ONE reference dictionary (`dicts`): the CLI
/// twin of the desktop home «Пошук у словниках» panel. Never builds a
/// client, never touches the network (the live loaders stay `dict` /
/// `sync`; this command only reads what they stored). A missing index
/// file is a hard error («індекс не знайдено: …», the `stats`/`verify`
/// rule); `KIND` passes [`normalize_dicts_kind`] (an unknown kind exits
/// 1 with the house validation message) and `--limit` [`dicts_limit`]
/// (clamped to 1..=1000, never an error). The rows come straight from
/// the shared `dict(kind, id, label)` table through the connection
/// `rada_db::open` returns (the `verify` / `dict-stats` direct-SQL
/// idiom): the canonical kind token is bound as a parameter (never
/// interpolated) and the rows are ordered `id ASC`. The optional
/// `--query` filter runs on the Rust side through the shared
/// [`dict_matches`] helper instead of a SQL `LIKE`: SQLite folds only
/// ASCII case in `LOWER()`/`LIKE` (`NOCASE` included), so a Cyrillic
/// needle like «міністер» would silently miss every «Міністерство …»
/// label, while `to_lowercase` folds Ukrainian letters (the
/// `dict --search` precedent, which shares this exact helper); skipping
/// SQL `LIKE` also sidesteps the `%`/`_` wildcard-escaping caveat
/// entirely. `LIMIT` is interpolated from the clamped `usize` in the
/// unfiltered statement (ASCII digits only — the `docs`
/// interpolation-safety reasoning) and applied after the filter in the
/// query pass (a SQL LIMIT before the filter would drop matches sorted
/// past the cutoff; the dictionaries hold a few hundred rows, so the
/// full-kind read is cheap). Informational: zero matches exits 0 with
/// the empty triple. With `--out` the exact stdout body goes to the
/// file (parent dirs created) and nothing prints to stdout: the plain
/// report is written UTF-8 with a BOM (Excel-friendly, like the
/// `dict-stats`/`verify` reports), the `--json` body BOM-less; one
/// `[dicts] записано {n} записів → {file}` note goes to stderr. The
/// header always renders, so the file is never BOM-alone.
fn cmd_dicts(
    db: &PathBuf,
    kind: &str,
    query: Option<&str>,
    limit: usize,
    json: bool,
    out: Option<&Path>,
) -> Result<()> {
    if !db.exists() {
        return Err(anyhow::anyhow!("індекс не знайдено: {}", db.display()));
    }
    let kind = normalize_dicts_kind(kind)?;
    let limit = dicts_limit(limit);
    let conn = rada_db::open(db).with_context(|| format!("open db {}", db.display()))?;
    let mut items: Vec<(i64, String)> = Vec::new();
    match query {
        None => {
            // unfiltered: the clamped cap goes straight into the SQL
            // (the `docs`/`annots` limit-interpolation idiom)
            let sql = format!(
                "SELECT id, label FROM dict WHERE kind = ?1 ORDER BY id ASC LIMIT {limit}"
            );
            let mut stmt = conn
                .prepare(&sql)
                .context("prepare dicts listing")?;
            let rows = stmt
                .query_map([kind], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
                .context("run dicts listing")?;
            for row in rows {
                items.push(row?);
            }
        }
        Some(q) => {
            // filtered: read the whole kind (a few hundred rows), keep
            // the Unicode-folded substring matches through the shared
            // `dict --search` helper, stop once the cap is reached
            let mut stmt = conn
                .prepare("SELECT id, label FROM dict WHERE kind = ?1 ORDER BY id ASC")
                .context("prepare dicts search")?;
            let rows = stmt
                .query_map([kind], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
                .context("run dicts search")?;
            for row in rows {
                let (id, label) = row?;
                if items.len() >= limit {
                    break;
                }
                if dict_matches(&label, q) {
                    items.push((id, label));
                }
            }
        }
    }
    if json {
        let body = dicts_json(kind, query, &items);
        match out {
            Some(p) => {
                // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                write_out_file(p, format!("{body}\n").as_bytes())?;
                eprintln!("{}", dicts_out_note(items.len(), p));
            }
            None => println!("{body}"),
        }
        return Ok(());
    }
    let lines = dicts_lines(kind, query, &items);
    match out {
        Some(p) => {
            // UTF-8 BOM prefix: the same Excel-friendly convention as
            // search/arrivals/stats/verify; the bytes are exactly the
            // stdout report (the header always renders, so never
            // BOM-alone).
            write_out_file(p, format!("\u{FEFF}{}\n", lines.join("\n")).as_bytes())?;
            eprintln!("{}", dicts_out_note(items.len(), p));
        }
        None => {
            for line in lines {
                println!("{line}");
            }
        }
    }
    Ok(())
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

/// Output format behind `hits --format` (the txt|md document axis).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HitsFormat {
    Txt,
    Md,
}

/// Validate `hits --format` (the `normalize_fmt` house rule: trim, lower,
/// a clear Ukrainian error listing the accepted values, exit 1 through
/// `main`): `txt` (alias `text`) and `md` (alias `markdown`).
fn normalize_hits_format(fmt: &str) -> Result<HitsFormat> {
    match fmt.trim().to_ascii_lowercase().as_str() {
        "txt" | "text" => Ok(HitsFormat::Txt),
        "md" | "markdown" => Ok(HitsFormat::Md),
        _ => Err(anyhow::anyhow!(
            "формат {fmt}: використовуйте txt або md"
        )),
    }
}

/// The muted-style extras of one [`rada_db::SearchHit`] row, shared by
/// both [`hits_lines`] and [`hits_md_lines`] so the two cannot drift:
/// the status label (dictionary `stan`, skipped when the status is not
/// set), the document number, the adoption date and the current-edition
/// date — each rendered only when its own field is non-empty (the house
/// include-only-non-empty-extras convention; the same fields `search_table`
/// prints inline and the GUI `export_list` renders per row). Dates go
/// through [`fmt_date`] like every other listing.
fn hits_extras(h: &rada_db::SearchHit) -> Vec<String> {
    let mut extras = Vec::new();
    if h.status != 0 {
        extras.push(format!(
            "Стан: {}",
            rada_core::models::StatusId(h.status).label()
        ));
    }
    if !h.org_num.trim().is_empty() {
        extras.push(format!("№{}", h.org_num.trim()));
    }
    if !h.org_date.trim().is_empty() {
        extras.push(format!("від {}", fmt_date(h.org_date.trim())));
    }
    if !h.poddat.trim().is_empty() {
        extras.push(format!("ред. {}", fmt_date(h.poddat.trim())));
    }
    extras
}

/// The plain-text body of `hits` (shared by stdout and `--out` so the two
/// cannot drift): «Знайдено {n} документів:» header, then a numbered
/// block per hit — `{i}. {nazva} [{nreg}]` plus an indented muted-style
/// extras line (the [`hits_extras`] strings joined with « · », like the
/// GUI txt export; a row without extras renders no second line).
fn hits_lines(rows: &[rada_db::SearchHit]) -> Vec<String> {
    let mut lines = Vec::with_capacity(1 + 2 * rows.len());
    lines.push(format!("Знайдено {} документів:", rows.len()));
    for (i, h) in rows.iter().enumerate() {
        lines.push(format!("{}. {} [{}]", i + 1, h.nazva, h.nreg));
        let extras = hits_extras(h);
        if !extras.is_empty() {
            lines.push(format!("  {}", extras.join(" · ")));
        }
    }
    lines
}

/// The markdown body of `hits` (shared by stdout and `--out`; loosely
/// mirrors the GUI `export_list` md shape, implemented independently):
/// `# Знайдено {n} документів` header, a blank line, then a numbered
/// block per hit — `{i}. **{nazva}** — [`{nreg}`](portal link)` with the
/// portal URL built from the nreg (the same
/// `https://zakon.rada.gov.ua/laws/show/{nreg}` shape as everywhere),
/// plus one nested bullet per non-empty [`hits_extras`] string.
fn hits_md_lines(rows: &[rada_db::SearchHit]) -> Vec<String> {
    let mut lines = Vec::with_capacity(2 + 2 * rows.len());
    lines.push(format!("# Знайдено {} документів", rows.len()));
    lines.push(String::new());
    for (i, h) in rows.iter().enumerate() {
        lines.push(format!(
            "{}. **{}** — [`{}`](https://zakon.rada.gov.ua/laws/show/{})",
            i + 1,
            h.nazva,
            h.nreg,
            h.nreg
        ));
        for e in hits_extras(h) {
            lines.push(format!("   - {e}"));
        }
    }
    lines
}

/// The stderr note printed after `hits --out` has written the document
/// list (`n` hits) to `file`.
fn hits_out_note(n: usize, file: &Path) -> String {
    format!("[hits] записано {n} документів → {}", file.display())
}

/// Offline «експорт списку» of the search results (the CLI counterpart of
/// the GUI search list export): the hits come from the same local FTS
/// query as `search` (`rada_db::search::search`, title term only, offset
/// 0, `limit` clamped to 1..=500 like `recent`) — the command never
/// builds a client and never touches the network, and there is no
/// `--json`: the `--format txt|md` document axis is the point. With
/// `--out` the exact stdout body goes to the file (parent dirs created)
/// and nothing prints to stdout: the list is written UTF-8 WITH a BOM
/// (human document, the same Excel-friendly convention as
/// search/arrivals/stats). Zero hits write the BOM alone (n=0 note) while
/// the «нічого не знайдено» line stays stdout-only (the house `search
/// --out` zero-hit rule, the `backfill` wording).
fn cmd_hits(
    db: &PathBuf,
    term: &[String],
    limit: usize,
    fmt: &str,
    out: Option<&Path>,
) -> Result<()> {
    let fmt = normalize_hits_format(fmt)?;
    let limit = limit.clamp(1, 500);
    let conn = rada_db::open(db)?;
    let q = rada_db::search::SearchQuery {
        nazva: if term.is_empty() {
            None
        } else {
            Some(term.join(" "))
        },
        limit,
        offset: 0,
        ..Default::default()
    };
    let (hits, _total) = rada_db::search::search(&conn, &q)?;
    if hits.is_empty() {
        // The stdout-only zero-hit report line (the house `search --out`
        // rule, the `backfill` wording): with `--out` the file carries the
        // BOM alone and the n=0 note goes to stderr.
        println!("нічого не знайдено");
        if let Some(p) = out {
            write_out_file(p, "\u{FEFF}".as_bytes())?;
            eprintln!("{}", hits_out_note(0, p));
        }
        return Ok(());
    }
    let lines = match fmt {
        HitsFormat::Txt => hits_lines(&hits),
        HitsFormat::Md => hits_md_lines(&hits),
    };
    match out {
        Some(p) => {
            // human document: UTF-8 BOM prefix, the exact stdout body plus
            // the BOM and the trailing newline (the `export` convention).
            write_out_file(p, format!("\u{FEFF}{}\n", lines.join("\n")).as_bytes())?;
            eprintln!("{}", hits_out_note(hits.len(), p));
        }
        None => {
            for line in lines {
                println!("{line}");
            }
        }
    }
    Ok(())
}

/// Live card of one document: human-readable field table on stdout by
/// default, the full pretty JSON card with `--json`. With `--out` the exact
/// stdout body goes to the file (parent dirs created) and nothing prints to
/// stdout: the human table is written UTF-8 with a BOM (Excel-friendly, like
/// search/arrivals/resolve/related), the `--json` body BOM-less. The fetch
/// stays live (one paced request) and runs BEFORE any write: a fetch failure
/// is a hard error that writes no file (the card has no offline cache), as is
/// a write failure («запис {file}»).
fn cmd_card(nreg: &str, json: bool, out: Option<&Path>, pace: u64) -> Result<()> {
    let c = client(pace);
    let j = c.card(nreg)?;
    let body = card_body(&j, json)?;
    match out {
        Some(p) => {
            if json {
                // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                write_out_file(p, format!("{body}\n").as_bytes())?;
                eprintln!("{}", card_json_out_note(p));
            } else {
                // human table: UTF-8 BOM prefix, the same Excel-friendly
                // convention as search/arrivals/resolve/related
                write_out_file(p, format!("\u{FEFF}{body}\n").as_bytes())?;
                eprintln!("{}", card_out_note(nreg, p));
            }
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
        card_json_body(j)
    } else {
        Ok(card_summary(j))
    }
}

/// The `--json` body of `card` (the pure half of [`card_body`], shared by
/// stdout and `--out` so the two cannot drift): the pretty serialization of
/// the raw [`rada_core::models::LawCardJson`] exactly as the portal sent it
/// (optional fields render as `null`; the `dict` / `related` JSON-body
/// style). stdout prints this string plus a trailing newline; the `--out`
/// file receives the same bytes BOM-less (machine data, the house JSON rule).
fn card_json_body(j: &rada_core::models::LawCardJson) -> Result<String> {
    serde_json::to_string_pretty(j).map_err(anyhow::Error::from)
}

/// Human-readable summary lines of a live JSON card (the default `card`
/// stdout). Optional fields print only when present — an absent optional
/// renders NO row at all (no dash placeholder): `None`, blank/whitespace-only
/// strings, `org == 0` and `is_archive != Some(true)` are all skipped.
/// Dates go through [`fmt_date`].
fn card_lines(j: &rada_core::models::LawCardJson) -> Vec<String> {
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
    lines
}

/// [`card_lines`] joined with `\n`: the human table printed by `card` on
/// stdout (and mirrored BOM-prefixed into the `--out` file).
fn card_summary(j: &rada_core::models::LawCardJson) -> String {
    card_lines(j).join("\n")
}

/// The stderr note printed after `card --out` has written the human summary
/// table to `file` (`[card] {nreg} → {file}` — the single-object label that
/// mirrors `[export] {nreg} ({fmt}) → {file}`: unlike the count-driven list
/// notes, the card note names the document instead of a row count).
fn card_out_note(nreg: &str, file: &Path) -> String {
    format!("[card] {nreg} → {}", file.display())
}

/// The stderr note printed after `card --json --out` has written the JSON
/// body to `file` (`[card] записано JSON → {file}` — the same wording as
/// the `search` / `cache` / `annot list` / `resolve` / `dict` / `related`
/// JSON notes).
fn card_json_out_note(file: &Path) -> String {
    format!("[card] записано JSON → {}", file.display())
}

fn cmd_text(
    db: &PathBuf,
    nreg: &str,
    edition: Option<&str>,
    out: Option<&Path>,
    grep: Option<&str>,
    context: usize,
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
            emit_text(&text, out, grep, context)
        }
        Err(e) => {
            // Offline fallback: serve the cached copy when we have one.
            let hit = rada_db::open(db)
                .ok()
                .and_then(|conn| rada_db::textcache::get(&conn, nreg, key).ok().flatten());
            match hit {
                Some(text) => {
                    eprintln!("[text] live fetch failed ({e}); served from offline cache");
                    emit_text(&text, out, grep, context)
                }
                None => Err(anyhow::anyhow!("text {nreg}: {e} (no cached copy)")),
            }
        }
    }
}

/// Print `text` to stdout, or (with `--out`) write it as UTF-8 to the file,
/// creating parent directories; a write failure is a hard error. With a
/// non-empty `--grep` needle the grepped output replaces the full body on
/// both stdout and the `--out` file ([`emit_grep`]); an empty (or
/// whitespace-only) needle only prints a muted stderr warning and the full
/// body follows. Without `--grep` the behavior is byte-identical to the
/// historical one (`--context` is ignored).
fn emit_text(
    text: &str,
    out: Option<&Path>,
    grep: Option<&str>,
    context: usize,
) -> Result<()> {
    match grep {
        Some(needle) if !needle.trim().is_empty() => return emit_grep(text, needle, out, context),
        Some(_) => eprintln!("grep: порожній рядок — друкуємо все"),
        None => {}
    }
    match out {
        Some(p) => {
            // the same verbatim body bytes through the shared
            // `write_out_file` (parent dirs created, «запис {file}» context)
            write_out_file(p, text.as_bytes())?;
            eprintln!("{}", out_note(text.chars().count(), p));
            Ok(())
        }
        None => {
            println!("{text}");
            Ok(())
        }
    }
}

/// The `--grep` output of `text` (shared by stdout and `--out` so the two
/// cannot drift). Matched lines print as `{n}: {line}` (1-based line
/// number); with `--context N` the N untouched lines around each match are
/// kept too, disjoint blocks separated by `--` ([`grep_render`]). Zero
/// matches: the single «збігів немає» line goes to stdout only (the
/// `search --facets` rule) plus a muted stderr note, and the `--out` file
/// carries the UTF-8 BOM alone (the `cache --search --out` rule) with a
/// `записано 0 рядків` note. A non-empty result on stdout prints exactly
/// the rendered lines; with `--out` the same bytes go to the file with the
/// Excel-friendly BOM prefix ([`write_out_file`]) and one
/// `[text] записано {matches} рядків → {file}` note goes to stderr.
fn emit_grep(text: &str, needle: &str, out: Option<&Path>, context: usize) -> Result<()> {
    let kept = grep_body(text, needle, context);
    let matches = kept.iter().filter(|(_, _, m)| *m).count();
    let lines = grep_render(&kept);
    if kept.is_empty() {
        println!("збігів немає");
        eprintln!("[text] збігів немає");
        if let Some(p) = out {
            write_out_file(p, "\u{FEFF}".as_bytes())?;
            eprintln!("{}", text_grep_out_note(0, p));
        }
        return Ok(());
    }
    match out {
        Some(p) => {
            // human table: UTF-8 BOM prefix, the same Excel-friendly
            // convention as search/arrivals/resolve/related
            write_out_file(p, format!("\u{FEFF}{}\n", lines.join("\n")).as_bytes())?;
            eprintln!("{}", text_grep_out_note(matches, p));
        }
        None => {
            for line in lines {
                println!("{line}");
            }
        }
    }
    Ok(())
}

/// Pure grep core behind `text --grep`: keep every line of `body` whose
/// plain `to_lowercase` form contains the trimmed-lowercased `needle`, plus
/// the `context` lines before and after each match (overlapping windows
/// merge, lines are deduped and stay in ascending 1-based order). Returns
/// `(line_no, line text, is_match)` triples; `is_match` marks the actual
/// matches (context lines are `false`). An empty or whitespace-only needle
/// matches every line; the caller decides whether that means "print all".
fn grep_body(body: &str, needle: &str, context: usize) -> Vec<(usize, String, bool)> {
    let needle = needle.trim().to_lowercase();
    let lines: Vec<&str> = body.lines().collect();
    let matched: Vec<bool> = lines
        .iter()
        .map(|l| l.to_lowercase().contains(&needle))
        .collect();
    let mut keep = vec![false; lines.len()];
    for (i, m) in matched.iter().enumerate() {
        if *m {
            let lo = i.saturating_sub(context);
            let hi = (i + context).min(lines.len() - 1);
            for k in lo..=hi {
                keep[k] = true;
            }
        }
    }
    lines
        .into_iter()
        .enumerate()
        .filter(|(i, _)| keep[*i])
        .map(|(i, l)| (i + 1, l.to_string(), matched[i]))
        .collect()
}

/// Render the kept [`grep_body`] lines as `{n}: {line}` strings, inserting
/// a `--` separator between disjoint blocks — maximal runs of kept lines
/// whose numbers are consecutive: a boundary that skips ≥1 body line emits
/// the separator, never at the very start or end. Empty input renders no
/// lines (the caller prints the stdout-only «збігів немає» instead).
fn grep_render(kept: &[(usize, String, bool)]) -> Vec<String> {
    let mut lines: Vec<String> = Vec::with_capacity(kept.len());
    for (idx, (n, text, _)) in kept.iter().enumerate() {
        if idx > 0 && *n != kept[idx - 1].0 + 1 {
            lines.push("--".to_string());
        }
        lines.push(format!("{n}: {text}"));
    }
    lines
}

/// The stderr note printed after `text --out` has written `file`.
fn out_note(chars: usize, file: &Path) -> String {
    format!("[text] записано {chars} симв. → {}", file.display())
}

/// The stderr note printed after `text --grep --out` has written the
/// grepped output (`matches` matched lines) to `file`.
fn text_grep_out_note(matches: usize, file: &Path) -> String {
    format!("[text] записано {matches} рядків → {}", file.display())
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

/// One outline heading recognized in a cached body: `level` 1 for
/// «Розділ/Глава …», 2 for «Стаття …»; `text` is the trimmed line.
struct TocEntry {
    level: u8,
    text: String,
}

/// Headings are short structural lines: a candidate longer than this (in
/// characters, Cyrillic-safe) is a body paragraph that merely starts with
/// the keyword, not a heading.
const TOC_MAX_LINE_CHARS: usize = 120;

/// Outline heading level of `line` (already trimmed): the leading
/// alphabetic run — the first word — is lowercased and matched against the
/// structural keywords, mirroring the GUI reader's classification
/// (`reader.rs` `kw`/`PKind`: «Стаття» tested before «Розділ/Глава», any
/// case), with a word-boundary twist («Розділяй …» is not a heading).
/// `Some(1)` for «Розділ/Глава …», `Some(2)` for «Стаття …», `None` for
/// prose.
fn toc_keyword(line: &str) -> Option<u8> {
    let end = line
        .find(|c: char| !c.is_alphabetic())
        .unwrap_or(line.len());
    let word = line[..end].to_lowercase();
    // GUI parity: an «Стаття …» line stays an article even when it mentions
    // a «розділ» later on.
    if word == "стаття" {
        Some(2)
    } else if word == "розділ" || word == "глава" {
        Some(1)
    } else {
        None
    }
}

/// Pure outline parser behind `toc`: one [`TocEntry`] per line of `body`
/// whose trimmed form starts with a structural keyword as a whole first
/// word and is not absurdly long ([`TOC_MAX_LINE_CHARS`], longer lines are
/// body paragraphs); document order preserved. An act without sections
/// yields an empty outline (a valid result, not an error).
fn toc_entries(body: &str) -> Vec<TocEntry> {
    let mut out = Vec::new();
    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty() || line.chars().count() > TOC_MAX_LINE_CHARS {
            continue;
        }
        if let Some(level) = toc_keyword(line) {
            out.push(TocEntry {
                level,
                text: line.to_string(),
            });
        }
    }
    out
}

/// The plain-table body of `toc` (shared by stdout and `--out` so the two
/// cannot drift): «{n} розділів(и) та статей:» header, then one line per
/// entry — level 1 flush left, level 2 indented two spaces. The header
/// always renders (the stdout zero-heading line); the `--out` file carries
/// the BOM alone when the outline is empty (the house `search --out` rule).
fn toc_lines(entries: &[TocEntry]) -> Vec<String> {
    let mut lines = Vec::with_capacity(entries.len() + 1);
    lines.push(format!("{} розділів(и) та статей:", entries.len()));
    for e in entries {
        if e.level >= 2 {
            lines.push(format!("  {}", e.text));
        } else {
            lines.push(e.text.clone());
        }
    }
    lines
}

/// The stderr note printed after `toc --out` has written the outline
/// (`n` headings) to `file`.
fn toc_out_note(n: usize, file: &Path) -> String {
    format!("[toc] записано {n} заголовків → {}", file.display())
}

/// Offline outline of a CACHED document: the body is read from the local
/// text cache under the current-edition key (`""`) — the command never
/// builds a paced client and never touches the network. An uncached `nreg`
/// is a hard error pointing at `text` (the cache warmer), exactly like
/// `export`. With `--out` the rendered table is written UTF-8 WITH a BOM
/// (human table, parent dirs created), stdout stays silent and one
/// `[toc] записано {n} заголовків → {file}` note goes to stderr; zero
/// headings write the BOM alone (n=0 note). Without `--out` the same bytes
/// minus the BOM go to stdout; `--json` emits the pretty
/// `{"nreg":…,"entries":[{level,text},…]}` body, BOM-less in both modes.
fn cmd_toc(db: &PathBuf, nreg: &str, json: bool, out: Option<&Path>) -> Result<()> {
    let conn = rada_db::open(db)?;
    let body = rada_db::textcache::get(&conn, nreg, "")?.ok_or_else(|| {
        anyhow::anyhow!("документ {nreg} не кешовано — спершу прочитайте його (text {nreg})")
    })?;
    let entries = toc_entries(&body);
    if json {
        let value = serde_json::json!({
            "nreg": nreg,
            "entries": entries
                .iter()
                .map(|e| serde_json::json!({ "level": e.level, "text": e.text }))
                .collect::<Vec<_>>(),
        });
        let body = serde_json::to_string_pretty(&value).map_err(anyhow::Error::from)?;
        match out {
            Some(p) => {
                // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                write_out_file(p, format!("{body}\n").as_bytes())?;
                eprintln!("{}", toc_out_note(entries.len(), p));
            }
            None => println!("{body}"),
        }
        return Ok(());
    }
    let lines = toc_lines(&entries);
    match out {
        Some(p) => {
            // UTF-8 BOM prefix: the same Excel-friendly convention as
            // search/arrivals/resolve/related; the bytes are exactly the
            // stdout table, so a zero-heading outline carries the BOM alone
            // (the header line is the stdout-only zero message, the house
            // rule).
            let text = if entries.is_empty() {
                "\u{FEFF}".to_string()
            } else {
                format!("\u{FEFF}{}\n", lines.join("\n"))
            };
            write_out_file(p, text.as_bytes())?;
            eprintln!("{}", toc_out_note(entries.len(), p));
        }
        None => {
            for line in lines {
                println!("{line}");
            }
        }
    }
    Ok(())
}

/// Offline text cache: `--purge` deletes cached bodies (all or one `nreg`),
/// `--search TERM` scans the cached current texts offline (ranked by
/// occurrence count), `--dump DIR` exports every cached body (all editions)
/// as UTF-8-with-BOM plain-text files, and the bare form prints cache
/// statistics. With
/// `--search --out` the exact stdout body goes to the file (parent dirs
/// created) and nothing prints to stdout: the plain table is written UTF-8
/// with a BOM (Excel-friendly, like search/arrivals/resolve/related), the
/// `--json` body BOM-less; zero hits write the BOM alone (n=0 note). The
/// «у кешованому тексті збігів … немає» line is stdout-only (the
/// `search --facets` rule): it never enters the file. The bare stats form
/// honors `--out` too: the one-line report ([`cache_stats_lines`]) goes to
/// the file BOM-prefixed with a `[cache] записано → {file}` note (it always
/// renders, so never BOM-alone). `--json` applies to `--search` only;
/// `--dump` and `--purge` ignore `--out`.
fn cmd_cache(
    db: &PathBuf,
    purge: bool,
    search: Option<&str>,
    limit: Option<usize>,
    nreg: Option<&str>,
    json: bool,
    out: Option<&Path>,
    dump: Option<&Path>,
) -> Result<()> {
    let conn = rada_db::open(db)?;
    if purge {
        let n = rada_db::textcache::purge(&conn, nreg)?;
        eprintln!("[cache] видалено рядків: {n}");
        return Ok(());
    }
    // `--dump DIR`: offline bulk export of every cached body. The textcache
    // API exposes no enumeration helper (get/purge/stats/search_bodies only),
    // so — like the `nreg` stats query below — this reads the `docs_text`
    // table through the same connection directly. Dated editions (keys
    // besides `""`) get their own file; bodies go verbatim + trailing
    // newline, UTF-8 WITH a BOM (human documents, via `write_out_file`).
    // stdout stays quiet: only the final summary line.
    if let Some(dir) = dump {
        let mut stmt = conn
            .prepare(
                "SELECT nreg, edition, body FROM docs_text
                 ORDER BY nreg, edition",
            )
            .context("dump docs_text")?;
        let mut rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .context("dump docs_text")?;
        let mut written = 0usize;
        while let Some((nreg, edition, body)) = rows.next().transpose()? {
            let name = if edition.is_empty() {
                format!("{}.txt", cache_dump_filename(&nreg))
            } else {
                format!(
                    "{}.{}.txt",
                    cache_dump_filename(&nreg),
                    cache_dump_filename(&edition)
                )
            };
            // human document: UTF-8 with a BOM; a write failure aborts with
            // the usual «запис {file}» context (exit 1 via main).
            write_out_file(&dir.join(name), format!("\u{FEFF}{body}\n").as_bytes())?;
            written += 1;
        }
        for line in cache_dump_lines(written, dir) {
            println!("{line}");
        }
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
            let body = cache_search_json_body(term, &rows)?;
            match out {
                Some(p) => {
                    // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                    write_out_file(p, format!("{body}\n").as_bytes())?;
                    eprintln!("{}", cache_search_json_out_note(p));
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
    let lines = cache_stats_lines(docs, rows, bytes);
    match out {
        Some(p) => {
            // UTF-8 BOM prefix: the same Excel-friendly convention as
            // search/arrivals/resolve/related; the bytes are exactly the
            // stdout report (the one line always renders, so the file is
            // never BOM-alone).
            write_out_file(p, format!("\u{FEFF}{}\n", lines.join("\n")).as_bytes())?;
            eprintln!("{}", cache_stats_out_note(p));
        }
        None => {
            for line in lines {
                println!("{line}");
            }
        }
    }
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

/// The `--json` body of `cache --search` (shared by stdout and `--out` so
/// the two cannot drift): the pretty
/// `{"term":…,"hits":[{nreg,title,hits,snippet},…]}` object over the
/// resolved `(hit, title)` pairs (titles come from the offline index).
/// stdout prints exactly this string plus a trailing newline; the `--out`
/// file receives the same bytes BOM-less (machine data, the house JSON
/// rule). Zero hits render `"hits": []` (the `dict --json` empty style).
fn cache_search_json_body(
    term: &str,
    hits: &[(rada_db::textcache::CacheHit, String)],
) -> Result<String> {
    let value = serde_json::json!({
        "term": term,
        "hits": hits
            .iter()
            .map(|(h, title)| serde_json::json!({
                "nreg": h.nreg,
                "title": title,
                "hits": h.hits,
                "snippet": h.snippet,
            }))
            .collect::<Vec<_>>(),
    });
    serde_json::to_string_pretty(&value).map_err(anyhow::Error::from)
}

/// The stderr note printed after `cache --search --json --out` has written
/// the JSON body to `file` (`[cache] записано JSON → {file}` — the same
/// wording as the `search` / `annot list` / `resolve` / `dict` / `related`
/// JSON notes).
fn cache_search_json_out_note(file: &Path) -> String {
    format!("[cache] записано JSON → {}", file.display())
}

/// The stdout report of the bare `cache` stats form (shared by stdout and
/// `--out` so the two cannot drift): the single
/// `rows={docs}  total_rows={rows}  size={:.2} MB` line (double-space
/// separators, size in MB with two decimals). `docs` is the `--nreg` row
/// count (or the global total when no `nreg` was given). An empty cache
/// still renders `rows=0  total_rows=0  size=0.00 MB`, so the `--out` file
/// never carries the BOM alone.
fn cache_stats_lines(docs: i64, rows: i64, bytes: i64) -> Vec<String> {
    vec![format!(
        "rows={docs}  total_rows={rows}  size={:.2} MB",
        bytes as f64 / 1_048_576.0
    )]
}

/// The stderr note printed after the bare `cache` stats form wrote `file`
/// (`[cache] записано → {file}` — the `stats` report-note style; the stats
/// line carries no natural count to announce, like `stats` itself).
fn cache_stats_out_note(file: &Path) -> String {
    format!("[cache] записано → {}", file.display())
}

/// The dump filename stem behind `cache --dump` (pure): ASCII alphanumerics
/// and `-`, `_`, `.` are kept; every other char — including the
/// Windows-reserved residue `: * ? / \ < > |` and any non-ASCII letter —
/// becomes `_`. An empty or whitespace-only nreg yields `"doc"`; the stem
/// is capped at 80 chars (the caller appends the `.txt` suffix, plus the
/// `.{edition}` infix for dated editions).
fn cache_dump_filename(nreg: &str) -> String {
    if nreg.trim().is_empty() {
        return "doc".to_string();
    }
    nreg.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .take(80)
        .collect()
}

/// The stdout of `cache --dump` (pure): never per-file lines, only the one
/// final summary `документів записано: {n} → {dir}`, or the empty-cache
/// notice `кеш текстів порожній — нічого експортувати` when nothing was
/// exported (still a normal exit 0).
fn cache_dump_lines(n: usize, dir: &Path) -> Vec<String> {
    if n == 0 {
        vec!["кеш текстів порожній — нічого експортувати".to_string()]
    } else {
        vec![format!("документів записано: {n} → {}", dir.display())]
    }
}

/// Final summary line of `backfill` (one line, pure): counts of the run plus
/// the cache row total afterwards (`textcache::stats`).
fn backfill_summary(fetched: usize, cached_already: usize, failed: usize, total_rows: i64) -> String {
    format!(
        "отримано {fetched}, вже в кеші {cached_already}, помилок {failed}; кеш тепер: {total_rows} тіл"
    )
}

/// The summary-report body of `backfill` (shared by stdout and `--out` so
/// the two cannot drift): the single [`backfill_summary`] counter line,
/// carrying the ` (сухий прогін)` tail on a `--dry-run`. The live per-doc
/// progress lines are progress, not body: they never enter this builder
/// (and never enter the `--out` file).
fn backfill_summary_lines(
    fetched: usize,
    cached_already: usize,
    failed: usize,
    total_rows: i64,
    dry_run: bool,
) -> Vec<String> {
    let line = backfill_summary(fetched, cached_already, failed, total_rows);
    vec![if dry_run {
        format!("{line} (сухий прогін)")
    } else {
        line
    }]
}

/// The `--json` body of `backfill` (shared by stdout and `--out` so the two
/// cannot drift): the pretty `{"cached_already":…,"failed":…,"fetched":…,"rows":…}`
/// object over the run counters (map keys sort, the historical inline
/// shape). stdout prints exactly this string plus a trailing newline; the
/// `--out` file receives the same bytes BOM-less (machine data, the house
/// JSON rule).
fn backfill_json_body(
    fetched: usize,
    cached_already: usize,
    failed: usize,
    total_rows: i64,
) -> Result<String> {
    let value = serde_json::json!({
        "fetched": fetched,
        "cached_already": cached_already,
        "failed": failed,
        "rows": total_rows,
    });
    serde_json::to_string_pretty(&value).map_err(anyhow::Error::from)
}

/// The stderr note printed after `backfill --out` has written the summary
/// report (`n_lines` report lines) to `file`.
fn backfill_out_note(n_lines: usize, file: &Path) -> String {
    format!("[backfill] записано {n_lines} рядків звіту → {}", file.display())
}

/// The stderr note printed after `backfill --json --out` has written the
/// JSON body to `file` (`[backfill] записано JSON → {file}` — the same
/// wording as the `search` / `cache` / `annot list` / `resolve` / `dict` /
/// `related` JSON notes).
fn backfill_json_out_note(file: &Path) -> String {
    format!("[backfill] записано JSON → {}", file.display())
}

/// Fetch and cache the act texts of the top hits of a local title search so
/// later reading is offline. The hits come from `rada_db::search::search`
/// (`nazva` only, `limit` clamped to 1..=25); already-cached documents are
/// skipped, each missing one goes through the paced client `show_text` and is
/// put into the cache under the current edition (`""`), like `text` does.
/// A single fetch failure prints a muted line and never aborts the loop —
/// the command is best-effort and exits 0 even when every fetch fails.
/// `--dry-run` only lists the cached/missing state: no network, no writes.
/// With `--out` the exact stdout summary report (or the `--json` body) goes
/// to the file (parent dirs created) and does not print to stdout: the
/// plain report is written UTF-8 with a BOM (Excel-friendly, like
/// search/arrivals/related), the `--json` body BOM-less; one
/// `[backfill] записано …` note goes to stderr. The per-doc progress lines
/// are progress, not body: they stay on stdout even with `--out`. On
/// «нічого не знайдено» the report line stays stdout-only and the file
/// receives the BOM alone (the house `search --out` zero-hit rule).
fn cmd_backfill(
    db: &PathBuf,
    query: &str,
    limit: usize,
    dry_run: bool,
    json: bool,
    out: Option<&Path>,
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
        // The stdout-only zero-hit report line (the house `search --out`
        // rule): with `--out` the file carries the summary body, which is
        // empty here, so it receives the BOM alone and the note goes to
        // stderr.
        println!("нічого не знайдено");
        if let Some(p) = out {
            write_out_file(p, "\u{FEFF}".as_bytes())?;
            eprintln!("{}", backfill_out_note(0, p));
        }
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
        let body = backfill_json_body(fetched, cached_already, failed, rows)?;
        match out {
            Some(p) => {
                // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                write_out_file(p, format!("{body}\n").as_bytes())?;
                eprintln!("{}", backfill_json_out_note(p));
            }
            None => println!("{body}"),
        }
        return Ok(());
    }
    let lines = backfill_summary_lines(fetched, cached_already, failed, rows, dry_run);
    match out {
        Some(p) => {
            // UTF-8 BOM prefix: the same Excel-friendly convention as
            // search/arrivals/resolve/related; the bytes are exactly the
            // stdout summary report (the per-doc progress lines printed
            // above are progress, not body, and stayed on stdout).
            write_out_file(p, format!("\u{FEFF}{}\n", lines.join("\n")).as_bytes())?;
            eprintln!("{}", backfill_out_note(lines.len(), p));
        }
        None => {
            for line in lines {
                println!("{line}");
            }
        }
    }
    Ok(())
}

/// Plain-data snapshot behind `stats`: everything read offline from the
/// local index, ready for both renderers (aligned lines / JSON). Every
/// source except the db path is optional: a source that is absent, empty
/// at the file level or fails to read stays `None` and is skipped from
/// both outputs. The JSON body mirrors these field names 1:1.
struct StatsInfo {
    db: String,
    docs: Option<i64>,
    size_bytes: Option<u64>,
    last_import: Option<String>,
    annotations: Option<i64>,
    dict_stan: Option<i64>,
    dict_typs: Option<i64>,
    dict_organs: Option<i64>,
    cache_rows: Option<i64>,
    cache_bytes: Option<u64>,
}

/// Aligned `key : value` report lines for `stats` (human mode), card_summary
/// style: the index file always renders, every `None` source contributes no
/// line, empty dict tables and an empty text cache render as «порожньо»,
/// sizes group bytes into `{:.1} MB` like the `cache` report does.
fn stats_lines(info: &StatsInfo) -> Vec<String> {
    let mb = |b: u64| format!("{:.1}", b as f64 / 1_048_576.0);
    let dict = |n: i64| if n == 0 { "порожньо".to_string() } else { n.to_string() };
    let mut rows: Vec<(String, String)> = vec![("файл індексу".into(), info.db.clone())];
    if let Some(b) = info.size_bytes {
        rows.push(("розмір індексу".into(), format!("{} MB", mb(b))));
    }
    if let Some(t) = &info.last_import {
        rows.push(("останній імпорт/синк".into(), t.clone()));
    }
    if let Some(n) = info.docs {
        rows.push(("документів".into(), n.to_string()));
    }
    if let Some(n) = info.annotations {
        rows.push(("анотацій".into(), n.to_string()));
    }
    if let Some(n) = info.dict_stan {
        rows.push(("словник stan".into(), dict(n)));
    }
    if let Some(n) = info.dict_typs {
        rows.push(("словник typs".into(), dict(n)));
    }
    if let Some(n) = info.dict_organs {
        rows.push(("словник organs".into(), dict(n)));
    }
    if let Some(n) = info.cache_rows {
        let value = if n == 0 {
            "порожньо".to_string()
        } else {
            format!("{} тіл · {} MB", n, mb(info.cache_bytes.unwrap_or(0)))
        };
        rows.push(("кеш текстів".into(), value));
    }
    let w = rows.iter().map(|(k, _)| k.chars().count()).max().unwrap_or(0);
    rows.into_iter().map(|(k, v)| format!("{k:<w$} : {v}")).collect()
}

/// The `--json` body of `stats` (shared by stdout and `--out` so the two
/// cannot drift): the [`StatsInfo`] fields serialized 1:1, pretty, in the
/// same style as the other commands' JSON bodies; unavailable sources are
/// `null`.
fn stats_json(info: &StatsInfo) -> String {
    let value = serde_json::json!({
        "db": info.db,
        "docs": info.docs,
        "size_bytes": info.size_bytes,
        "last_import": info.last_import,
        "annotations": info.annotations,
        "dict_stan": info.dict_stan,
        "dict_typs": info.dict_typs,
        "dict_organs": info.dict_organs,
        "cache_rows": info.cache_rows,
        "cache_bytes": info.cache_bytes,
    });
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string())
}

/// The stderr note printed after `stats --out` has written `file`
/// (`[stats] записано → {file}`), shared by the table and the `--json` mode.
fn stats_out_note(file: &Path) -> String {
    format!("[stats] записано → {}", file.display())
}

/// Offline index report (`stats`): never builds a client, never touches the
/// network. A missing index file is a hard error («індекс не знайдено: …»),
/// mirroring the `open(db)?` failures of the other offline commands. Each
/// source is gathered independently and a source that cannot be read is
/// skipped from the report. With `--out` the exact stdout body goes to the
/// file (parent dirs created) and nothing prints to stdout: the plain
/// report is written UTF-8 with a BOM (Excel-friendly, like
/// search/arrivals/resolve/related), the `--json` body BOM-less; one
/// `[stats] записано → {file}` note goes to stderr.
fn cmd_stats(db: &PathBuf, json: bool, out: Option<&Path>) -> Result<()> {
    if !db.exists() {
        return Err(anyhow::anyhow!("індекс не знайдено: {}", db.display()));
    }
    let conn = rada_db::open(db).with_context(|| format!("open db {}", db.display()))?;
    // Every source degrades to `None` on failure: the report shows what exists.
    let cache = rada_db::textcache::stats(&conn).ok();
    let info = StatsInfo {
        db: db.display().to_string(),
        docs: rada_db::doc_count(&conn).ok(),
        size_bytes: std::fs::metadata(db).ok().map(|m| m.len()),
        last_import: rada_db::last_import(&conn)
            .map(|t| {
                let t = t.trim().to_string();
                if t.is_empty() {
                    None
                } else {
                    Some(t)
                }
            })
            .flatten(),
        annotations: rada_db::annotations::all(&conn).ok().map(|v| v.len() as i64),
        dict_stan: Some(rada_db::dict::all_labels(&conn, "stan").len() as i64),
        dict_typs: Some(rada_db::dict::all_labels(&conn, "typs").len() as i64),
        dict_organs: Some(rada_db::dict::all_labels(&conn, "organs").len() as i64),
        cache_rows: cache.map(|(rows, _)| rows),
        cache_bytes: cache.map(|(_, bytes)| bytes.max(0) as u64),
    };
    if json {
        let body = stats_json(&info);
        match out {
            Some(p) => {
                // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                write_out_file(p, format!("{body}\n").as_bytes())?;
                eprintln!("{}", stats_out_note(p));
            }
            None => println!("{body}"),
        }
        return Ok(());
    }
    let lines = stats_lines(&info);
    match out {
        Some(p) => {
            // UTF-8 BOM prefix: the same Excel-friendly convention as
            // search/arrivals/resolve/related; the bytes are exactly the
            // stdout report, so an empty report carries the BOM alone.
            let text = if lines.is_empty() {
                "\u{FEFF}".to_string()
            } else {
                format!("\u{FEFF}{}\n", lines.join("\n"))
            };
            write_out_file(p, text.as_bytes())?;
            eprintln!("{}", stats_out_note(p));
        }
        None => {
            for line in lines {
                println!("{line}");
            }
        }
    }
    Ok(())
}

/// Outcome of one `verify` check.
#[derive(Clone, Copy, PartialEq, Eq)]
enum VerifyStatus {
    Ok,
    Warn,
    Fail,
}

impl VerifyStatus {
    /// The plain CLI status prefix: `stats`/`cache` print plain text with
    /// no decorative glyphs, so `verify` keeps the same convention.
    fn as_str(&self) -> &'static str {
        match self {
            VerifyStatus::Ok => "ok",
            VerifyStatus::Warn => "увага",
            VerifyStatus::Fail => "помилка",
        }
    }
}

/// One `verify` check: the label, its status and the detail printed after
/// the «: ».
struct VerifyCheck {
    name: String,
    status: VerifyStatus,
    detail: String,
}

/// `true` when no check failed (warnings are allowed): the single rule
/// behind the verdict line in [`verify_lines`], the `--json` exit field
/// and the exit code of `verify`.
fn verify_healthy(checks: &[VerifyCheck]) -> bool {
    !checks.iter().any(|c| c.status == VerifyStatus::Fail)
}

/// The report body of `verify` (shared by stdout and `--out` so the two
/// cannot drift): one `{mark} {name}: {detail}` line per check with the
/// plain `ok`/`увага`/`помилка` prefixes, then the verdict line — derived
/// from the statuses inside this SAME builder: «вердикт: є проблеми» when
/// any check failed, «вердикт: база здорова» otherwise.
fn verify_lines(checks: &[VerifyCheck]) -> Vec<String> {
    let mut lines: Vec<String> = checks
        .iter()
        .map(|c| format!("{} {}: {}", c.status.as_str(), c.name, c.detail))
        .collect();
    lines.push(format!(
        "вердикт: {}",
        if verify_healthy(checks) {
            "база здорова"
        } else {
            "є проблеми"
        }
    ));
    lines
}

/// The `--json` body of `verify` (shared by stdout and `--out`): the
/// checks array plus the verdict and the exit severity (`0` healthy /
/// `1` problems), built as a hand-rolled `serde_json::Value` like the
/// other commands' JSON bodies.
fn verify_json(checks: &[VerifyCheck]) -> String {
    let items: Vec<serde_json::Value> = checks
        .iter()
        .map(|c| {
            serde_json::json!({
                "name": c.name,
                "status": c.status.as_str(),
                "detail": c.detail,
            })
        })
        .collect();
    let healthy = verify_healthy(checks);
    let value = serde_json::json!({
        "checks": items,
        "verdict": if healthy { "база здорова" } else { "є проблеми" },
        "exit": if healthy { 0 } else { 1 },
    });
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string())
}

/// The stderr note printed after `verify --out` has written `file`:
/// `[verify] база здорова → {file}` / `[verify] є проблеми → {file}`.
fn verify_out_note(ok: bool, file: &Path) -> String {
    let verdict = if ok { "база здорова" } else { "є проблеми" };
    format!("[verify] {verdict} → {}", file.display())
}

/// Offline database health check (`verify`): never builds a client, never
/// touches the network. A missing index file is a hard error
/// («індекс не знайдено: …», the `stats` rule — no partial report). Each
/// check degrades gracefully to `fail`/`warn` instead of aborting; the run
/// exits 1 as soon as any check failed (warnings alone stay exit 0). With
/// `--out` the exact stdout body goes to the file (parent dirs created)
/// and nothing prints to stdout: the plain report is written UTF-8 with a
/// BOM (Excel-friendly, like search/arrivals/stats), the `--json` body
/// BOM-less; one `[verify] {verdict} → {file}` note goes to stderr.
fn cmd_verify(db: &PathBuf, json: bool, out: Option<&Path>) -> Result<()> {
    if !db.exists() {
        return Err(anyhow::anyhow!("індекс не знайдено: {}", db.display()));
    }
    let conn = rada_db::open(db).with_context(|| format!("open db {}", db.display()))?;
    let mut checks: Vec<VerifyCheck> = Vec::new();

    // a. `PRAGMA integrity_check`: the single «ok» row when the file is
    // sound, one row per reported problem otherwise.
    let integrity = (|| -> Result<Vec<String>> {
        let mut stmt = conn.prepare("PRAGMA integrity_check")?;
        let mut rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut problems = Vec::new();
        while let Some(row) = rows.next().transpose()? {
            problems.push(row);
        }
        Ok(problems)
    })();
    let (status, detail) = match integrity {
        Ok(rows) if rows.len() == 1 && rows[0] == "ok" => {
            (VerifyStatus::Ok, "ok".to_string())
        }
        Ok(rows) if rows.is_empty() => (VerifyStatus::Fail, "немає відповіді".to_string()),
        Ok(rows) => (VerifyStatus::Fail, rows.join("; ")),
        Err(e) => (VerifyStatus::Fail, format!("{e:#}")),
    };
    checks.push(VerifyCheck {
        name: "цілість SQLite".into(),
        status,
        detail,
    });

    // b. `PRAGMA foreign_key_check`: one row per violated foreign key.
    let fk = (|| -> Result<i64> {
        let mut stmt = conn.prepare("PRAGMA foreign_key_check")?;
        let rows = stmt.query_map([], |_r| Ok(()))?;
        let mut violations = 0i64;
        for row in rows {
            row?;
            violations += 1;
        }
        Ok(violations)
    })();
    let (status, detail) = match fk {
        Ok(0) => (VerifyStatus::Ok, "ok".to_string()),
        Ok(n) => (VerifyStatus::Fail, format!("{n} порушень")),
        Err(e) => (VerifyStatus::Fail, format!("{e:#}")),
    };
    checks.push(VerifyCheck {
        name: "зовнішні ключі".into(),
        status,
        detail,
    });

    // c. documents: an empty index is suspicious but not broken (warn).
    let docs: Option<i64> = rada_db::doc_count(&conn).ok();
    checks.push(match docs {
        Some(0) => VerifyCheck {
            name: "документи".into(),
            status: VerifyStatus::Warn,
            detail: "0 — індекс порожній".into(),
        },
        Some(n) => VerifyCheck {
            name: "документи".into(),
            status: VerifyStatus::Ok,
            detail: n.to_string(),
        },
        None => VerifyCheck {
            name: "документи".into(),
            status: VerifyStatus::Fail,
            detail: "таблиця docs недоступна".into(),
        },
    });

    // d. FTS consistency: `docs_fts` (the external-content FTS5 table over
    // `docs`) vs the `docs` row count; a failing FTS query means the index
    // table itself is gone.
    let fts: Option<i64> = conn
        .query_row("SELECT COUNT(*) FROM docs_fts", [], |r| r.get(0))
        .ok();
    let (status, detail) = match (fts, docs) {
        (Some(f), Some(d)) if f == d => (VerifyStatus::Ok, f.to_string()),
        (Some(f), Some(d)) => (VerifyStatus::Warn, format!("docs={d}, docs_fts={f}")),
        (Some(f), None) => (VerifyStatus::Warn, format!("docs_fts={f}, docs недоступна")),
        (None, _) => (VerifyStatus::Fail, "відсутній".to_string()),
    };
    checks.push(VerifyCheck {
        name: "повнотекстовий індекс".into(),
        status,
        detail,
    });

    // e. duplicate nregs: the schema guards nreg UNIQUE, a nonzero count
    // means something bypassed it (warn, not fatal).
    let dups: Option<i64> = conn
        .query_row(
            "SELECT COUNT(*) FROM (SELECT nreg FROM docs GROUP BY nreg HAVING COUNT(*) > 1)",
            [],
            |r| r.get(0),
        )
        .ok();
    let (status, detail) = match dups {
        Some(0) => (VerifyStatus::Ok, "немає".to_string()),
        Some(n) => (VerifyStatus::Warn, n.to_string()),
        None => (VerifyStatus::Fail, "таблиця docs недоступна".to_string()),
    };
    checks.push(VerifyCheck {
        name: "дублікати nreg".into(),
        status,
        detail,
    });

    // f. text cache: informational only — an empty cache is normal.
    let cache_rows = rada_db::textcache::stats(&conn)
        .map(|(rows, _)| rows)
        .unwrap_or(0);
    checks.push(VerifyCheck {
        name: "кеш текстів".into(),
        status: VerifyStatus::Ok,
        detail: cache_rows.to_string(),
    });

    let healthy = verify_healthy(&checks);
    if json {
        let body = verify_json(&checks);
        match out {
            Some(p) => {
                // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                write_out_file(p, format!("{body}\n").as_bytes())?;
                eprintln!("{}", verify_out_note(healthy, p));
            }
            None => println!("{body}"),
        }
    } else {
        let lines = verify_lines(&checks);
        match out {
            Some(p) => {
                // UTF-8 BOM prefix: the same Excel-friendly convention as
                // search/arrivals/stats; the bytes are exactly the stdout
                // report (the checks always render, so never BOM-alone).
                write_out_file(p, format!("\u{FEFF}{}\n", lines.join("\n")).as_bytes())?;
                eprintln!("{}", verify_out_note(healthy, p));
            }
            None => {
                for line in lines {
                    println!("{line}");
                }
            }
        }
    }
    // Any fail ⇒ exit 1 (every report line already went through the
    // line-flushing stdout); warnings alone keep the run at exit 0.
    if !healthy {
        std::process::exit(1);
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
/// The table and the JSON body are built once through the shared pure
/// builders [`arrivals_lines`] / [`arrivals_json_body`], so stdout and the
/// file cannot drift. With `--out` the exact stdout body goes to the file
/// (parent dirs created, via the shared [`write_out_file`]) and nothing
/// prints to stdout: the plain table is written UTF-8 with a BOM
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
        let body = arrivals_json_body(&feed, &new)?;
        match out {
            Some(p) => {
                // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                write_out_file(p, format!("{body}\n").as_bytes())?;
                eprintln!("{}", arrivals_json_out_note(p));
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
    let lines = arrivals_lines(&feed, &new, limit);
    match out {
        Some(p) => {
            // UTF-8 BOM prefix: the same Excel-friendly CSV convention as the GUI.
            write_out_file(p, format!("\u{FEFF}{}\n", lines.join("\n")).as_bytes())?;
            eprintln!("{}", arrivals_out_note(rows, p));
        }
        None => {
            for line in lines {
                println!("{line}");
            }
        }
    }
    Ok(())
}

/// The stderr note printed after `arrivals --out` has written the plain
/// table (`rows` document rows) to `file`.
fn arrivals_out_note(rows: usize, file: &Path) -> String {
    format!("[arrivals] записано {rows} рядків → {}", file.display())
}

/// The plain-table body of `arrivals` (shared by stdout and `--out` so the
/// two cannot drift): «{total} документів, нових: {new}» header, then at
/// most `limit` `«{marker}{nreg}  [{status}]  {nazva}»` rows — `* ` marks a
/// nreg absent from the saved snapshot, `  ` a seen one — plus the
/// `… ще {rest} див. --limit` tail when the feed exceeds `limit` (the
/// caller clamps it to 1..=1000). The header always renders, so an empty
/// feed is just the header line and the `--out` file is never BOM-alone.
fn arrivals_lines(feed: &[rada_core::models::UpdatedDoc], new: &[String], limit: usize) -> Vec<String> {
    let newset: std::collections::HashSet<&str> = new.iter().map(String::as_str).collect();
    let mut lines: Vec<String> = Vec::with_capacity(feed.len().min(limit) + 2);
    lines.push(format!("{} документів, нових: {}", feed.len(), new.len()));
    for d in feed.iter().take(limit) {
        let marker = if newset.contains(d.nreg.as_str()) { "* " } else { "  " };
        lines.push(format!("{marker}{}  [{}]  {}", d.nreg, d.status, d.nazva));
    }
    if feed.len() > limit {
        lines.push(format!("… ще {} див. --limit", feed.len() - limit));
    }
    lines
}

/// The `--json` body of `arrivals` (shared by stdout and `--out` so the two
/// cannot drift): the pretty `{"items":[…feed rows…],"new_nregs":[…]}`
/// object (the historical inline shape; `--since` filtering already happened
/// upstream, so both fields honor the filter). stdout prints exactly this
/// string plus a trailing newline; the `--out` file receives the same bytes
/// BOM-less (machine data, the house JSON rule).
fn arrivals_json_body(
    feed: &[rada_core::models::UpdatedDoc],
    new: &[String],
) -> Result<String> {
    serde_json::to_string_pretty(&serde_json::json!({
        "items": feed,
        "new_nregs": new,
    }))
    .map_err(anyhow::Error::from)
}

/// The stderr note printed after `arrivals --json --out` has written the
/// JSON body to `file` (`[arrivals] записано JSON → {file}` — the same
/// wording as the `search` / `cache` / `annot list` / `resolve` / `dict` /
/// `related` JSON notes).
fn arrivals_json_out_note(file: &Path) -> String {
    format!("[arrivals] записано JSON → {}", file.display())
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

/// Proleptic-Gregorian `YYYYMMDD` → days since 1970-01-01 — the exact
/// inverse of [`days_to_ymd`] (`ymd_to_days("19700101") == Some(0)`,
/// `days_to_ymd(0) == "19700101"`). Delegates to the crate's single
/// canonical Howard Hinnant implementation [`recent_days_from_civil`]
/// instead of carrying a second day-count formula: the previous
/// Julian-flavoured body here counted 2440496 at 1970-01-01, collapsed
/// adjacent month ends (`20261031` and `20261101` mapped to the same
/// 2461254) and drifted +3 days across February→March. `None` for
/// non-8-digit input or an impossible calendar date (bad month,
/// out-of-range day, non-leap Feb 29). Its shipped consumer
/// [`filter_since`] SUBTRACTS two values, so comparisons are
/// epoch-relative either way; the epoch fix additionally lets the
/// values compose with [`days_to_ymd`]/`today_yyyymmdd` correctly.
fn ymd_to_days(s: &str) -> Option<i64> {
    recent_days_from_civil(s)
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
            let body = resolve_json_body(hits)?;
            match out {
                Some(p) => {
                    // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                    write_out_file(p, format!("{body}\n").as_bytes())?;
                    eprintln!("{}", resolve_json_out_note(p));
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
/// (search-style formatting, dates through [`fmt_date`]). Shared by stdout
/// (multi-candidate case) and the BOM-prefixed `--out` file so the two
/// cannot drift; a single local hit keeps its historical bare-nreg stdout.
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

/// The `--json` body of `resolve` (shared by stdout and `--out` so the two
/// cannot drift): the pretty `{"candidates":[{nreg,nazva,status,org_date,
/// org_num,poddat},…]}` object over the local candidate hits (the
/// historical inline shape; the live fallback path never reaches it).
/// stdout prints exactly this string plus a trailing newline; the `--out`
/// file receives the same bytes BOM-less (machine data, the house JSON
/// rule).
fn resolve_json_body(hits: &[rada_db::SearchHit]) -> Result<String> {
    serde_json::to_string_pretty(&serde_json::json!({ "candidates": hits }))
        .map_err(anyhow::Error::from)
}

/// The stderr note printed after `resolve --json --out` has written the
/// JSON body to `file` (`[resolve] записано JSON → {file}` — the same
/// wording as the `search` / `cache` / `annot list` / `dict` / `related`
/// JSON notes).
fn resolve_json_out_note(file: &Path) -> String {
    format!("[resolve] записано JSON → {}", file.display())
}

/// List «related documents» for `nreg`: links parsed from the live portal
/// page (one paced request), titles preferred from the offline index.
/// With `--out` the exact stdout body goes to the file (parent dirs created)
/// and nothing prints to stdout: the plain table is written UTF-8 with a BOM
/// (Excel-friendly, like arrivals/resolve), the `--json` body BOM-less;
/// zero links write the BOM alone (n=0 note, the house `search --out` rule).
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
        let body = related_json_body(&list)?;
        match out {
            Some(p) => {
                // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                write_out_file(p, format!("{body}\n").as_bytes())?;
                eprintln!("{}", related_json_out_note(p));
            }
            None => println!("{body}"),
        }
        return Ok(());
    }
    let body = related_table(&list);
    match out {
        Some(p) => {
            // UTF-8 BOM prefix: the same Excel-friendly convention as
            // arrivals/resolve. Zero links write the BOM alone and no
            // trailing newline: the «не знайдено» line is the stdout-only
            // zero-hit message (the house `search --out` rule).
            let text = if list.is_empty() {
                "\u{FEFF}".to_string()
            } else {
                format!("\u{FEFF}{body}\n")
            };
            write_out_file(p, text.as_bytes())?;
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

/// The `--json` body of `related` (shared by stdout and `--out` so the two
/// cannot drift): the pretty `{"items":[{nreg,title},…]}` object over the
/// parsed links (titles already index-enriched). stdout prints exactly this
/// string plus a trailing newline; the `--out` file receives the same bytes
/// BOM-less (machine data, the house JSON rule). No links: the wrapper
/// renders `"items": []` (the `dict --json` empty style).
fn related_json_body(list: &[rada_core::related::RelatedDoc]) -> Result<String> {
    let items: Vec<serde_json::Value> = list
        .iter()
        .map(|r| serde_json::json!({ "nreg": r.nreg, "title": r.title }))
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({ "items": items }))
        .map_err(anyhow::Error::from)
}

/// The stderr note printed after `related --out` has written the plain
/// table (`n` links) to `file`.
fn related_out_note(n: usize, file: &Path) -> String {
    format!("[related] записано {n} посилань → {}", file.display())
}

/// The stderr note printed after `related --json --out` has written the JSON
/// body to `file` (`[related] записано JSON → {file}` — the same wording as
/// the `search` / `cache` / `annot list` / `resolve` / `dict` JSON notes).
fn related_json_out_note(file: &Path) -> String {
    format!("[related] записано JSON → {}", file.display())
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
                let body = annot_json_body(&items);
                match out {
                    Some(p) => {
                        // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                        write_out_file(p, format!("{body}\n").as_bytes())?;
                        eprintln!("{}", annot_json_out_note(p));
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
        AnnCmd::Search { term, json, out } => {
            let out = out.as_deref();
            // `annotations::all` returns rows id-ascending: the insertion
            // order the filtered table below preserves.
            let all = rada_db::annotations::all(&conn)?;
            // GUI favorites parity: trim + lowercase the needle once; an
            // empty (or whitespace-only) term matches every annotation.
            let needle = term.trim().to_lowercase();
            let items: Vec<rada_db::Annotation> = all
                .into_iter()
                .filter(|a| annot_search_match(a, &needle))
                .collect();
            if json {
                // the same array shape as `annot list --json`: both commands
                // print through the one shared [`annot_json_body`] builder
                let body = annot_json_body(&items);
                match out {
                    Some(p) => {
                        // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                        write_out_file(p, format!("{body}\n").as_bytes())?;
                        eprintln!("{}", annot_json_out_note(p));
                    }
                    None => println!("{body}"),
                }
                return Ok(());
            }
            let lines = annot_search_lines(&items, &term);
            match out {
                Some(p) => {
                    // UTF-8 BOM prefix: the same Excel-friendly convention as
                    // search/arrivals/resolve/related. Zero matches writes the
                    // BOM alone: the header line is the stdout-only zero-hit
                    // message (the house `search --out` rule).
                    let text = if items.is_empty() {
                        "\u{FEFF}".to_string()
                    } else {
                        format!("\u{FEFF}{}\n", lines.join("\n"))
                    };
                    write_out_file(p, text.as_bytes())?;
                    eprintln!("{}", annot_search_out_note(items.len(), p));
                }
                None => {
                    // Zero matches: the body is just the header line — it
                    // prints, keeping the output deterministic.
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
                // machine data: raw UTF-8 JSON, no BOM, nothing on stdout;
                // the file receives exactly the stdout bytes — the shared
                // `json` string plus the trailing newline `println!` adds
                // (the house JSON rule).
                write_out_file(p, format!("{json}\n").as_bytes())?;
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

/// The `--json` body of `annot list` and `annot search` (one shared builder
/// so the two commands, and stdout vs `--out` within each, cannot drift):
/// the pretty `[{id,nreg,kind,text,span_start,span_end,created_at},…]`
/// array — the field order of [`rada_db::Annotation`] itself (the
/// historical inline shape). stdout prints exactly this string plus a
/// trailing newline; the `--out` file receives the same bytes BOM-less
/// (machine data, the house JSON rule). Empty: both commands render the
/// same `[]` (the shared empty form; these field types always serialize,
/// so the fallback keeps the array shape and never fires).
fn annot_json_body(items: &[rada_db::Annotation]) -> String {
    serde_json::to_string_pretty(items).unwrap_or_else(|_| "[]".to_string())
}

/// The stderr note printed after `annot list --json --out` or
/// `annot search --json --out` has written the JSON body to `file`
/// (`[annot] записано JSON → {file}` — the same wording as the `search` /
/// `cache` / `resolve` / `related` / `dict` JSON notes).
fn annot_json_out_note(file: &Path) -> String {
    format!("[annot] записано JSON → {}", file.display())
}

/// Case-insensitive substring test behind `annot search` (the GUI
/// favorites filter, mirrored onto the three annotation surfaces): the
/// needle must occur in the annotation `text`, `kind` or `nreg`, all
/// folded with plain `to_lowercase` (folds Ukrainian letters І/і, Ї/ї,
/// Є/є, …). The caller trims and lowercases the needle once; an empty
/// needle matches every annotation.
fn annot_search_match(a: &rada_db::Annotation, needle_low: &str) -> bool {
    a.text.to_lowercase().contains(needle_low)
        || a.kind.to_lowercase().contains(needle_low)
        || a.nreg.to_lowercase().contains(needle_low)
}

/// The plain-table body of `annot search` (shared by stdout and `--out`
/// so the two cannot drift): «{n} анотацій з «{term}»:` header, then the
/// `annot list` rows through the shared [`annot_table_lines`] renderer
/// (same columns). The header always renders: zero matches is just the
/// header line on stdout, while the `--out` file carries the BOM alone
/// (the header is the stdout-only zero-hit message, the house rule).
fn annot_search_lines(items: &[rada_db::Annotation], term: &str) -> Vec<String> {
    let mut lines = vec![format!("{} анотацій з «{term}»:", items.len())];
    lines.extend(annot_table_lines(items));
    lines
}

/// The stderr note printed after `annot search --out` has written the
/// match table (`n` matched annotations) to `file`.
fn annot_search_out_note(n: usize, file: &Path) -> String {
    format!("[annot] записано {n} збігів → {}", file.display())
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

/// One `recent` row: the annotation surfaces the view renders, with the
/// title resolved from the offline index (`title_by_nreg`; empty when the
/// nreg is not indexed, then the row renders its own nreg). `created_at` is
/// the stored UTC `YYYY-MM-DD HH:MM:SS` stamp, printed verbatim like
/// `annot list` does.
struct RecentRow {
    nreg: String,
    kind: String,
    created_at: String,
    text: String,
    title: String,
}

/// Render one [`RecentRow`] as its compact two-line block: the
/// `[{kind}] {title||nreg} · {created_at}` line (the trimmed title when
/// the doc is indexed, otherwise the nreg itself) and the indented text
/// (trimmed, newlines folded to spaces, truncated to 60 chars like
/// [`annot_table_lines`]; an empty text — bookmarks — renders no text
/// line). Shared by [`recent_lines`] and [`annots_doc_lines`] so the
/// cross-document and per-document reports render rows identically.
fn recent_row_lines(r: &RecentRow) -> Vec<String> {
    let label = if r.title.trim().is_empty() {
        r.nreg.as_str()
    } else {
        r.title.trim()
    };
    let mut lines = vec![format!("[{}] {label} · {}", r.kind, r.created_at)];
    let t: String = r
        .text
        .trim()
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .take(60)
        .collect();
    if !t.is_empty() {
        lines.push(format!("  {t}"));
    }
    lines
}

/// Render one [`RecentRow`] as its `--json` object
/// (`{created_at,kind,nreg,text,title}` — map keys sort, so the shape is
/// alphabetical). Shared by [`recent_json_body`] and [`annots_doc_json`]
/// so the two JSON bodies carry identical row objects.
fn recent_row_json(r: &RecentRow) -> serde_json::Value {
    serde_json::json!({
        "nreg": r.nreg,
        "kind": r.kind,
        "created_at": r.created_at,
        "text": r.text,
        "title": r.title,
    })
}

/// The plain-table body of `recent` (shared by stdout and `--out` so the
/// two cannot drift): «Останні анотації ({n}):» header, then a compact
/// two-line block per row through the shared [`recent_row_lines`]
/// renderer. Zero rows render the single stdout-only «Останніх анотацій
/// немає» line (the house `search --out` rule): the `--out` file carries
/// the BOM alone, the message never enters it.
fn recent_lines(rows: &[RecentRow]) -> Vec<String> {
    if rows.is_empty() {
        return vec!["Останніх анотацій немає".to_string()];
    }
    let mut lines = Vec::with_capacity(1 + 2 * rows.len());
    lines.push(format!("Останні анотації ({}):", rows.len()));
    for r in rows {
        lines.extend(recent_row_lines(r));
    }
    lines
}

/// The `--json` body of `recent` (shared by stdout and `--out` so the two
/// cannot drift): the pretty `[{nreg,kind,created_at,text,title},…]` array
/// of the shared [`recent_row_json`] row objects, built as hand-rolled
/// `serde_json::Value`s like the other commands' JSON bodies (map keys
/// sort). stdout prints exactly this string plus a trailing newline; the
/// `--out` file receives the same bytes BOM-less (machine data, the house
/// JSON rule). Empty: a plain `[]`.
fn recent_json_body(rows: &[RecentRow]) -> String {
    let items: Vec<serde_json::Value> = rows.iter().map(recent_row_json).collect();
    serde_json::to_string_pretty(&serde_json::Value::Array(items))
        .unwrap_or_else(|_| "[]".to_string())
}

/// The stderr note printed after `recent --out` has written the plain
/// table (`n` annotations) to `file`.
fn recent_out_note(n: usize, file: &Path) -> String {
    format!("[recent] записано {n} анотацій → {}", file.display())
}

/// The stderr note printed after `recent --json --out` has written the JSON
/// body to `file` (`[recent] записано JSON → {file}` — the same wording as
/// the `search` / `cache` / `annot list` / `resolve` / `dict` / `related`
/// JSON notes).
fn recent_json_out_note(file: &Path) -> String {
    format!("[recent] записано JSON → {}", file.display())
}

/// Offline «what did I highlight lately» (`recent`): the newest annotations
/// across all documents, never building a client and never touching the
/// network (annotations live in the local index). A missing index file is a
/// hard error («індекс не знайдено: …», the `stats`/`verify` rule). The
/// rows come straight from the `annotations` table through the connection
/// `rada_db::open` returns (the `verify` / `dict-stats` direct-SQL idiom:
/// `prepare` + `query_map`, the CLI crate has no rusqlite dep of its own),
/// ordered `created_at DESC` with an `id DESC` tie-break, `LIMIT` clamped
/// to 1..=500; titles come from the offline index per row (the
/// `cache --search` style). With `--out` the exact stdout body goes to the
/// file (parent dirs created) and nothing prints to stdout: the plain
/// table is written UTF-8 with a BOM (Excel-friendly, like
/// search/arrivals/stats/verify), the `--json` body BOM-less; one
/// `[recent] записано …` note goes to stderr. Zero annotations write the
/// BOM alone (n=0 note) while the «Останніх анотацій немає» line stays
/// stdout-only (the house `search --out` zero-hit rule).
fn cmd_recent(db: &PathBuf, limit: usize, json: bool, out: Option<&Path>) -> Result<()> {
    if !db.exists() {
        return Err(anyhow::anyhow!("індекс не знайдено: {}", db.display()));
    }
    let limit = limit.clamp(1, 500);
    let conn = rada_db::open(db).with_context(|| format!("open db {}", db.display()))?;
    let mut stmt = conn
        .prepare(
            "SELECT nreg, kind, text, created_at FROM annotations
             ORDER BY created_at DESC, id DESC LIMIT ?1",
        )
        .context("prepare recent annotations")?;
    let mut rows = stmt
        .query_map([limit as i64], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .context("run recent annotations")?;
    let mut items: Vec<RecentRow> = Vec::new();
    while let Some((nreg, kind, text, created_at)) = rows.next().transpose()? {
        // Offline title lookup (the `export` / `cache --search` style): an
        // nreg missing from `docs` keeps the empty title, and the row then
        // renders its own nreg (the `title||nreg` rule).
        let title = rada_db::title_by_nreg(&conn, &nreg).unwrap_or_default();
        items.push(RecentRow {
            nreg,
            kind,
            created_at,
            text,
            title,
        });
    }
    if json {
        let body = recent_json_body(&items);
        match out {
            Some(p) => {
                // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                write_out_file(p, format!("{body}\n").as_bytes())?;
                eprintln!("{}", recent_json_out_note(p));
            }
            None => println!("{body}"),
        }
        return Ok(());
    }
    let lines = recent_lines(&items);
    match out {
        Some(p) => {
            // UTF-8 BOM prefix: the same Excel-friendly convention as
            // search/arrivals/stats/verify; the bytes are exactly the
            // stdout table, so a zero-annotation result carries the BOM
            // alone (the «немає» line is the stdout-only zero message,
            // the house rule).
            let text = if items.is_empty() {
                "\u{FEFF}".to_string()
            } else {
                format!("\u{FEFF}{}\n", lines.join("\n"))
            };
            write_out_file(p, text.as_bytes())?;
            eprintln!("{}", recent_out_note(items.len(), p));
        }
        None => {
            for line in lines {
                println!("{line}");
            }
        }
    }
    Ok(())
}

/// Cap the `annots --limit` option at the `docs`/`recent` row budget:
/// `0` renders one row and anything above `500` is truncated to `500`
/// (`usize::clamp`, mirroring [`docs_limit`]); 1..=500 passes through
/// untouched (no hard error).
fn annots_limit(limit: usize) -> usize {
    limit.clamp(1, 500)
}

/// Canonicalise `annots --kind` through the shared [`normalize_fav_kind`]
/// mapping: the three Ukrainian labels the CLI's annotation filters speak
/// («коментар»→comment, «виділення»→highlight, «закладка»→bookmark, the
/// plural forms included) and the English tokens the rows print, all
/// case-insensitive. An unknown kind is a clear error (exit 1 through
/// `main`, the `normalize_fmt` rule).
fn normalize_annots_kind(kind: &str) -> Result<&'static str> {
    normalize_fav_kind(kind)
}

/// The report body of `annots` (shared by stdout and `--out` so the two
/// cannot drift): «Анотації документа {nreg}:» header, then one
/// `recent`-style two-line block per row through the shared
/// [`recent_row_lines`] renderer (input order — the SQL orders
/// `created_at DESC, id DESC`, the exact `recent-annots` row order), then
/// the verdict «разом: {count}» over the RENDERED rows (post-`--limit`,
/// the `recent` rule). Informational: a document without annotations (or
/// an unknown nreg) renders header + «немає анотацій» + «разом: 0» and
/// the run still exits 0 (the `docs` zero-rows rule — the header always
/// renders, so the `--out` file is never BOM-alone).
fn annots_doc_lines(nreg: &str, rows: &[RecentRow]) -> Vec<String> {
    let mut lines = vec![format!("Анотації документа {nreg}:")];
    if rows.is_empty() {
        lines.push("немає анотацій".to_string());
    }
    for r in rows {
        lines.extend(recent_row_lines(r));
    }
    lines.push(format!("разом: {}", rows.len()));
    lines
}

/// The `--json` body of `annots` (shared by stdout and `--out` so the two
/// cannot drift): the pretty
/// `{"annots":[{created_at,kind,nreg,text,title},…],"doc":nreg,"total":N}`
/// object over the rendered rows (the `docs`/`favorites` envelope idiom);
/// the row objects come from the shared [`recent_row_json`], so they
/// match `recent-annots` exactly. Map keys sort, so «annots» < «doc» <
/// «total» and the row keys keep their alphabetical order inside every
/// entry; `total` counts the LISTED rows (the «разом» rule,
/// post-`--limit`). stdout prints exactly this string plus a trailing
/// newline; the `--out` file receives the same bytes BOM-less (machine
/// data, the house JSON rule).
fn annots_doc_json(nreg: &str, rows: &[RecentRow]) -> String {
    let items: Vec<serde_json::Value> = rows.iter().map(recent_row_json).collect();
    let value = serde_json::json!({
        "annots": items,
        "doc": nreg,
        "total": rows.len(),
    });
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string())
}

/// The stderr note printed after `annots --out` has written `file`
/// (`n` rendered annotation rows, the same wording in both modes, the
/// [`docs_list_out_note`] style): `[annots] записано {n} анотацій → {file}`.
fn annots_doc_out_note(n: usize, file: &Path) -> String {
    format!("[annots] записано {n} анотацій → {}", file.display())
}

/// Offline «what did I annotate in this document» (`annots`): the
/// per-document annotation list — the drill-down companion of
/// [`cmd_recent`] (the `docs`-to-`orgs` relationship, onto annotations).
/// Never builds a client, never touches the network. A missing index
/// file is a hard error («індекс не знайдено: …», the `stats`/`verify`
/// rule); `--kind` passes the shared [`normalize_annots_kind`] (an
/// unknown kind exits 1 with the house validation message) and
/// `--limit` [`annots_limit`] (clamped to 1..=500, never an error). The
/// rows come straight from the `annotations` table through the
/// connection `rada_db::open` returns (the `recent` direct-SQL idiom):
/// `nreg` and the canonical kind token bound as parameters, one
/// statement serving both modes — `(?2 = '' OR kind = ?2)` with the
/// empty string as the no-filter sentinel (the table's `CHECK` pins kind
/// to the three nonempty tokens, so it can never match a row); `LIMIT`
/// interpolated from the clamped `usize` (ASCII digits only — the same
/// interpolation-safety reasoning as the `docs` limit). Rows are ordered
/// exactly like `recent-annots`: `created_at DESC` with an `id DESC`
/// tie-break. The title is resolved once per run (every row shares the
/// nreg; `title_by_nreg`, the `export` / `cache --search` style — a miss
/// keeps the empty title and the rows render their own nreg, the
/// `title||nreg` rule). Informational: a document without annotations
/// (or an unknown nreg) exits 0 with the zero-rows report. With `--out`
/// the exact stdout body goes to the file (parent dirs created) and
/// nothing prints to stdout: the plain report is written UTF-8 with a
/// BOM (Excel-friendly, like search/arrivals/stats/verify), the `--json`
/// body BOM-less; one `[annots] записано {n} анотацій → {file}` note
/// goes to stderr. The header always renders, so the file is never
/// BOM-alone.
fn cmd_annots(
    db: &PathBuf,
    nreg: &str,
    kind: Option<&str>,
    limit: usize,
    json: bool,
    out: Option<&Path>,
) -> Result<()> {
    if !db.exists() {
        return Err(anyhow::anyhow!("індекс не знайдено: {}", db.display()));
    }
    // the shared `favorites --kind` validator: absent keeps every kind,
    // present must fold to one of the three canonical tokens
    let kind = match kind {
        Some(k) => Some(normalize_annots_kind(k)?),
        None => None,
    };
    let limit = annots_limit(limit);
    let conn = rada_db::open(db).with_context(|| format!("open db {}", db.display()))?;
    // one statement for both modes: `nreg` and the canonical kind token
    // are bound as parameters (never interpolated); the empty `kind`
    // disables the kind test (the CHECK constraint keeps stored kinds
    // nonempty); `limit` is interpolated only after `annots_limit` — a
    // clamped usize renders as plain ASCII digits, no SQL metacharacters
    // possible
    let sql = format!(
        "SELECT nreg, kind, text, created_at FROM annotations
         WHERE nreg = ?1 AND (?2 = '' OR kind = ?2)
         ORDER BY created_at DESC, id DESC LIMIT {limit}"
    );
    let mut stmt = conn
        .prepare(&sql)
        .context("prepare annots list")?;
    // one offline title lookup for the whole run: every row belongs to
    // the same nreg (the `recent` per-row lookup degenerates here); an
    // nreg missing from `docs` keeps the empty title and the rows then
    // render their own nreg (the `title||nreg` rule)
    let title = rada_db::title_by_nreg(&conn, nreg).unwrap_or_default();
    let kind_tok = kind.unwrap_or("");
    let mut rows = stmt
        .query_map([nreg, kind_tok], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .context("run annots list")?;
    let mut items: Vec<RecentRow> = Vec::new();
    while let Some((row_nreg, row_kind, text, created_at)) = rows.next().transpose()? {
        items.push(RecentRow {
            nreg: row_nreg,
            kind: row_kind,
            created_at,
            text,
            title: title.clone(),
        });
    }
    if json {
        let body = annots_doc_json(nreg, &items);
        match out {
            Some(p) => {
                // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                write_out_file(p, format!("{body}\n").as_bytes())?;
                eprintln!("{}", annots_doc_out_note(items.len(), p));
            }
            None => println!("{body}"),
        }
        return Ok(());
    }
    let lines = annots_doc_lines(nreg, &items);
    match out {
        Some(p) => {
            // UTF-8 BOM prefix: the same Excel-friendly convention as
            // search/arrivals/stats/verify; the bytes are exactly the
            // stdout report (the header always renders, so never
            // BOM-alone).
            write_out_file(p, format!("\u{FEFF}{}\n", lines.join("\n")).as_bytes())?;
            eprintln!("{}", annots_doc_out_note(items.len(), p));
        }
        None => {
            for line in lines {
                println!("{line}");
            }
        }
    }
    Ok(())
}

/// Sort order behind `favorites --sort` (the GUI favorites' grouped view
/// and its sort choices, projected onto the CLI): [`FavSort::Nreg`] is the
/// default — one group per document like the GUI grouped render;
/// [`FavSort::Title`] orders by the resolved title (the GUI «за назвою»);
/// [`FavSort::New`] is the flat `created_at DESC` stream.
#[derive(Clone, Copy)]
enum FavSort {
    Nreg,
    Title,
    New,
}

/// Parse `--sort` for `favorites` (the `normalize_fmt` house rule: a clear
/// Ukrainian error listing the accepted values, exit 1 through `main`).
fn parse_fav_sort(s: &str) -> Result<FavSort> {
    match s.trim().to_ascii_lowercase().as_str() {
        "nreg" => Ok(FavSort::Nreg),
        "title" => Ok(FavSort::Title),
        "new" => Ok(FavSort::New),
        _ => Err(anyhow::anyhow!("невідомий порядок сортування {s} (nreg|title|new)")),
    }
}

/// Canonicalise `--kind` for `favorites` to the stored English token (the
/// same mapping the GUI chips apply: «коментарі»→comment,
/// «виділення»→highlight, «закладки»→bookmark; singular and plural
/// Ukrainian forms are accepted). Case-insensitive; an unknown kind is a
/// clear error listing the accepted values (the `normalize_fmt` rule).
fn normalize_fav_kind(kind: &str) -> Result<&'static str> {
    match kind.trim().to_lowercase().as_str() {
        "comment" | "коментар" | "коментарі" => Ok("comment"),
        "highlight" | "виділення" => Ok("highlight"),
        "bookmark" | "закладка" | "закладки" => Ok("bookmark"),
        _ => Err(anyhow::anyhow!(
            "невідомий тип анотації {kind} (comment|highlight|bookmark)"
        )),
    }
}

/// One `favorites` row: the annotation surfaces the view renders, with the
/// title resolved from the offline index (`title_by_nreg`; empty when the
/// nreg is not indexed, then the row renders its own nreg — the same
/// `title||nreg` rule as [`RecentRow`]). `created_at` is the stored UTC
/// `YYYY-MM-DD HH:MM:SS` stamp, printed verbatim.
#[derive(Clone)]
struct FavRow {
    nreg: String,
    kind: String,
    text: String,
    created_at: String,
    title: String,
}

/// The row label shared by both render modes: the trimmed title when the
/// doc is indexed, otherwise the nreg itself (the `recent` rule).
fn fav_label(r: &FavRow) -> &str {
    if r.title.trim().is_empty() {
        r.nreg.as_str()
    } else {
        r.title.trim()
    }
}

/// Fold one annotation text into a single printable line: trimmed, newlines
/// folded to spaces, capped at 60 chars (the shared `annot list` / `recent`
/// style); an empty text (bookmarks) renders as an empty string.
fn fav_text_line(text: &str) -> String {
    text.trim()
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .take(60)
        .collect()
}

/// Reorder the rows for one `--sort` choice (stable, so ties keep the base
/// `nreg ASC, id ASC` order the query returns): `nreg` sorts by the nreg
/// itself (groups stay together, `id ASC` inside each), `title` sorts by
/// the resolved `title||nreg` label then the nreg (the GUI «за назвою»), `new` flips to `created_at` descending. Shared by
/// both renderers so the table and the JSON body can never disagree.
fn favorites_sorted(rows: &[FavRow], sort: FavSort) -> Vec<FavRow> {
    let mut rows = rows.to_vec();
    match sort {
        // stable: ties keep the query's id order inside each nreg group
        FavSort::Nreg => rows.sort_by(|a, b| a.nreg.cmp(&b.nreg)),
        FavSort::Title => rows.sort_by(|a, b| {
            fav_label(a)
                .cmp(fav_label(b))
                .then_with(|| a.nreg.cmp(&b.nreg))
        }),
        FavSort::New => rows.sort_by(|a, b| b.created_at.cmp(&a.created_at)),
    }
    rows
}

/// The plain-table body of `favorites` (shared by stdout and `--out` so the
/// two cannot drift): «Обрані анотації ({n}):» header (the `recent_lines`
/// header style), then either — `nreg` sort (the default, the GUI grouped
/// view): a `{title||nreg}:` group header per document with the rows
/// indented under it (`  [{kind}] {created_at}` plus the doubly-indented
/// text line, empty texts rendering none, the `recent_lines` text rule);
/// or — `title` / `new` sorts: a flat list rendered exactly like
/// [`recent_lines`] (`[{kind}] {title||nreg} · {created_at}` + the indented
/// text). Rows are reordered through [`favorites_sorted`] first, so the
/// body always matches the requested order. Zero rows render the single
/// stdout-only «Обраних анотацій немає» line (the house `search --out`
/// rule): the `--out` file carries the BOM alone, the message never enters
/// it.
fn favorites_lines(rows: &[FavRow], sort: FavSort) -> Vec<String> {
    if rows.is_empty() {
        return vec!["Обраних анотацій немає".to_string()];
    }
    let rows = favorites_sorted(rows, sort);
    let mut lines = Vec::with_capacity(1 + 2 * rows.len());
    lines.push(format!("Обрані анотації ({}):", rows.len()));
    match sort {
        FavSort::Nreg => {
            let mut group: Option<&str> = None;
            for r in &rows {
                if group != Some(r.nreg.as_str()) {
                    lines.push(format!("{}:", fav_label(r)));
                    group = Some(r.nreg.as_str());
                }
                lines.push(format!("  [{}] {}", r.kind, r.created_at));
                let t = fav_text_line(&r.text);
                if !t.is_empty() {
                    lines.push(format!("    {t}"));
                }
            }
        }
        FavSort::Title | FavSort::New => {
            for r in &rows {
                lines.push(format!("[{}] {} · {}", r.kind, fav_label(r), r.created_at));
                let t = fav_text_line(&r.text);
                if !t.is_empty() {
                    lines.push(format!("  {t}"));
                }
            }
        }
    }
    lines
}

/// One JSON annotation object (the `recent_json_body` field set; map keys
/// sort, the house JSON style).
fn fav_json_item(r: &FavRow) -> serde_json::Value {
    serde_json::json!({
        "nreg": r.nreg,
        "kind": r.kind,
        "created_at": r.created_at,
        "text": r.text,
        "title": r.title,
    })
}

/// The `--json` body of `favorites` (shared by stdout and `--out` so the
/// two cannot drift): `nreg` sort (grouped) renders
/// `{"grouped":{nreg:[{…},…]…},"total":N}` — the per-document grouping of
/// the default table; `title` / `new` sorts render the flat
/// `{"items":[{…}…],"total":N}`. Rows are reordered through
/// [`favorites_sorted`] first, so the body matches the table. Pretty,
/// BOM-less in both modes (the house JSON rule). Empty:
/// `{"grouped":{},"total":0}` / `{"items":[],"total":0}`.
fn favorites_json_body(rows: &[FavRow], sort: FavSort) -> String {
    let rows = favorites_sorted(rows, sort);
    let body = match sort {
        FavSort::Nreg => {
            let mut grouped = serde_json::Map::new();
            for r in &rows {
                grouped
                    .entry(r.nreg.clone())
                    .or_insert_with(|| serde_json::Value::Array(Vec::new()))
                    .as_array_mut()
                    .expect("group inserted as an array")
                    .push(fav_json_item(r));
            }
            serde_json::json!({ "grouped": grouped, "total": rows.len() })
        }
        FavSort::Title | FavSort::New => serde_json::json!({
            "items": rows.iter().map(fav_json_item).collect::<Vec<_>>(),
            "total": rows.len(),
        }),
    };
    serde_json::to_string_pretty(&body).unwrap_or_else(|_| "{}".to_string())
}

/// The stderr note printed after `favorites --out` has written the plain
/// table (`n` annotations) to `file`.
fn favorites_out_note(n: usize, file: &Path) -> String {
    format!("[favorites] записано {n} анотацій → {}", file.display())
}

/// The stderr note printed after `favorites --json --out` has written the
/// JSON body to `file` (`[favorites] записано JSON → {file}` — the same
/// wording as the `search` / `cache` / `annot list` / `recent` JSON notes).
fn favorites_json_out_note(file: &Path) -> String {
    format!("[favorites] записано JSON → {}", file.display())
}

/// Offline Favorites (`favorites`): every annotation across all documents —
/// the CLI mirror of the GUI/web Favorites list — never building a client
/// and never touching the network (annotations live in the local index).
/// A missing index file is a hard error («індекс не знайдено: …», the
/// `stats`/`verify`/`recent` rule). The rows come straight from the
/// `annotations` table through the connection `rada_db::open` returns (the
/// `verify` / `recent` direct-SQL idiom), base-ordered `nreg ASC, id ASC`
/// so the default grouping keeps documents together with `id ASC` inside
/// each group; `--kind` filters case-insensitively against the canonical
/// English token (`normalize_fav_kind` mirrors the GUI chip mapping), and
/// `--sort title|new` reorders in memory (stable sorts keep the base order
/// within ties). Titles come from the offline index once per distinct
/// nreg (a `HashMap` cache over the `cache --search` / `recent` lookup).
/// With `--out` the exact stdout body goes to the file (parent dirs
/// created) and nothing prints to stdout: the plain table is written UTF-8
/// with a BOM (Excel-friendly, like search/arrivals/stats/verify), the
/// `--json` body BOM-less; one `[favorites] записано …` note goes to
/// stderr. Zero annotations write the BOM alone (n=0 note) while the
/// «Обраних анотацій немає» line stays stdout-only (the house `search
/// --out` zero-hit rule).
fn cmd_favorites(
    db: &PathBuf,
    kind: Option<&str>,
    sort: &str,
    json: bool,
    out: Option<&Path>,
) -> Result<()> {
    if !db.exists() {
        return Err(anyhow::anyhow!("індекс не знайдено: {}", db.display()));
    }
    // Validate both selectors before opening the database (clear errors,
    // exit 1 through `main`, nothing printed).
    let kind = kind.map(normalize_fav_kind).transpose()?;
    let sort = parse_fav_sort(sort)?;
    let conn = rada_db::open(db).with_context(|| format!("open db {}", db.display()))?;
    let mut stmt = conn
        .prepare(
            "SELECT nreg, kind, text, created_at FROM annotations
             ORDER BY nreg ASC, id ASC",
        )
        .context("prepare favorites annotations")?;
    let mut rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .context("run favorites annotations")?;
    // Offline title lookup cached per distinct nreg (the `recent` idiom,
    // but one lookup per document instead of one per row); an nreg missing
    // from `docs` keeps the empty title, and the row renders its own nreg.
    let mut titles: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut items: Vec<FavRow> = Vec::new();
    while let Some((nreg, k, text, created_at)) = rows.next().transpose()? {
        if let Some(want) = kind {
            if !k.eq_ignore_ascii_case(want) {
                continue;
            }
        }
        let title = titles
            .entry(nreg.clone())
            .or_insert_with(|| rada_db::title_by_nreg(&conn, &nreg).unwrap_or_default())
            .clone();
        items.push(FavRow {
            nreg,
            kind: k,
            text,
            created_at,
            title,
        });
    }
    // Reorder per --sort lives in the pure [`favorites_sorted`] helper the
    // two renderers share (stable sorts keep the base order within ties).
    if json {
        let body = favorites_json_body(&items, sort);
        match out {
            Some(p) => {
                // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                write_out_file(p, format!("{body}\n").as_bytes())?;
                eprintln!("{}", favorites_json_out_note(p));
            }
            None => println!("{body}"),
        }
        return Ok(());
    }
    let lines = favorites_lines(&items, sort);
    match out {
        Some(p) => {
            // UTF-8 BOM prefix: the same Excel-friendly convention as
            // search/arrivals/stats/verify; the bytes are exactly the
            // stdout table, so a zero-annotation result carries the BOM
            // alone (the «немає» line is the stdout-only zero message,
            // the house rule).
            let text = if items.is_empty() {
                "\u{FEFF}".to_string()
            } else {
                format!("\u{FEFF}{}\n", lines.join("\n"))
            };
            write_out_file(p, text.as_bytes())?;
            eprintln!("{}", favorites_out_note(items.len(), p));
        }
        None => {
            for line in lines {
                println!("{line}");
            }
        }
    }
    Ok(())
}

/// One `dates` histogram row: the four-digit year prefix of the primary
/// date column (`org_date` — the same column the
/// `search --date-from/--date-to` filters compare against) and the number
/// of documents carrying it.
struct YearRow {
    year: String,
    count: i64,
}

/// The report body of `dates` (shared by stdout and `--out` so the two
/// cannot drift): «Розподіл за роками (дата прийняття):» header, then one
/// `{year}: {count}` line per year ascending (counts render plain, like
/// the `stats`/`verify` numbers — the CLI has no digit-grouping helper to
/// copy), then the «без дати: {undated}» line only when `undated > 0`,
/// then the verdict: «найновіший рік: {max_year}», or «індекс порожній»
/// when the histogram has no rows at all. Informational: an empty
/// histogram never fails the run.
fn dates_lines(rows: &[YearRow], total: i64, undated: i64) -> Vec<String> {
    // `total` joins the signature as the shared report contract with
    // [`dates_json`]; the plain body renders only the per-year counts.
    let _ = total;
    let mut lines = vec!["Розподіл за роками (дата прийняття):".to_string()];
    for r in rows {
        lines.push(format!("{}: {}", r.year, r.count));
    }
    if undated > 0 {
        lines.push(format!("без дати: {undated}"));
    }
    match rows.iter().map(|r| r.year.as_str()).max() {
        Some(max) => lines.push(format!("найновіший рік: {max}")),
        None => lines.push("індекс порожній".to_string()),
    }
    lines
}

/// The `--json` body of `dates` (shared by stdout and `--out` so the two
/// cannot drift): the pretty
/// `{"years":[{year,count},…],"undated":N,"total":N,"newest":"YYYY"|null}`
/// object built as hand-rolled `serde_json::Value`s like the other
/// commands' JSON bodies (map keys sort, so `newest`/`total`/`undated`/
/// `years` keep their alphabetical order; `newest` is `null` for an empty
/// histogram). stdout prints exactly this string plus a trailing newline;
/// the `--out` file receives the same bytes BOM-less (machine data, the
/// house JSON rule).
fn dates_json(rows: &[YearRow], total: i64, undated: i64) -> String {
    let years: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| serde_json::json!({ "year": r.year, "count": r.count }))
        .collect();
    let value = serde_json::json!({
        "years": years,
        "undated": undated,
        "total": total,
        "newest": rows.iter().map(|r| r.year.as_str()).max(),
    });
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string())
}

/// The stderr note printed after `dates --out` has written `file`
/// (`years` year rows, the same wording in both modes):
/// `[dates] записано {years} років → {file}`.
fn dates_out_note(years: usize, file: &Path) -> String {
    format!("[dates] записано {years} років → {}", file.display())
}

/// Offline «how fresh is my index» (`dates`): the document-date
/// distribution by year of `org_date` — the SAME adoption-date column the
/// `search --date-from/--date-to` filters compare against (see
/// `rada_db::search`), so the histogram and the filters always agree.
/// Never builds a client, never touches the network. A missing index file
/// is a hard error («індекс не знайдено: …», the `stats`/`verify` rule).
/// The histogram comes straight from `docs` through the connection
/// `rada_db::open` returns (the `verify` / `dict-stats` direct-SQL idiom):
/// `substr(org_date,1,4)` grouped over the rows passing the
/// `GLOB '[0-9][0-9][0-9][0-9]*'` year-prefix guard, ordered ascending.
/// Documents with a missing or malformed date (the empty-string default)
/// are the `total` minus the histogram sum and render as «без дати».
/// Informational: the run always exits 0. With `--out` the exact stdout
/// body goes to the file (parent dirs created) and nothing prints to
/// stdout: the plain report is written UTF-8 with a BOM (Excel-friendly,
/// like search/arrivals/stats/verify), the `--json` body BOM-less; one
/// `[dates] записано {years} років → {file}` note goes to stderr. The
/// header always renders, so the file is never BOM-alone.
fn cmd_dates(db: &PathBuf, json: bool, out: Option<&Path>) -> Result<()> {
    if !db.exists() {
        return Err(anyhow::anyhow!("індекс не знайдено: {}", db.display()));
    }
    let conn = rada_db::open(db).with_context(|| format!("open db {}", db.display()))?;
    let total: i64 = conn
        .query_row("SELECT COUNT(*) FROM docs", [], |r| r.get(0))
        .unwrap_or(0);
    let mut rows: Vec<YearRow> = Vec::new();
    let mut stmt = conn
        .prepare(
            "SELECT substr(org_date,1,4) AS y, COUNT(*) FROM docs
             WHERE org_date GLOB '[0-9][0-9][0-9][0-9]*'
             GROUP BY y ORDER BY y",
        )
        .context("prepare dates histogram")?;
    let mut mapped = stmt
        .query_map([], |r| {
            Ok(YearRow {
                year: r.get(0)?,
                count: r.get(1)?,
            })
        })
        .context("run dates histogram")?;
    while let Some(row) = mapped.next().transpose()? {
        rows.push(row);
    }
    let dated: i64 = rows.iter().map(|r| r.count).sum();
    let undated = total - dated;
    if json {
        let body = dates_json(&rows, total, undated);
        match out {
            Some(p) => {
                // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                write_out_file(p, format!("{body}\n").as_bytes())?;
                eprintln!("{}", dates_out_note(rows.len(), p));
            }
            None => println!("{body}"),
        }
        return Ok(());
    }
    let lines = dates_lines(&rows, total, undated);
    match out {
        Some(p) => {
            // UTF-8 BOM prefix: the same Excel-friendly convention as
            // search/arrivals/stats/verify; the bytes are exactly the
            // stdout report (the header always renders, so never
            // BOM-alone).
            write_out_file(p, format!("\u{FEFF}{}\n", lines.join("\n")).as_bytes())?;
            eprintln!("{}", dates_out_note(rows.len(), p));
        }
        None => {
            for line in lines {
                println!("{line}");
            }
        }
    }
    Ok(())
}

/// One `months` histogram row: the two-digit month substring
/// (`substr(org_date,5,2)`, `'01'..='12'`) within the selected year of
/// the primary date column (`org_date` — the same column the
/// `search --date-from/--date-to` filters compare against) and the
/// number of documents carrying it.
struct MonthRow {
    month: String,
    count: i64,
}

/// Validate the `months --year` option (the `normalize_hits_format`
/// house rule: a clear Ukrainian error, exit 1 through `main`): absent
/// means "the newest year present in the index" (`Ok(None)` — the
/// `dates` verdict rule the caller resolves via SQL); present must be
/// exactly four ASCII digits (`"2026"`, surrounding whitespace
/// trimmed), anything else — `"26"`, `"abcd"`, the empty string — fails
/// with «рік має бути РРРР (наприклад 2026)».
fn normalize_month_year(year: Option<&str>) -> Result<Option<String>> {
    match year {
        None => Ok(None),
        Some(y) => {
            let y = y.trim();
            if y.len() == 4 && y.bytes().all(|b| b.is_ascii_digit()) {
                Ok(Some(y.to_string()))
            } else {
                Err(anyhow::anyhow!("рік має бути РРРР (наприклад 2026)"))
            }
        }
    }
}

/// The report body of `months` (shared by stdout and `--out` so the two
/// cannot drift): «Документи {year} за місяцями:» header, then one
/// `{year}-{month}: {count}` line for EVERY month `'01'..='12'` — a
/// month the histogram has no row for is zero-filled by the renderer
/// (the stable 12-line grid decision documented in the `--help`) —
/// counts render plain like the `dates`/`stats` numbers; then the
/// verdict «разом: {sum}» over the twelve rendered counts (the
/// documents of the year with a full 8-digit date, NOT the whole
/// index). Informational: an all-zero year still renders the header,
/// the 12 zero rows and «разом: 0», and the run exits 0.
fn months_lines(year: &str, rows: &[MonthRow]) -> Vec<String> {
    let mut lines = vec![format!("Документи {year} за місяцями:")];
    let mut total: i64 = 0;
    for m in 1..=12 {
        let month = format!("{m:02}");
        let count = rows
            .iter()
            .find(|r| r.month == month)
            .map(|r| r.count)
            .unwrap_or(0);
        total += count;
        lines.push(format!("{year}-{month}: {count}"));
    }
    lines.push(format!("разом: {total}"));
    lines
}

/// The `--json` body of `months` (shared by stdout and `--out` so the
/// two cannot drift): the pretty
/// `{"months":[{count,month},…],"total":N,"year":"YYYY"}` object built
/// as hand-rolled `serde_json::Value`s like the other commands' JSON
/// bodies (map keys sort, so `months`/`total`/`year` and the
/// `count`/`month` inside every entry keep their alphabetical order).
/// The twelve entries are the zero-filled `'01'..='12'` grid (the same
/// stable-grid decision as [`months_lines`]); `total` is the sum of the
/// twelve counts. stdout prints exactly this string plus a trailing
/// newline; the `--out` file receives the same bytes BOM-less (machine
/// data, the house JSON rule).
fn months_json(year: &str, rows: &[MonthRow]) -> String {
    let mut total: i64 = 0;
    let months: Vec<serde_json::Value> = (1..=12)
        .map(|m| {
            let month = format!("{m:02}");
            let count = rows
                .iter()
                .find(|r| r.month == month)
                .map(|r| r.count)
                .unwrap_or(0);
            total += count;
            serde_json::json!({ "month": month, "count": count })
        })
        .collect();
    let value = serde_json::json!({ "months": months, "total": total, "year": year });
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string())
}

/// The stderr note printed after `months --out` has written `file`
/// (the histogrammed `year`, the same wording in both modes):
/// `[months] записано {year} рік → {file}`.
fn months_out_note(year: &str, file: &Path) -> String {
    format!("[months] записано {year} рік → {}", file.display())
}

/// Offline «how is one year distributed» (`months`): the month-by-month
/// document-date distribution of `org_date` within ONE year — the
/// month-level companion of `dates` (which stops at year granularity),
/// so `dates`' year count and this grid's «разом» agree whenever the
/// dates are well-formed 8-digit values. Never builds a client, never
/// touches the network. A missing index file is a hard error
/// («індекс не знайдено: …», the `stats`/`verify` rule). The year comes
/// from `--year` after [`normalize_month_year`] (a malformed value is a
/// hard error, exit 1); absent, it defaults to the newest year of the
/// index — `MAX(substr(org_date,1,4))` over the same
/// `GLOB '[0-9][0-9][0-9][0-9]*'` year-prefix guard as `dates` (an
/// index without a single dated row falls back to `0000`, which renders
/// the all-zero grid). The histogram comes straight from `docs` through
/// the connection `rada_db::open` returns (the `verify` / `dict-stats`
/// direct-SQL idiom): `substr(org_date,5,2)` grouped over the rows
/// passing `GLOB '{year}[0-9][0-9][0-9][0-9]'` — the four-digit year
/// plus the four `MMDD` digits of the 8-digit `YYYYMMDD` contract (the
/// year is validated four-digit, so the interpolation is safe), ordered
/// ascending; the renderer zero-fills the missing months. Informational:
/// the run always exits 0. With `--out` the exact stdout body goes to
/// the file (parent dirs created) and nothing prints to stdout: the
/// plain report is written UTF-8 with a BOM (Excel-friendly, like
/// search/arrivals/stats/verify), the `--json` body BOM-less; one
/// `[months] записано {year} рік → {file}` note goes to stderr. The
/// header always renders, so the file is never BOM-alone.
fn cmd_months(db: &PathBuf, year: Option<&str>, json: bool, out: Option<&Path>) -> Result<()> {
    if !db.exists() {
        return Err(anyhow::anyhow!("індекс не знайдено: {}", db.display()));
    }
    let year = normalize_month_year(year)?;
    let conn = rada_db::open(db).with_context(|| format!("open db {}", db.display()))?;
    // Default year: the newest one present in the index (the `dates`
    // verdict rule — same GLOB guard, MAX over the four-digit prefix);
    // an index with no dated row at all reports the all-zero `0000`.
    let year: String = match year {
        Some(y) => y,
        None => conn
            .query_row(
                "SELECT MAX(substr(org_date,1,4)) FROM docs
                 WHERE org_date GLOB '[0-9][0-9][0-9][0-9]*'",
                [],
                |r| r.get::<_, Option<String>>(0),
            )
            .unwrap_or(None)
            .unwrap_or_else(|| "0000".to_string()),
    };
    let mut rows: Vec<MonthRow> = Vec::new();
    let mut stmt = conn
        .prepare(&format!(
            "SELECT substr(org_date,5,2) AS m, COUNT(*) FROM docs
             WHERE org_date GLOB '{year}[0-9][0-9][0-9][0-9]'
             GROUP BY m ORDER BY m",
        ))
        .context("prepare months histogram")?;
    let mut mapped = stmt
        .query_map([], |r| {
            Ok(MonthRow {
                month: r.get(0)?,
                count: r.get(1)?,
            })
        })
        .context("run months histogram")?;
    while let Some(row) = mapped.next().transpose()? {
        rows.push(row);
    }
    if json {
        let body = months_json(&year, &rows);
        match out {
            Some(p) => {
                // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                write_out_file(p, format!("{body}\n").as_bytes())?;
                eprintln!("{}", months_out_note(&year, p));
            }
            None => println!("{body}"),
        }
        return Ok(());
    }
    let lines = months_lines(&year, &rows);
    match out {
        Some(p) => {
            // UTF-8 BOM prefix: the same Excel-friendly convention as
            // search/arrivals/stats/verify; the bytes are exactly the
            // stdout report (the header always renders, so never
            // BOM-alone).
            write_out_file(p, format!("\u{FEFF}{}\n", lines.join("\n")).as_bytes())?;
            eprintln!("{}", months_out_note(&year, p));
        }
        None => {
            for line in lines {
                println!("{line}");
            }
        }
    }
    Ok(())
}

/// Validate one `years --from` / `years --to` option (the
/// `normalize_month_year` house rule: a clear Ukrainian error, exit 1
/// through `main`): absent means "the caller resolves the bound from the
/// index" (`Ok(0)` — the sentinel year `0000` never occurs in a real
/// index, so the caller substitutes the `dates` MIN/MAX); present must be
/// exactly four ASCII digits (`"2026"`, surrounding whitespace trimmed),
/// anything else — `"26"`, `"abcd"`, the empty string — fails with
/// «рік має бути РРРР (наприклад 2026)».
fn normalize_year_arg(arg: Option<&str>) -> Result<u32> {
    match arg {
        None => Ok(0),
        Some(y) => {
            let y = y.trim();
            if y.len() == 4 && y.bytes().all(|b| b.is_ascii_digit()) {
                Ok(y.parse::<u32>().unwrap_or(0))
            } else {
                Err(anyhow::anyhow!("рік має бути РРРР (наприклад 2026)"))
            }
        }
    }
}

/// Validate the resolved closed year range of `years` (the same house
/// rule): the bounds must satisfy `from <= to` — a `--from` later than
/// the `--to` (either side explicit or defaulted) fails with
/// «початок періоду пізніший за кінець» (exit 1 through `main`).
fn check_year_range(from: u32, to: u32) -> Result<()> {
    if from > to {
        Err(anyhow::anyhow!("початок періоду пізніший за кінець"))
    } else {
        Ok(())
    }
}

/// Keep the histogram rows the closed range `[from, to]` holds: a row
/// survives when its `year` is exactly four digits that parse as a number
/// (`dates`' GLOB already guards this, the parse is the belt) and the
/// parsed year lies inside the range; the survivors sort ascending.
/// Shared by [`years_lines`] and [`years_json`] so the two cannot drift.
fn years_range_rows(from: u32, to: u32, rows: &[YearRow]) -> Vec<YearRow> {
    let mut kept: Vec<YearRow> = rows
        .iter()
        .filter_map(|r| {
            let y = r.year.as_str();
            if y.len() == 4 && y.bytes().all(|b| b.is_ascii_digit()) {
                let n = y.parse::<u32>().ok()?;
                ((from..=to).contains(&n)).then(|| YearRow {
                    year: r.year.clone(),
                    count: r.count,
                })
            } else {
                None
            }
        })
        .collect();
    kept.sort_by_key(|r| r.year.clone());
    kept
}

/// The report body of `years` (shared by stdout and `--out` so the two
/// cannot drift): «Документи за роками:» header, then — when the closed
/// range `[from, to]` holds no histogram row — «немає документів за цей
/// період» (the `recent` zero rule); then one `{yyyy}: {count}` line per
/// year the range holds, ascending (counts render plain like the
/// `dates`/`stats` numbers). Unlike `months` the grid is NOT zero-filled:
/// a year inside the range without documents simply has no row. Then the
/// verdict «разом: {sum}» over the SHOWN rows, then the «без дати:
/// {undated}» line only when `undated > 0` — the `dates` line, computed
/// over the WHOLE index, independent of the range. Informational: an
/// empty range still renders header + «немає документів за цей період» +
/// «разом: 0» (+ the undated line), and the run exits 0.
fn years_lines(from: u32, to: u32, rows: &[YearRow], undated: i64) -> Vec<String> {
    let kept = years_range_rows(from, to, rows);
    let mut lines = vec!["Документи за роками:".to_string()];
    if kept.is_empty() {
        lines.push("немає документів за цей період".to_string());
    }
    let mut total: i64 = 0;
    for r in &kept {
        total += r.count;
        lines.push(format!("{}: {}", r.year, r.count));
    }
    lines.push(format!("разом: {total}"));
    if undated > 0 {
        lines.push(format!("без дати: {undated}"));
    }
    lines
}

/// The `--json` body of `years` (shared by stdout and `--out` so the two
/// cannot drift): the pretty
/// `{"count_total":N,"from":"YYYY","rows":[{count,year},…],"to":"YYYY",
/// "undated":N}` object built as hand-rolled `serde_json::Value`s like
/// the other commands' JSON bodies (map keys sort, so `count_total`/
/// `from`/`rows`/`to`/`undated` and the `count`/`year` inside every entry
/// keep their alphabetical order — documented in `--help`). `from`/`to`
/// are the resolved closed bounds (zero-padded `YYYY`); `rows` are the
/// in-range rows only (the `years_lines` omit-zero-years rule);
/// `count_total` sums the SHOWN rows; `undated` is the whole-index `dates`
/// count. stdout prints exactly this string plus a trailing newline; the
/// `--out` file receives the same bytes BOM-less (machine data, the house
/// JSON rule).
fn years_json(from: u32, to: u32, rows: &[YearRow], undated: i64) -> String {
    let kept = years_range_rows(from, to, rows);
    let total: i64 = kept.iter().map(|r| r.count).sum();
    let entries: Vec<serde_json::Value> = kept
        .iter()
        .map(|r| serde_json::json!({ "year": r.year, "count": r.count }))
        .collect();
    let value = serde_json::json!({
        "rows": entries,
        "count_total": total,
        "from": format!("{from:04}"),
        "to": format!("{to:04}"),
        "undated": undated,
    });
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string())
}

/// The stderr note printed after `years --out` has written `file` (the
/// resolved closed period, the same wording in both modes):
/// `[years] записано період {from}–{to} → {file}`.
fn years_out_note(from: u32, to: u32, file: &Path) -> String {
    format!(
        "[years] записано період {from:04}–{to:04} → {}",
        file.display()
    )
}

/// Offline «which years hold the documents» (`years`): the document-date
/// distribution by year of `org_date` over a CLOSED year range — the
/// range-selected companion of `dates` (same column, same GLOB guard, so
/// the histograms always agree), unlike `months` NOT zero-filling the
/// empty years inside the range. Never builds a client, never touches the
/// network. A missing index file is a hard error («індекс не знайдено: …»,
/// the `stats`/`verify`/`months` rule); a malformed `--from`/`--to` or a
/// `--from` later than the `--to` exits 1 via [`normalize_year_arg`] and
/// [`check_year_range`]. The bounds default to the oldest / newest year
/// present in the index — `MIN`/`MAX(substr(org_date,1,4))` over the same
/// `GLOB '[0-9][0-9][0-9][0-9]*'` guard as `dates` (an index without a
/// single dated row leaves the range `0000–0000`, which renders the zero
/// report). The histogram comes straight from `docs` through the
/// connection `rada_db::open` returns (the `verify` / `dict-stats`
/// direct-SQL idiom, the `dates` query verbatim): `substr(org_date,1,4)`
/// grouped over the rows passing the GLOB year-prefix guard, ordered
/// ascending; the pure renderers keep the in-range rows whose four-digit
/// year parses. Documents outside the histogram (missing or malformed
/// date) are `total` minus the full histogram sum and render as
/// «без дати» (the `dates` rule, range-independent). Informational: the
/// run always exits 0. With `--out` the exact stdout body goes to the
/// file (parent dirs created) and nothing prints to stdout: the plain
/// report is written UTF-8 with a BOM (Excel-friendly, like
/// search/arrivals/stats/verify), the `--json` body BOM-less; one
/// `[years] записано період {from}–{to} → {file}` note goes to stderr.
/// The header always renders, so the file is never BOM-alone.
fn cmd_years(
    db: &PathBuf,
    from: Option<&str>,
    to: Option<&str>,
    json: bool,
    out: Option<&Path>,
) -> Result<()> {
    if !db.exists() {
        return Err(anyhow::anyhow!("індекс не знайдено: {}", db.display()));
    }
    let from = normalize_year_arg(from)?;
    let to = normalize_year_arg(to)?;
    let conn = rada_db::open(db).with_context(|| format!("open db {}", db.display()))?;
    // Default bounds: the oldest / newest year present in the index (the
    // `dates` MIN/MAX rule — same GLOB guard over the four-digit prefix);
    // an index with no dated row at all leaves the `0` sentinel, which
    // renders the zero report (the all-zero `0000` idiom of `months`).
    let from = if from == 0 {
        conn.query_row(
            "SELECT MIN(substr(org_date,1,4)) FROM docs
             WHERE org_date GLOB '[0-9][0-9][0-9][0-9]*'",
            [],
            |r| r.get::<_, Option<String>>(0),
        )
        .unwrap_or(None)
        .and_then(|y| y.parse::<u32>().ok())
        .unwrap_or(0)
    } else {
        from
    };
    let to = if to == 0 {
        conn.query_row(
            "SELECT MAX(substr(org_date,1,4)) FROM docs
             WHERE org_date GLOB '[0-9][0-9][0-9][0-9]*'",
            [],
            |r| r.get::<_, Option<String>>(0),
        )
        .unwrap_or(None)
        .and_then(|y| y.parse::<u32>().ok())
        .unwrap_or(0)
    } else {
        to
    };
    check_year_range(from, to)?;
    let total: i64 = conn
        .query_row("SELECT COUNT(*) FROM docs", [], |r| r.get(0))
        .unwrap_or(0);
    let mut rows: Vec<YearRow> = Vec::new();
    let mut stmt = conn
        .prepare(
            "SELECT substr(org_date,1,4) AS y, COUNT(*) FROM docs
             WHERE org_date GLOB '[0-9][0-9][0-9][0-9]*'
             GROUP BY y ORDER BY y",
        )
        .context("prepare years histogram")?;
    let mut mapped = stmt
        .query_map([], |r| {
            Ok(YearRow {
                year: r.get(0)?,
                count: r.get(1)?,
            })
        })
        .context("run years histogram")?;
    while let Some(row) = mapped.next().transpose()? {
        rows.push(row);
    }
    // `undated` mirrors `dates` exactly: the whole-index count minus the
    // FULL histogram sum (range-independent).
    let dated: i64 = rows.iter().map(|r| r.count).sum();
    let undated = total - dated;
    if json {
        let body = years_json(from, to, &rows, undated);
        match out {
            Some(p) => {
                // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                write_out_file(p, format!("{body}\n").as_bytes())?;
                eprintln!("{}", years_out_note(from, to, p));
            }
            None => println!("{body}"),
        }
        return Ok(());
    }
    let lines = years_lines(from, to, &rows, undated);
    match out {
        Some(p) => {
            // UTF-8 BOM prefix: the same Excel-friendly convention as
            // search/arrivals/stats/verify; the bytes are exactly the
            // stdout report (the header always renders, so never
            // BOM-alone).
            write_out_file(p, format!("\u{FEFF}{}\n", lines.join("\n")).as_bytes())?;
            eprintln!("{}", years_out_note(from, to, p));
        }
        None => {
            for line in lines {
                println!("{line}");
            }
        }
    }
    Ok(())
}

/// One `recent` document row: the registration number, the title as
/// stored in the TEXT `nazva` column of `docs` (the index has no `title`
/// column — `nazva` is the title), and the raw 8-digit `org_date`
/// (`YYYYMMDD`) the row qualified with.
struct RecentDocRow {
    nreg: String,
    title: String,
    org_date: String,
}

/// Validate the `recent --days` option (the `normalize_month_year` house
/// rule: a clear Ukrainian error, exit 1 through `main`): absent means
/// the 7-day default (`Ok(7)`); present must parse to an unsigned number
/// inside 1..=365 (surrounding whitespace trimmed), anything else —
/// `"0"`, `"366"`, `"abcd"`, the empty string — fails with
/// «днів має бути 1–365».
fn normalize_recent_days(days: Option<&str>) -> Result<u32> {
    match days {
        None => Ok(7),
        Some(d) => {
            let d = d.trim();
            match d.parse::<u32>() {
                Ok(n) if (1..=365).contains(&n) => Ok(n),
                _ => Err(anyhow::anyhow!("днів має бути 1–365")),
            }
        }
    }
}

/// Validate the `recent --limit` option (the same house rule as
/// [`normalize_recent_days`], mirroring how `orgs` bounds its `--limit`
/// at 500): absent means the 100-row default (`Ok(100)`); present must
/// parse to an unsigned number inside 1..=500 (whitespace trimmed),
/// anything else — `"0"`, `"501"`, `"abcd"`, the empty string — fails
/// with «ліміт має бути 1–500».
fn normalize_recent_limit(limit: Option<&str>) -> Result<usize> {
    match limit {
        None => Ok(100),
        Some(l) => {
            let l = l.trim();
            match l.parse::<usize>() {
                Ok(n) if (1..=500).contains(&n) => Ok(n),
                _ => Err(anyhow::anyhow!("ліміт має бути 1–500")),
            }
        }
    }
}

/// Proleptic-Gregorian `YYYYMMDD` → days since 1970-01-01 (Howard
/// Hinnant's canonical `days_from_civil` — the exact inverse of the
/// existing [`days_to_ymd`]); `None` for non-8-digit input or an
/// impossible calendar date (bad month, out-of-range day, non-leap
/// Feb 29). This is the crate's single canonical day count: the
/// `recent` window arithmetic uses it directly and [`ymd_to_days`]
/// delegates to it, so `arrivals --since` day-window math and the
/// `recent` cutoff share one epoch (1970-01-01) and compose with
/// [`days_to_ymd`] into correct calendar walks. (The
/// `search --date-from/--date-to` filters never converted dates at
/// all: `rada_db::search` compares the 8-digit `org_date` strings
/// inclusively in SQL.)
fn recent_days_from_civil(s: &str) -> Option<i64> {
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
    let yy = y - if m <= 2 { 1 } else { 0 };
    let era = if yy >= 0 { yy } else { yy - 399 } / 400;
    let yoe = yy - era * 400; // [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146097 + doe - 719468)
}

/// The inclusive cutoff date of the `recent` window: the calendar date
/// `days - 1` before the 8-digit `anchor` (the freshest indexed
/// `org_date`), so `--days 1` selects exactly the anchor day and
/// `--days 7` spans seven consecutive days ending at the anchor. Pure
/// (the canonical [`recent_days_from_civil`] + [`days_to_ymd`] civil-date
/// pair does the arithmetic, leap years and month ends included); `None`
/// for a malformed or impossible anchor.
fn recent_cutoff(anchor: &str, days: u32) -> Option<String> {
    let z = recent_days_from_civil(anchor)?;
    Some(days_to_ymd(z - (days as i64 - 1)))
}

/// The report body of `recent` (shared by stdout and `--out` so the two
/// cannot drift): «Свіжі документи ({from}–{to}):» header with the
/// inclusive cutoff window formatted `dd.mm.yyyy` via [`fmt_date`] (the
/// header drops the range — «Свіжі документи:» — when the index carries
/// no valid 8-digit date to anchor on), then one
/// `{dd.mm.yyyy}  {nreg}  {title}` line per document in input order (the
/// SQL orders by `org_date` descending, `nreg` ascending), then the
/// verdict «разом: {count}» over the rendered rows (post-`--limit`, NOT
/// the whole window). Informational: no qualifying document renders the
/// header + «немає документів за цей період» + «разом: 0» and the run
/// still exits 0.
fn recent_docs_lines(range: Option<(&str, &str)>, rows: &[RecentDocRow]) -> Vec<String> {
    let mut lines = vec![match range {
        Some((from, to)) => format!(
            "Свіжі документи ({}–{}):",
            fmt_date(from),
            fmt_date(to)
        ),
        None => "Свіжі документи:".to_string(),
    }];
    if rows.is_empty() {
        lines.push("немає документів за цей період".to_string());
    }
    for r in rows {
        lines.push(format!("{}  {}  {}", fmt_date(&r.org_date), r.nreg, r.title));
    }
    lines.push(format!("разом: {}", rows.len()));
    lines
}

/// The `--json` body of `recent` (shared by stdout and `--out` so the two
/// cannot drift): the pretty
/// `{"days":N,"docs":[{date,nreg,title},…],"from":"YYYYMMDD"|null,"to":"YYYYMMDD"|null}`
/// object built as hand-rolled `serde_json::Value`s like the other
/// commands' JSON bodies (map keys sort, so `days`/`docs`/`from`/`to` and
/// the `date`/`nreg`/`title` inside every entry keep their alphabetical
/// order). `date` is the `dd.mm.yyyy` rendering (the report rule);
/// `from`/`to` are the raw `YYYYMMDD` cutoff bounds — both `null` when
/// the index has no valid 8-digit date to anchor on (the `dates`
/// `"newest": null` idiom). stdout prints exactly this string plus a
/// trailing newline; the `--out` file receives the same bytes BOM-less
/// (machine data, the house JSON rule).
fn recent_docs_json(days: u32, range: Option<(&str, &str)>, rows: &[RecentDocRow]) -> String {
    let docs: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "date": fmt_date(&r.org_date),
                "nreg": r.nreg,
                "title": r.title,
            })
        })
        .collect();
    let value = serde_json::json!({
        "days": days,
        "docs": docs,
        "from": range.map(|(from, _)| from),
        "to": range.map(|(_, to)| to),
    });
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string())
}

/// The stderr note printed after `recent --out` has written `file`
/// (`n` rendered document rows, the same wording in both modes):
/// `[recent] записано {n} документів → {file}`.
fn recent_docs_out_note(n: usize, file: &Path) -> String {
    format!("[recent] записано {n} документів → {}", file.display())
}

/// Offline «what is the freshest» (`recent`): the newest documents by
/// adoption date over the `org_date` column of `docs` — the day-level
/// companion of `months` / `dates`, so the window is anchored on the
/// index's own freshest full 8-digit date (never the system clock: an
/// idle index still reports its newest documents). Never builds a
/// client, never touches the network. A missing index file is a hard
/// error («індекс не знайдено: …», the `stats`/`verify` rule). `--days`
/// passes [`normalize_recent_days`] and `--limit` [`normalize_recent_limit`]
/// (malformed values are hard errors, exit 1). The anchor is the newest
/// `org_date` surviving the full 8-digit `GLOB
/// '[0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9]'` guard AND
/// [`recent_days_from_civil`]' calendar check, walked `ORDER BY
/// org_date DESC` until one validates (identical to `MAX(org_date)` on
/// a well-formed index, robust on a dirty one); the inclusive cutoff
/// comes from the pure [`recent_cutoff`]. The rows come straight from
/// `docs` through
/// the connection `rada_db::open` returns (the `verify` / `dict-stats`
/// direct-SQL idiom): `nreg`/`nazva`/`org_date` between the cutoff and
/// the anchor, ordered `org_date DESC, nreg ASC`, the renderer-side
/// calendar filter dropping impossible dates before the `LIMIT`
/// truncation. An index without a single valid dated row (empty DB
/// included) renders the zero-rows report and exits 0. With `--out` the
/// exact stdout body goes to the file (parent dirs created) and nothing
/// prints to stdout: the plain report is written UTF-8 with a BOM
/// (Excel-friendly, like search/arrivals/stats/verify), the `--json`
/// body BOM-less; one `[recent] записано {n} документів → {file}` note
/// goes to stderr. The header always renders, so the file is never
/// BOM-alone.
fn cmd_recent_docs(
    db: &PathBuf,
    days: Option<&str>,
    limit: Option<&str>,
    json: bool,
    out: Option<&Path>,
) -> Result<()> {
    if !db.exists() {
        return Err(anyhow::anyhow!("індекс не знайдено: {}", db.display()));
    }
    let days = normalize_recent_days(days)?;
    let limit = normalize_recent_limit(limit)?;
    let conn = rada_db::open(db).with_context(|| format!("open db {}", db.display()))?;
    // Anchor: the freshest full 8-digit date present in the index (the
    // `months`/`dates` MAX idiom, widened with the
    // `recent_days_from_civil`
    // calendar check so a malformed newest value — e.g. month 13 —
    // cannot poison the window; on a well-formed index this is exactly
    // MAX(org_date)).
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT org_date FROM docs
             WHERE org_date GLOB '[0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9]'
             ORDER BY org_date DESC",
        )
        .context("prepare recent anchor")?;
    let mut mapped = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .context("run recent anchor")?;
    let mut anchor: Option<String> = None;
    while let Some(date) = mapped.next().transpose()? {
        if recent_days_from_civil(&date).is_some() {
            anchor = Some(date);
            break;
        }
    }
    // The inclusive window is [cutoff, anchor]; no anchor (no valid
    // dated row at all) renders the zero-rows report.
    let range: Option<(String, String)> = anchor
        .as_deref()
        .and_then(|a| recent_cutoff(a, days).map(|cutoff| (cutoff, a.to_string())));
    let mut rows: Vec<RecentDocRow> = Vec::new();
    if let Some((from, to)) = &range {
        let mut stmt = conn
            .prepare(
                "SELECT nreg, nazva, org_date FROM docs
                 WHERE org_date GLOB '[0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9]'
                   AND org_date >= ?1 AND org_date <= ?2
                 ORDER BY org_date DESC, nreg ASC",
            )
            .context("prepare recent docs")?;
        let mut mapped = stmt
            .query_map([from.as_str(), to.as_str()], |r| {
                Ok(RecentDocRow {
                    nreg: r.get(0)?,
                    title: r.get(1)?,
                    org_date: r.get(2)?,
                })
            })
            .context("run recent docs")?;
        while let Some(row) = mapped.next().transpose()? {
            // The SQL already bounds the string range; the calendar
            // filter drops impossible in-range dates (e.g. 20260931)
            // BEFORE the limit truncates, so a dirty row never steals
            // a rendered slot.
            if recent_days_from_civil(&row.org_date).is_none() {
                continue;
            }
            if rows.len() >= limit {
                break;
            }
            rows.push(row);
        }
    }
    let range = range.as_ref().map(|(from, to)| (from.as_str(), to.as_str()));
    if json {
        let body = recent_docs_json(days, range, &rows);
        match out {
            Some(p) => {
                // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                write_out_file(p, format!("{body}\n").as_bytes())?;
                eprintln!("{}", recent_docs_out_note(rows.len(), p));
            }
            None => println!("{body}"),
        }
        return Ok(());
    }
    let lines = recent_docs_lines(range, &rows);
    match out {
        Some(p) => {
            // UTF-8 BOM prefix: the same Excel-friendly convention as
            // search/arrivals/stats/verify; the bytes are exactly the
            // stdout report (the header always renders, so never
            // BOM-alone).
            write_out_file(p, format!("\u{FEFF}{}\n", lines.join("\n")).as_bytes())?;
            eprintln!("{}", recent_docs_out_note(rows.len(), p));
        }
        None => {
            for line in lines {
                println!("{line}");
            }
        }
    }
    Ok(())
}

/// One `orgs` histogram row: the publisher id as stored in the TEXT
/// `org_num` column (the empty string is the «publisher unknown»
/// convention), its label resolved from the offline `organs` dictionary
/// (or the «— не вказано» placeholder), and the number of documents
/// carrying it.
struct OrgRow {
    org_num: String,
    label: String,
    count: i64,
}

/// The report body of `orgs` (shared by stdout and `--out` so the two
/// cannot drift): «Найкращі видавці (за документами):» header, then one
/// `{label} [{org_num}]: {count}` line per publisher in input order (the
/// SQL sorts by count descending — no rank prefix, mirroring the
/// `dates`/`dict-stats` body style, counts render plain like the
/// `stats`/`verify` numbers), then the «всього документів: {total}»
/// verdict. Informational: an empty histogram never fails the run. With
/// `year` (the validated four-digit `--year` filter) the header becomes
/// «Видавці документів {yyyy} року:»; `None` keeps the unfiltered
/// header exactly as today.
fn orgs_lines(rows: &[OrgRow], total: i64, year: Option<&str>) -> Vec<String> {
    let mut lines = vec![match year {
        Some(y) => format!("Видавці документів {y} року:"),
        None => "Найкращі видавці (за документами):".to_string(),
    }];
    for r in rows {
        lines.push(format!("{} [{}]: {}", r.label, r.org_num, r.count));
    }
    lines.push(format!("всього документів: {total}"));
    lines
}

/// The `--json` body of `orgs` (shared by stdout and `--out` so the two
/// cannot drift): the pretty
/// `{"orgs":[{count,label,org_num},…],"total":N}` object over the rows,
/// built as hand-rolled `serde_json::Value`s like the other commands'
/// JSON bodies (map keys sort, so `orgs` precedes `total` and
/// `count`/`label`/`org_num` keep their alphabetical order inside every
/// entry). With `year` (the validated four-digit `--year` filter) the
/// object gains `"year": "yyyy"` — map keys sort, so it lands last,
/// after `total`. stdout prints exactly this string plus a trailing
/// newline; the `--out` file receives the same bytes BOM-less (machine
/// data, the house JSON rule).
fn orgs_json(rows: &[OrgRow], total: i64, year: Option<&str>) -> String {
    let orgs: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "org_num": r.org_num,
                "label": r.label,
                "count": r.count,
            })
        })
        .collect();
    let mut value = serde_json::json!({ "orgs": orgs, "total": total });
    if let Some(y) = year {
        value["year"] = serde_json::Value::from(y);
    }
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string())
}

/// The stderr note printed after `orgs --out` has written `file`
/// (`n` publisher rows, the same wording in both modes):
/// `[orgs] записано {n} видавців → {file}`.
fn orgs_out_note(n: usize, file: &Path) -> String {
    format!("[orgs] записано {n} видавців → {}", file.display())
}

/// Offline «who publishes the most» (`orgs`): the top-publishers histogram
/// over the `org_num` column of `docs` — complements `dates` /
/// `dict-stats` and helps the user pick the `search --org` filters. Never
/// builds a client, never touches the network. A missing index file is a
/// hard error («індекс не знайдено: …», the `stats`/`verify` rule). The
/// histogram comes straight from `docs` through the connection
/// `rada_db::open` returns (the `verify` / `dict-stats` direct-SQL idiom):
/// `GROUP BY org_num ORDER BY COUNT(*) DESC LIMIT` clamped to 1..=200.
/// `org_num` is TEXT (`''` = publisher unknown), so the label comes from
/// the offline `organs` dictionary only when it parses to a nonzero id —
/// one `query_row` per distinct `org_num` (the `favorites` title-cache
/// idiom), everything else renders the «— не вказано» placeholder. The
/// `total` is the whole index (`COUNT(*) FROM docs`, the `dates` rule).
/// Informational: the run always exits 0. With `--out` the exact stdout
/// body goes to the file (parent dirs created) and nothing prints to
/// stdout: the plain report is written UTF-8 with a BOM (Excel-friendly,
/// like search/arrivals/stats/verify), the `--json` body BOM-less; one
/// `[orgs] записано {n} видавців → {file}` note goes to stderr. The
/// header always renders, so the file is never BOM-alone. With `--year`
/// (validated through the `years`/`months` [`normalize_year_arg`] rule:
/// exactly four ASCII digits or «рік має бути РРРР (наприклад 2026)»,
/// exit 1 through `main`) only documents whose `org_date` starts with
/// those four digits and is a full 8-digit date — the `months` digit-
/// guard GLOB idiom with the year prefix pinned — count toward the
/// histogram and the «всього документів» verdict (the same `COUNT(*)`
/// rule, filtered); the header names the year instead.
fn cmd_orgs(
    db: &PathBuf,
    year: Option<&str>,
    limit: usize,
    json: bool,
    out: Option<&Path>,
) -> Result<()> {
    if !db.exists() {
        return Err(anyhow::anyhow!("індекс не знайдено: {}", db.display()));
    }
    // the `years`/`months` house validator: absent keeps the whole-index
    // report, present must be exactly four ASCII digits; the canonical
    // `yyyy` string drives the header, the JSON key and the GLOB prefix
    let year = match year {
        Some(_) => Some(format!("{:04}", normalize_year_arg(year)?)),
        None => None,
    };
    let limit = limit.clamp(1, 200);
    let conn = rada_db::open(db).with_context(|| format!("open db {}", db.display()))?;
    // a row counts for `--year yyyy` only when `org_date` is a full
    // 8-digit date starting with the four validated digits; the digits
    // are interpolated only after `normalize_year_arg` (ASCII digits
    // cannot carry SQL or GLOB metacharacters), the unfiltered path
    // keeps today's queries verbatim
    let (total_sql, where_sql) = match &year {
        None => ("SELECT COUNT(*) FROM docs".to_string(), String::new()),
        Some(y) => {
            let glob = format!("{y}[0-9][0-9][0-9][0-9]");
            (
                format!("SELECT COUNT(*) FROM docs WHERE org_date GLOB '{glob}'"),
                format!(" WHERE d.org_date GLOB '{glob}'"),
            )
        }
    };
    let total: i64 = conn
        .query_row(&total_sql, [], |r| r.get(0))
        .unwrap_or(0);
    let sql = format!(
        "SELECT d.org_num, COUNT(*) AS c FROM docs d{where_sql}
         GROUP BY d.org_num ORDER BY c DESC LIMIT ?1"
    );
    let mut stmt = conn
        .prepare(&sql)
        .context("prepare orgs histogram")?;
    let mut mapped = stmt
        .query_map([limit as i64], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })
        .context("run orgs histogram")?;
    // Offline label lookup cached per distinct org_num (the `favorites`
    // title-cache idiom): the TEXT `org_num` resolves through the
    // `organs` dictionary only when it parses to a nonzero id; an empty /
    // zero / non-numeric org_num or a dict miss keeps the empty label,
    // and the row renders the «— не вказано» placeholder.
    let mut labels: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut rows: Vec<OrgRow> = Vec::new();
    while let Some((org_num, count)) = mapped.next().transpose()? {
        let label = labels
            .entry(org_num.clone())
            .or_insert_with(|| match org_num.trim().parse::<i64>() {
                Ok(id) if id != 0 => conn
                    .query_row(
                        "SELECT label FROM dict WHERE kind = 'organs' AND id = ?1",
                        [id],
                        |r| r.get::<_, String>(0),
                    )
                    .unwrap_or_default(),
                _ => String::new(),
            })
            .clone();
        rows.push(OrgRow {
            org_num,
            label: if label.is_empty() {
                "— не вказано".to_string()
            } else {
                label
            },
            count,
        });
    }
    if json {
        let body = orgs_json(&rows, total, year.as_deref());
        match out {
            Some(p) => {
                // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                write_out_file(p, format!("{body}\n").as_bytes())?;
                eprintln!("{}", orgs_out_note(rows.len(), p));
            }
            None => println!("{body}"),
        }
        return Ok(());
    }
    let lines = orgs_lines(&rows, total, year.as_deref());
    match out {
        Some(p) => {
            // UTF-8 BOM prefix: the same Excel-friendly convention as
            // search/arrivals/stats/verify; the bytes are exactly the
            // stdout report (the header always renders, so never
            // BOM-alone).
            write_out_file(p, format!("\u{FEFF}{}\n", lines.join("\n")).as_bytes())?;
            eprintln!("{}", orgs_out_note(rows.len(), p));
        }
        None => {
            for line in lines {
                println!("{line}");
            }
        }
    }
    Ok(())
}

/// Cap the `docs --limit` option at the `hits`/`recent` row budget:
/// `0` renders one row and anything above `500` is truncated to `500`
/// (`usize::clamp`, the same house idiom as the `orgs` 1..=200 bound);
/// 1..=500 passes through untouched (no hard error).
fn docs_limit(limit: usize) -> usize {
    limit.clamp(1, 500)
}

/// Validate that `docs` was given at least one filter: `--org` and
/// `--year` are both optional, but the command never dumps the whole
/// index — with neither the run exits 1 with «вкажіть --org або --year»
/// (through `main`, after the missing-index error, before any query).
/// An empty `--org ""` still counts as given: it is an exact `org_num`
/// match like every other key, not an absent filter.
fn check_docs_filters(org: Option<&str>, year: Option<&str>) -> Result<()> {
    if org.is_none() && year.is_none() {
        return Err(anyhow::anyhow!("вкажіть --org або --year"));
    }
    Ok(())
}

/// The report body of `docs` (shared by stdout and `--out` so the two
/// cannot drift): the header — `org` is what the caller resolved for
/// display: the `organs`-dictionary label when the numeric key resolved,
/// the raw publisher key otherwise — with the three header forms:
/// `org` alone «Документи видавця {org}:» (the historical report,
/// byte-identical), `year` alone «Документи {yyyy} року:» (the
/// `orgs --year` header family), both «Документи видавця {org}:
/// {yyyy} року:»; `None`/`None` is unreachable through `cmd_docs`
/// ([`check_docs_filters`] exits 1 first) and renders the neutral
/// «Документи:» fallback. Then one `{dd.mm.yyyy}  {nreg}  {nazva}` line
/// per document in input order (the SQL orders by `org_date` descending,
/// `nreg` ascending; the rendering mirrors [`recent_docs_lines`]), then
/// the verdict «разом: {count}» over the RENDERED rows (post-`--limit`,
/// the `recent` rule, NOT the whole filtered archive). Informational: an
/// unknown publisher or an empty year window renders the header +
/// «немає документів за цей період» + «разом: 0» and the run still
/// exits 0.
fn docs_list_lines(org: Option<&str>, year: Option<&str>, rows: &[RecentDocRow]) -> Vec<String> {
    let header = match (org, year) {
        (Some(o), Some(y)) => format!("Документи видавця {o}: {y} року:"),
        (Some(o), None) => format!("Документи видавця {o}:"),
        (None, Some(y)) => format!("Документи {y} року:"),
        (None, None) => "Документи:".to_string(),
    };
    let mut lines = vec![header];
    if rows.is_empty() {
        lines.push("немає документів за цей період".to_string());
    }
    for r in rows {
        lines.push(format!("{}  {}  {}", fmt_date(&r.org_date), r.nreg, r.title));
    }
    lines.push(format!("разом: {}", rows.len()));
    lines
}

/// The `--json` body of `docs` (shared by stdout and `--out` so the two
/// cannot drift): the pretty
/// `{"docs":[{date,nreg,title},…],"org":…,"org_label":…,"total":N[,"year":"yyyy"]}`
/// object over the rendered rows, built as hand-rolled
/// `serde_json::Value`s like the other commands' JSON bodies (map keys
/// sort, so `docs`/`org`/`org_label`/`total`/`year` and the
/// `date`/`nreg`/`title` inside every entry keep their alphabetical
/// order). `date` is the raw 8-digit `org_date` (machine data; the plain
/// report renders it `dd.mm.yyyy` via [`fmt_date`]); `org` is the key as
/// given, `org_label` the header's display string (the resolved label,
/// or the raw key when the dictionary had nothing) — both keys render
/// only when `org` is given (the year-only body describes no publisher,
/// so the keys are omitted rather than faked; alphabetical order holds
/// either way); `total` counts the LISTED rows (the «разом» rule,
/// post-`--limit`). With `year` (the validated four-digit `--year`
/// filter) the object gains `"year": "yyyy"` — map keys sort, so it
/// lands last, after `total` (the `orgs_json(rows, total, year)` rule);
/// absent omits the key entirely. stdout prints exactly this string plus
/// a trailing newline; the `--out` file receives the same bytes BOM-less
/// (machine data, the house JSON rule).
fn docs_list_json(
    org: Option<&str>,
    org_label: Option<&str>,
    year: Option<&str>,
    rows: &[RecentDocRow],
) -> String {
    let docs: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "date": r.org_date,
                "nreg": r.nreg,
                "title": r.title,
            })
        })
        .collect();
    let mut value = serde_json::json!({
        "docs": docs,
        "total": rows.len(),
    });
    if let Some(o) = org {
        value["org"] = serde_json::Value::from(o);
        value["org_label"] = serde_json::Value::from(org_label.unwrap_or(o));
    }
    if let Some(y) = year {
        value["year"] = serde_json::Value::from(y);
    }
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string())
}

/// The stderr note printed after `docs --out` has written `file`
/// (`n` rendered document rows, the same wording in both modes):
/// `[docs] записано {n} документів → {file}`.
fn docs_list_out_note(n: usize, file: &Path) -> String {
    format!("[docs] записано {n} документів → {}", file.display())
}

/// Offline «what documents are there» (`docs`): the document list on
/// the TEXT `org_num` / `org_date` columns of `docs` — the drill-down
/// companion of [`cmd_orgs`], fed with the same publisher key the
/// histogram prints (or the year window `years` prints). Never builds a
/// client, never touches the network. A missing index file is a hard
/// error («індекс не знайдено: …», the `stats`/`verify` rule); then
/// [`check_docs_filters`] requires at least one of `--org` / `--year`
/// («вкажіть --org або --year», exit 1 through `main`, before any
/// query). `--year` passes the
/// `years`/`months`/`orgs` [`normalize_year_arg`] validator (exactly
/// four ASCII digits or «рік має бути РРРР (наприклад 2026)», exit 1
/// through `main`); `--limit` passes [`docs_limit`] (clamped to
/// 1..=500, never an error). The rows come straight from `docs` through
/// the connection `rada_db::open` returns (the `verify` / `dict-stats`
/// direct-SQL idiom): with `--org` the `WHERE` gains `org_num = ?1`
/// bound as a parameter (the year-only listing drops the clause
/// entirely — the `cmd_orgs` conditional-`WHERE` idiom), `org_date`
/// guarded to a full 8-digit date (the `recent` digit-GLOB; with
/// `--year` the four validated digits are pinned as the GLOB prefix,
/// the `orgs` idiom), ordered `org_date DESC, nreg ASC`, `LIMIT`
/// interpolated from the clamped `usize` (ASCII digits only — the same
/// interpolation-safety reasoning as the validated year prefix). The
/// header label comes from the offline `organs` dictionary exactly like
/// the `orgs` rows (one `query_row`, only with `--org`, resolved only
/// when the key parses to a nonzero id; a miss keeps the raw key in the
/// header).
/// Informational: an unknown publisher (or an empty year window) exits
/// 0 with the zero-rows report. With `--out` the exact stdout body goes
/// to the file (parent dirs created) and nothing prints to stdout: the
/// plain report is written UTF-8 with a BOM (Excel-friendly, like
/// search/arrivals/stats/verify), the `--json` body BOM-less; one
/// `[docs] записано {n} документів → {file}` note goes to stderr. The
/// header always renders, so the file is never BOM-alone.
fn cmd_docs(
    db: &PathBuf,
    org: Option<&str>,
    year: Option<&str>,
    limit: usize,
    json: bool,
    out: Option<&Path>,
) -> Result<()> {
    if !db.exists() {
        return Err(anyhow::anyhow!("індекс не знайдено: {}", db.display()));
    }
    // at least one of `--org` / `--year` must be given: the command
    // never dumps the whole index (the missing-index error above wins
    // first, and no query runs before this check)
    check_docs_filters(org, year)?;
    // the `years`/`months`/`orgs` house validator: absent keeps the
    // whole-archive report, present must be exactly four ASCII digits;
    // the canonical `yyyy` string drives the JSON key and the GLOB prefix
    let year = match year {
        Some(_) => Some(format!("{:04}", normalize_year_arg(year)?)),
        None => None,
    };
    let limit = docs_limit(limit);
    let conn = rada_db::open(db).with_context(|| format!("open db {}", db.display()))?;
    // Offline label lookup, the `orgs` rule, only with `--org`: the TEXT
    // key resolves through the `organs` dictionary only when it parses
    // to a nonzero id; a non-numeric / zero / empty key or a dict miss
    // leaves the label empty and the header names the raw key instead.
    let header_org: Option<String> = org.map(|org| {
        let label = match org.trim().parse::<i64>() {
            Ok(id) if id != 0 => conn
                .query_row(
                    "SELECT label FROM dict WHERE kind = 'organs' AND id = ?1",
                    [id],
                    |r| r.get::<_, String>(0),
                )
                .unwrap_or_default(),
            _ => String::new(),
        };
        if label.is_empty() {
            org.to_string()
        } else {
            label
        }
    });
    // a row qualifies only with a full 8-digit `org_date` (the `recent`
    // digit-GLOB guard); `--year yyyy` pins the four validated digits as
    // the prefix — year + four digit-classes is still exactly 8 digits,
    // the digits are interpolated only after `normalize_year_arg` (ASCII
    // digits cannot carry SQL or GLOB metacharacters)
    let glob = match &year {
        None => "[0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9]".to_string(),
        Some(y) => format!("{y}[0-9][0-9][0-9][0-9]"),
    };
    // with `--org` the `org_num` filter is bound as a parameter (never
    // interpolated), exactly the historical statement; the year-only
    // listing drops the clause (the `cmd_orgs` conditional-`WHERE`
    // idiom); `limit` is interpolated only after `docs_limit` — a
    // clamped usize renders as plain ASCII digits, no SQL
    // metacharacters possible
    let sql = match org {
        Some(_) => format!(
            "SELECT nreg, nazva, org_date FROM docs
             WHERE org_num = ?1 AND org_date GLOB '{glob}'
             ORDER BY org_date DESC, nreg ASC LIMIT {limit}"
        ),
        None => format!(
            "SELECT nreg, nazva, org_date FROM docs
             WHERE org_date GLOB '{glob}'
             ORDER BY org_date DESC, nreg ASC LIMIT {limit}"
        ),
    };
    let mut stmt = conn
        .prepare(&sql)
        .context("prepare docs list")?;
    let mut rows: Vec<RecentDocRow> = Vec::new();
    match org {
        Some(o) => {
            let mut mapped = stmt
                .query_map([o], |r| {
                    Ok(RecentDocRow {
                        nreg: r.get(0)?,
                        title: r.get(1)?,
                        org_date: r.get(2)?,
                    })
                })
                .context("run docs list")?;
            while let Some(row) = mapped.next().transpose()? {
                rows.push(row);
            }
        }
        // the year-only listing has no publisher parameter to bind
        None => {
            let mut mapped = stmt
                .query_map([], |r| {
                    Ok(RecentDocRow {
                        nreg: r.get(0)?,
                        title: r.get(1)?,
                        org_date: r.get(2)?,
                    })
                })
                .context("run docs list")?;
            while let Some(row) = mapped.next().transpose()? {
                rows.push(row);
            }
        }
    }
    if json {
        let body = docs_list_json(org, header_org.as_deref(), year.as_deref(), &rows);
        match out {
            Some(p) => {
                // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                write_out_file(p, format!("{body}\n").as_bytes())?;
                eprintln!("{}", docs_list_out_note(rows.len(), p));
            }
            None => println!("{body}"),
        }
        return Ok(());
    }
    let lines = docs_list_lines(header_org.as_deref(), year.as_deref(), &rows);
    match out {
        Some(p) => {
            // UTF-8 BOM prefix: the same Excel-friendly convention as
            // search/arrivals/stats/verify; the bytes are exactly the
            // stdout report (the header always renders, so never
            // BOM-alone).
            write_out_file(p, format!("\u{FEFF}{}\n", lines.join("\n")).as_bytes())?;
            eprintln!("{}", docs_list_out_note(rows.len(), p));
        }
        None => {
            for line in lines {
                println!("{line}");
            }
        }
    }
    Ok(())
}

/// One `statuses` histogram row: the status id as stored in the INTEGER
/// `status` column (0 is the «status unknown» default), its label resolved
/// from the offline `stan` dictionary (fallback: the built-in
/// `StatusId::label`, or the «— не визначено» placeholder), and the number
/// of documents carrying it.
struct StatusRow {
    id: i64,
    label: String,
    count: i64,
}

/// The report body of `statuses` (shared by stdout and `--out` so the two
/// cannot drift): «Розподіл за статусами:» header, then one
/// `{label} [{id}]: {count}` line per status in input order (the SQL sorts
/// by count descending — no rank prefix, mirroring the
/// `dates`/`dict-stats` body style, counts render plain like the
/// `stats`/`verify` numbers), then the «всього документів: {total}»
/// verdict, or «індекс порожній» when the histogram has no rows at all
/// (the `dates_lines` empty idiom). Informational: an empty histogram
/// never fails the run.
fn statuses_lines(rows: &[StatusRow], total: i64) -> Vec<String> {
    let mut lines = vec!["Розподіл за статусами:".to_string()];
    for r in rows {
        lines.push(format!("{} [{}]: {}", r.label, r.id, r.count));
    }
    match rows.first() {
        Some(_) => lines.push(format!("всього документів: {total}")),
        None => lines.push("індекс порожній".to_string()),
    }
    lines
}

/// The `--json` body of `statuses` (shared by stdout and `--out` so the two
/// cannot drift): the pretty
/// `{"statuses":[{count,id,label},…],"total":N}` object over the rows,
/// built as hand-rolled `serde_json::Value`s like the other commands'
/// JSON bodies (map keys sort, so `statuses` precedes `total` and
/// `count`/`id`/`label` keep their alphabetical order inside every entry).
/// stdout prints exactly this string plus a trailing newline; the `--out`
/// file receives the same bytes BOM-less (machine data, the house JSON
/// rule).
fn statuses_json(rows: &[StatusRow], total: i64) -> String {
    let statuses: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "label": r.label,
                "count": r.count,
            })
        })
        .collect();
    let value = serde_json::json!({ "statuses": statuses, "total": total });
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string())
}

/// The stderr note printed after `statuses --out` has written `file`
/// (`n` status rows, the same wording in both modes):
/// `[statuses] записано {n} статусів → {file}`.
fn statuses_out_note(n: usize, file: &Path) -> String {
    format!("[statuses] записано {n} статусів → {}", file.display())
}

/// Offline «what condition is my index in» (`statuses`): the
/// document-status distribution over the INTEGER `status` column of `docs`
/// — complements `dates` / `orgs`, shows the freshness mix, and helps the
/// user pick the status filters. Never builds a client, never touches the
/// network. A missing index file is a hard error («індекс не знайдено: …»,
/// the `stats`/`verify` rule). The histogram comes straight from `docs`
/// through the connection `rada_db::open` returns (the `verify` /
/// `dict-stats` direct-SQL idiom): `GROUP BY status ORDER BY COUNT(*)
/// DESC` (there are few statuses — the `stan` dictionary holds ~10, so no
/// `--limit`). The label comes from the offline `stan` dictionary — one
/// `query_row` per distinct status cached in a `HashMap` (the `orgs`
/// idiom), so the output is identical to `dict --kind stan` where
/// populated — and falls back to the built-in `StatusId(id).label()` when
/// the dict row is missing; an empty label renders the «— не визначено»
/// placeholder. The `total` is the whole index (`COUNT(*) FROM docs`, the
/// `dates` rule). Informational: the run always exits 0. With `--out` the
/// exact stdout body goes to the file (parent dirs created) and nothing
/// prints to stdout: the plain report is written UTF-8 with a BOM
/// (Excel-friendly, like search/arrivals/stats/verify), the `--json` body
/// BOM-less; one `[statuses] записано {n} статусів → {file}` note goes to
/// stderr. The header always renders, so the file is never BOM-alone.
fn cmd_statuses(db: &PathBuf, json: bool, out: Option<&Path>) -> Result<()> {
    if !db.exists() {
        return Err(anyhow::anyhow!("індекс не знайдено: {}", db.display()));
    }
    let conn = rada_db::open(db).with_context(|| format!("open db {}", db.display()))?;
    let total: i64 = conn
        .query_row("SELECT COUNT(*) FROM docs", [], |r| r.get(0))
        .unwrap_or(0);
    let mut stmt = conn
        .prepare(
            "SELECT status, COUNT(*) AS c FROM docs
             GROUP BY status ORDER BY c DESC",
        )
        .context("prepare statuses histogram")?;
    let mut mapped = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
        .context("run statuses histogram")?;
    // Offline label lookup cached per distinct status (the `orgs`
    // label-cache idiom): the `stan` dictionary wins where populated (the
    // output then matches `dict --kind stan` byte for byte), a dict miss
    // falls back to the built-in `StatusId` label, and an empty label
    // renders the «— не визначено» placeholder.
    let mut labels: std::collections::HashMap<i64, String> =
        std::collections::HashMap::new();
    let mut rows: Vec<StatusRow> = Vec::new();
    while let Some((id, count)) = mapped.next().transpose()? {
        let label = labels
            .entry(id)
            .or_insert_with(|| {
                conn.query_row(
                    "SELECT label FROM dict WHERE kind = 'stan' AND id = ?1",
                    [id],
                    |r| r.get::<_, String>(0),
                )
                .unwrap_or_else(|_| rada_core::models::StatusId(id).label().to_string())
            })
            .clone();
        rows.push(StatusRow {
            id,
            label: if label.is_empty() {
                "— не визначено".to_string()
            } else {
                label
            },
            count,
        });
    }
    if json {
        let body = statuses_json(&rows, total);
        match out {
            Some(p) => {
                // machine data: raw UTF-8 JSON, no BOM, nothing on stdout
                write_out_file(p, format!("{body}\n").as_bytes())?;
                eprintln!("{}", statuses_out_note(rows.len(), p));
            }
            None => println!("{body}"),
        }
        return Ok(());
    }
    let lines = statuses_lines(&rows, total);
    match out {
        Some(p) => {
            // UTF-8 BOM prefix: the same Excel-friendly convention as
            // search/arrivals/stats/verify; the bytes are exactly the
            // stdout report (the header always renders, so never
            // BOM-alone).
            write_out_file(p, format!("\u{FEFF}{}\n", lines.join("\n")).as_bytes())?;
            eprintln!("{}", statuses_out_note(rows.len(), p));
        }
        None => {
            for line in lines {
                println!("{line}");
            }
        }
    }
    Ok(())
}

/// The stdout report of `status` (shared by stdout and `--out` so the two
/// cannot drift): the four fixed `index`/`documents`/`last import`/`db size`
/// rows (a missing `last import` timestamp renders «unknown», size in MB
/// with one decimal), then one aligned `dict {kind:<7}: {n} entries` row
/// per reference dictionary. The report always renders every row, so the
/// `--out` file never carries the BOM alone.
fn status_lines(
    db: &Path,
    docs: i64,
    last_import: Option<&str>,
    size_bytes: u64,
    dicts: [(&str, i64); 3],
) -> Vec<String> {
    let mut lines = vec![
        format!("index      : {}", db.display()),
        format!("documents  : {docs}"),
        format!("last import: {}", last_import.unwrap_or("unknown")),
        format!(
            "db size    : {:.1} MB",
            size_bytes as f64 / 1_048_576.0
        ),
    ];
    for (kind, c) in dicts {
        lines.push(format!("dict {kind:<7}: {c} entries"));
    }
    lines
}

/// The stderr note printed after `status --out` has written `file`
/// (`[status] записано → {file}` — the `[stats]` note style; there is no
/// `--json` mode here, so no JSON note).
fn status_out_note(file: &Path) -> String {
    format!("[status] записано → {}", file.display())
}

/// Print index statistics. A missing index is the historical muted line on
/// stderr plus exit 0 (the guard stays as-is: with `--out` no file is
/// written). With `--out` the exact stdout report goes to the file (parent
/// dirs created) and nothing prints to stdout: UTF-8 with a BOM
/// (Excel-friendly, the `stats --out` rule), one `[status] записано →
/// {file}` note on stderr.
fn cmd_status(db: &PathBuf, out: Option<&Path>) -> Result<()> {
    if !db.exists() {
        eprintln!("no index at {} (run `rada-client init` first)", db.display());
        return Ok(());
    }
    let conn = rada_db::open(db)?;
    let n = rada_db::doc_count(&conn)?;
    let last = rada_db::last_import(&conn);
    let size = std::fs::metadata(db).map(|m| m.len()).unwrap_or(0);
    let mut dicts = Vec::with_capacity(3);
    for kind in ["stan", "typs", "organs"] {
        let c: i64 = conn
            .query_row("SELECT COUNT(*) FROM dict WHERE kind=?1", [kind], |r| {
                r.get(0)
            })
            .unwrap_or(0);
        dicts.push((kind, c));
    }
    let lines = status_lines(db, n, last.as_deref(), size, dicts.try_into().unwrap());
    match out {
        Some(p) => {
            // UTF-8 BOM prefix: the same Excel-friendly convention as
            // search/arrivals/stats; the bytes are exactly the stdout
            // report (every row always renders, so never BOM-alone).
            write_out_file(p, format!("\u{FEFF}{}\n", lines.join("\n")).as_bytes())?;
            eprintln!("{}", status_out_note(p));
        }
        None => {
            for line in lines {
                println!("{line}");
            }
        }
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
        days_to_ymd, recent_days_from_civil, ymd_to_days,
        annot_json_body, annot_json_out_note, annot_list_out_note, annot_out_note,
        annot_search_lines, annot_search_match, annot_search_out_note, annot_table_lines,
        annot_text_file_note,
        annots_doc_json, annots_doc_lines, annots_doc_out_note, annots_limit,
        arrivals_date_key, arrivals_json_body, arrivals_json_out_note,
        arrivals_lines,
        arrivals_out_note, backfill_out_note, backfill_summary, backfill_summary_lines,
        cache_dump_filename, cache_dump_lines,
        cache_search_json_body, cache_search_json_out_note, cache_stats_lines,
        cache_stats_out_note,
        cache_search_out_note, card_json_body, card_json_out_note, card_lines, card_out_note,
        dict_fresh_json, dict_fresh_lines, dict_json_body,
        dict_json_out_note, dict_matches, dict_out_note, dict_plain_lines, dict_stats_json,
        dict_stats_lines, dict_stats_out_note, dates_json, dates_lines, dates_out_note,
        dicts_json, dicts_limit, dicts_lines, dicts_out_note, normalize_dicts_kind,
        check_docs_filters, docs_limit, docs_list_json, docs_list_lines, docs_list_out_note,
        export_html,
        grep_body, grep_render,
        export_markdown, export_out_note, export_plain, favorites_json_body,
        favorites_json_out_note, favorites_lines, favorites_out_note, facet_lines, filter_since,
        new_nregs, normalize_annots_kind, normalize_fav_kind, normalize_hits_format, normalize_month_year,
        normalize_recent_days, normalize_recent_limit, normalize_year_arg, check_year_range,
        hits_lines, hits_md_lines, hits_out_note,
        months_json, months_lines, months_out_note,
        years_json, years_lines, years_out_note,
        normalize_fmt, out_note, orgs_json, orgs_lines, orgs_out_note,
        recent_cutoff, recent_docs_json, recent_docs_lines, recent_docs_out_note,
        recent_json_body, recent_json_out_note, recent_lines,
        recent_out_note, related_json_body, related_json_out_note, related_out_note,
        resolve_json_body, resolve_json_out_note, resolve_out_note, search_out_note, since_note,
        status_lines, status_out_note,
        stats_lines, stats_out_note, statuses_json, statuses_lines, statuses_out_note,
        text_grep_out_note, today_yyyymmdd, toc_entries, toc_lines,
        toc_out_note, verify_lines, verify_out_note, DictStat, FavRow, FavSort, HitsFormat,
        OrgRow,
        RecentDocRow, RecentRow, StatsInfo, StatusRow, TocEntry, YearRow,
        MonthRow,
        VerifyCheck, VerifyStatus,
    };

    #[test]
    fn stats_lines_renders_aligned_keys_and_placeholders() {
        // Full report: every source present, values formatted, keys
        // column-aligned.
        let full = StatsInfo {
            db: "data/zakon-index.sqlite".into(),
            docs: Some(295340),
            size_bytes: Some(104_857_600), // exactly 100 MB
            last_import: Some("2026-10-28 07:30:00".into()),
            annotations: Some(12),
            dict_stan: Some(8),
            dict_typs: Some(40),
            dict_organs: Some(4213),
            cache_rows: Some(1234),
            cache_bytes: Some(10_485_760), // exactly 10 MB
        };
        let lines = stats_lines(&full);
        assert_eq!(lines.len(), 9);
        // keys are padded to one width: the separator ':' sits at the same
        // character column in every line (byte offsets would differ — keys
        // are multi-byte Cyrillic)
        let colon_col = |l: &str| l.chars().position(|c| c == ':').unwrap();
        assert_eq!(colon_col(&lines[0]), "останній імпорт/синк".chars().count() + 1);
        assert!(lines.iter().all(|l| colon_col(l) == colon_col(&lines[0])));
        assert!(lines[0].starts_with("файл індексу") && lines[0].contains("data/zakon-index.sqlite"));
        assert!(lines[1].starts_with("розмір індексу") && lines[1].contains("100.0 MB"));
        assert!(lines[2].starts_with("останній імпорт/синк") && lines[2].contains("2026-10-28 07:30:00"));
        assert!(lines[3].starts_with("документів") && lines[3].contains("295340"));
        assert!(lines[4].starts_with("анотацій") && lines[4].contains("12"));
        assert!(lines[5].starts_with("словник stan") && lines[5].contains("8"));
        assert!(lines[6].starts_with("словник typs") && lines[6].contains("40"));
        assert!(lines[7].starts_with("словник organs") && lines[7].contains("4213"));
        assert!(lines[8].starts_with("кеш текстів") && lines[8].contains("1234 тіл · 10.0 MB"));
        // Zero sources: empty dict tables and an empty cache render «порожньо».
        let zeros = StatsInfo {
            db: "data/none.sqlite".into(),
            docs: Some(0),
            size_bytes: Some(0),
            last_import: Some("".into()),
            annotations: Some(0),
            dict_stan: Some(0),
            dict_typs: Some(0),
            dict_organs: Some(0),
            cache_rows: Some(0),
            cache_bytes: Some(0),
        };
        let lines = stats_lines(&zeros);
        assert_eq!(lines.len(), 9);
        assert!(lines[5].contains("порожньо") && lines[6].contains("порожньо")
            && lines[7].contains("порожньо"), "empty dict tables → «порожньо»");
        assert!(lines[8].contains("порожньо"), "empty cache → «порожньо»");
    }

    #[test]
    fn stats_out_note_formats_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//s.txt");
        let line = stats_out_note(file);
        assert_eq!(line, format!("[stats] записано → {}", file.display()));
        assert!(line.starts_with("[stats] "));
        assert!(line.contains("записано"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
    }

    #[test]
    fn stats_lines_include_all_sources_and_skip_missing() {
        // All sources Some: every line renders, cache bytes grouped into MB
        // the same way the `cache` report formats them («4.8 MB» style).
        let all = StatsInfo {
            db: "data/full.sqlite".into(),
            docs: Some(10),
            size_bytes: Some(2_097_152), // exactly 2 MB
            last_import: Some("2026-10-01 10:00:00".into()),
            annotations: Some(3),
            dict_stan: Some(8),
            dict_typs: Some(40),
            dict_organs: Some(4213),
            cache_rows: Some(7),
            cache_bytes: Some(5_033_165), // ≈ 4.8 MB
        };
        let lines = stats_lines(&all);
        assert_eq!(lines.len(), 9);
        let joined = lines.join("\n");
        for label in [
            "файл індексу",
            "розмір індексу",
            "останній імпорт/синк",
            "документів",
            "анотацій",
            "словник stan",
            "словник typs",
            "словник organs",
            "кеш текстів",
        ] {
            assert!(joined.contains(label), "missing source line: {label}");
        }
        assert!(joined.contains("2.0 MB"), "index size grouped into MB");
        assert!(joined.contains("7 тіл · 4.8 MB"), "cache bytes grouped like `cache`");
        // Missing sources (None): their lines are absent; only what exists
        // renders, alignment still holds across the surviving lines.
        let sparse = StatsInfo {
            db: "data/partial.sqlite".into(),
            docs: Some(5),
            size_bytes: None,
            last_import: None,
            annotations: None,
            dict_stan: None,
            dict_typs: None,
            dict_organs: None,
            cache_rows: None,
            cache_bytes: None,
        };
        let lines = stats_lines(&sparse);
        assert_eq!(lines.len(), 2, "only the file line and the docs line survive");
        assert!(lines[0].starts_with("файл індексу") && lines[0].contains("data/partial.sqlite"));
        assert!(lines[1].starts_with("документів") && lines[1].contains("5"));
        let joined = lines.join("\n");
        for label in [
            "розмір індексу",
            "останній імпорт/синк",
            "анотацій",
            "словник stan",
            "словник typs",
            "словник organs",
            "кеш текстів",
        ] {
            assert!(!joined.contains(label), "None source must render no line: {label}");
        }
        // Even the minimal report keeps the two ':' separators in one column.
        let colon_col = |l: &str| l.chars().position(|c| c == ':').unwrap();
        assert_eq!(colon_col(&lines[0]), colon_col(&lines[1]));
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
    fn card_lines_renders_present_and_missing_fields() {
        // Full card: every optional surface present earns its own row,
        // in the fixed label order (dates via fmt_date, size in bytes).
        let full = rada_core::models::LawCardJson {
            nreg: "2755-17".into(),
            nazva: "  Про Закон  ".into(),
            status: 5,
            orgdat: Some(20241115),
            orgnum: Some(" 2755-IX ".into()),
            poddat: Some(20241201),
            pridat: Some(20250101),
            pidstava: Some("1234-IX".into()),
            eds: vec![
                rada_core::models::Edition {
                    datred: 20241115,
                    format: Some(1),
                    pages: None,
                    size: Some(9),
                    pidstava: None,
                },
                rada_core::models::Edition {
                    datred: 20241201,
                    format: None,
                    pages: None,
                    size: None,
                    pidstava: None,
                },
            ],
            minjust: Some("8765".into()),
            types: "1|2".into(),
            org: 42,
            size: Some(1234),
            is_archive: Some(true),
        };
        assert_eq!(
            card_lines(&full),
            vec![
                "Про Закон",
                "nreg    : 2755-17",
                "status  : Чинний (5)",
                "number  : 2755-IX",
                "adopted : 15.11.2024",
                "edition : 01.12.2024",
                "inforce : 01.01.2025",
                "basis   : 1234-IX",
                "types   : 1|2",
                "org     : 42",
                "minjust : 8765",
                "size    : 1234 bytes",
                "archive : так",
                "editions: 2",
            ]
        );
        // The current plain-printer convention for absent fields: NO row at
        // all (no dash placeholder). None, whitespace-only strings, org == 0,
        // is_archive != Some(true) and an empty eds list are all skipped;
        // nazva is trimmed, and the title + nreg/status/editions rows always
        // print.
        let bare = rada_core::models::LawCardJson {
            nreg: "1-1".into(),
            nazva: "Про Акт".into(),
            status: 1,
            orgnum: Some("   ".into()),
            pidstava: Some("".into()),
            minjust: Some(" ".into()),
            types: "  ".into(),
            is_archive: Some(false),
            ..Default::default()
        };
        assert_eq!(
            card_lines(&bare),
            vec![
                "Про Акт",
                "nreg    : 1-1",
                "status  : Втратив чинність (1)",
                "editions: 0",
            ]
        );
    }

    #[test]
    fn card_json_body_renders_fields_and_nulls() {
        // Exact pretty shape of the raw card Value: struct field order,
        // 2-space indent, absent optionals as explicit nulls (NOT omitted,
        // unlike the plain table), nested Edition camelCase fields too;
        // BOM-less machine data (the house JSON rule).
        let card = rada_core::models::LawCardJson {
            nreg: "2755-17".into(),
            nazva: "Про Закон".into(),
            status: 5,
            orgdat: Some(20241115),
            eds: vec![rada_core::models::Edition {
                datred: 20241115,
                format: Some(1),
                pages: None,
                size: None,
                pidstava: None,
            }],
            ..Default::default()
        };
        let body = card_json_body(&card).unwrap();
        let expected = r#"{
  "nreg": "2755-17",
  "nazva": "Про Закон",
  "status": 5,
  "orgdat": 20241115,
  "orgnum": null,
  "poddat": null,
  "pridat": null,
  "pidstava": null,
  "eds": [
    {
      "datred": 20241115,
      "format": 1,
      "pages": null,
      "size": null,
      "pidstava": null
    }
  ],
  "minjust": null,
  "types": "",
  "org": 0,
  "size": null,
  "isArchive": null
}"#;
        assert_eq!(body, expected);
        assert!(!body.starts_with('\u{FEFF}'));
        // the shared dispatcher renders the very same bytes for both modes
        assert_eq!(super::card_body(&card, true).unwrap(), body);
    }

    #[test]
    fn card_out_note_and_json_note_format() {
        // plain note keeps the single-object label (mirrors [export]),
        // the JSON note uses the house «записано JSON» wording
        let file = std::path::Path::new("out//card.txt");
        assert_eq!(
            card_out_note("2755-17", file),
            format!("[card] 2755-17 → {}", file.display())
        );
        let jfile = std::path::Path::new("out//card.json");
        assert_eq!(
            card_json_out_note(jfile),
            format!("[card] записано JSON → {}", jfile.display())
        );
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
    fn annot_search_out_note_formats_count_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//as.txt");
        let line = annot_search_out_note(4, file);
        assert_eq!(line, format!("[annot] записано 4 збігів → {}", file.display()));
        assert!(line.starts_with("[annot] записано "));
        assert!(line.contains("4"));
        assert!(line.contains("збігів"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
        // the house zero-hit rule: the note still formats with n = 0
        assert_eq!(
            annot_search_out_note(0, file),
            format!("[annot] записано 0 збігів → {}", file.display())
        );
    }

    #[test]
    fn annot_search_match_covers_the_three_surfaces() {
        let a = rada_db::Annotation {
            id: 7,
            nreg: "1556-18".into(),
            kind: "highlight".into(),
            text: "Стаття 3 — Вища освіта".into(),
            span_start: Some(12),
            span_end: Some(17),
            created_at: "2026-10-28 07:30:00".into(),
        };
        // text surface, case-insensitive both ways (plain to_lowercase
        // folds Ukrainian letters: «В»/«в», «Я»/«я»)
        assert!(annot_search_match(&a, "вища освіта"));
        assert!(annot_search_match(&a, "стаття 3"));
        // kind surface
        assert!(annot_search_match(&a, "highli"));
        // nreg surface
        assert!(annot_search_match(&a, "1556"));
        // no surface carries the needle
        assert!(!annot_search_match(&a, "податков"));
        assert!(!annot_search_match(&a, "bookmark"));
        assert!(!annot_search_match(&a, "9-99"));
        // empty needle matches everything (the GUI favorites parity)
        assert!(annot_search_match(&a, ""));
        // the body builder: header + the shared `annot list` rows, and the
        // zero-match header (the stdout-only zero-hit line)
        let rows = annot_table_lines(&[a.clone()]);
        let lines = annot_search_lines(&[a], "освіта");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "1 анотацій з «освіта»:".to_string());
        assert_eq!(lines[1], rows[0]);
        assert_eq!(
            annot_search_lines(&[], "нічого"),
            vec!["0 анотацій з «нічого»:".to_string()]
        );
    }

    #[test]
    fn annot_json_body_renders_fields_and_empty() {
        // stdout prints EXACTLY this body (+ newline); the `--out` file
        // receives the same bytes BOM-less (+ trailing newline) — shared by
        // `annot list --json` and `annot search --json` so neither pair can
        // drift. Field order follows `rada_db::Annotation` (the historical
        // inline shape); missing spans render as `null`.
        let items = vec![
            rada_db::Annotation {
                id: 7,
                nreg: "1556-18".into(),
                kind: "highlight".into(),
                text: "Стаття 3 — Вища освіта".into(),
                span_start: Some(12),
                span_end: Some(17),
                created_at: "2026-10-28 07:30:00".into(),
            },
            rada_db::Annotation {
                id: 8,
                nreg: "2341-14".into(),
                kind: "bookmark".into(),
                text: String::new(),
                span_start: None,
                span_end: None,
                created_at: "2026-10-29 09:00:00".into(),
            },
        ];
        let body = annot_json_body(&items);
        assert_eq!(
            body,
            "[\n  {\n    \"id\": 7,\n    \"nreg\": \"1556-18\",\n    \"kind\": \"highlight\",\n    \"text\": \"Стаття 3 — Вища освіта\",\n    \"span_start\": 12,\n    \"span_end\": 17,\n    \"created_at\": \"2026-10-28 07:30:00\"\n  },\n  {\n    \"id\": 8,\n    \"nreg\": \"2341-14\",\n    \"kind\": \"bookmark\",\n    \"text\": \"\",\n    \"span_start\": null,\n    \"span_end\": null,\n    \"created_at\": \"2026-10-29 09:00:00\"\n  }\n]"
        );
        assert!(!body.starts_with('\u{FEFF}'), "JSON body stays BOM-less");
        // the shared empty form: both commands print a pretty `[]` array
        // (zero annotations for the nreg / zero matches for the term)
        assert_eq!(annot_json_body(&[]), "[]");
    }

    #[test]
    fn annot_json_out_note_formats_json_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//a.json");
        let line = annot_json_out_note(file);
        assert_eq!(line, format!("[annot] записано JSON → {}", file.display()));
        assert!(line.starts_with("[annot] записано "));
        assert!(line.contains("JSON"));
        assert!(line.contains('→'));
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
    fn related_json_out_note_formats_json_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//r.json");
        let line = related_json_out_note(file);
        assert_eq!(line, format!("[related] записано JSON → {}", file.display()));
        assert!(line.starts_with("[related] записано "));
        assert!(line.contains("JSON"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
    }

    #[test]
    fn related_json_body_renders_items_and_empty() {
        // stdout prints EXACTLY this body (+ newline); the `--out` file
        // receives the same bytes BOM-less (+ trailing newline). Map keys
        // sort, so «nreg» precedes «title» (the historical inline shape).
        let docs = vec![
            rada_core::related::RelatedDoc {
                nreg: "2341-14".to_string(),
                title: "Про затвердження".to_string(),
            },
            rada_core::related::RelatedDoc {
                nreg: "254к/96-вр".to_string(),
                title: "Регламент".to_string(),
            },
        ];
        let body = related_json_body(&docs).unwrap();
        assert_eq!(
            body,
            "{\n  \"items\": [\n    {\n      \"nreg\": \"2341-14\",\n      \"title\": \"Про затвердження\"\n    },\n    {\n      \"nreg\": \"254к/96-вр\",\n      \"title\": \"Регламент\"\n    }\n  ]\n}"
        );
        assert!(!body.starts_with('\u{FEFF}'), "JSON body stays BOM-less");
        // zero links: a normal result, the dict empty style `"items": []`
        assert_eq!(related_json_body(&[]).unwrap(), "{\n  \"items\": []\n}");
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
    fn cache_dump_filename_sanitizes_and_caps() {
        // a normal nreg passes through unchanged
        assert_eq!(cache_dump_filename("2755-17"), "2755-17");
        assert_eq!(cache_dump_filename("1556-18_2.ed1"), "1556-18_2.ed1");
        // slashes (and colons) are replaced
        assert_eq!(cache_dump_filename("2755-17/ed2024"), "2755-17_ed2024");
        // the whole Windows-reserved residue goes to '_'
        assert_eq!(
            cache_dump_filename("a:b*c?d/e\\f<g>h|i"),
            "a_b_c_d_e_f_g_h_i"
        );
        // empty or whitespace-only garbage → "doc"
        assert_eq!(cache_dump_filename(""), "doc");
        assert_eq!(cache_dump_filename("   \t "), "doc");
        // a 200-char input is capped at 80 chars
        let capped = cache_dump_filename(&"x".repeat(200));
        assert_eq!(capped.chars().count(), 80);
        assert_eq!(capped, "x".repeat(80));
        // non-ASCII letters are not kept: every «йц» char is replaced
        assert_eq!(cache_dump_filename("йц"), "__");
    }

    #[test]
    fn cache_dump_lines_summary_and_empty() {
        // double separator keeps the test path-separator-stable via display()
        let dir = std::path::Path::new("out//dump");
        let lines = cache_dump_lines(2, dir);
        assert_eq!(lines.len(), 1, "one summary line only");
        assert!(lines[0].contains("документів записано: 2"), "{}", lines[0]);
        assert!(lines[0].contains(&dir.display().to_string()));
        assert!(lines[0].contains('→'));
        // empty cache: exactly the zero message, still one line (exit 0)
        assert_eq!(
            cache_dump_lines(0, dir),
            vec!["кеш текстів порожній — нічого експортувати".to_string()]
        );
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
        // the house zero-result rule: every requested kind keeps its note
        // even with n = 0 (the BOM-only file case, mirrored from `--search`
        // zero matches)
        assert_eq!(
            dict_out_note("typs", 0, file),
            format!("[dict] typs: записано 0 записів → {}", file.display())
        );
    }

    #[test]
    fn dict_json_out_note_formats_json_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//d.json");
        let line = dict_json_out_note(file);
        assert_eq!(line, format!("[dict] записано JSON → {}", file.display()));
        assert!(line.starts_with("[dict] записано "));
        assert!(line.contains("JSON"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
    }

    #[test]
    fn dict_plain_lines_body_equals_stdout_rows_and_out_bytes() {
        // the table body shared by stdout and `--out`: in all-dicts mode a
        // `{k} (caption):` header precedes every non-empty group, rows are
        // `  {id}  {label}`; empty groups print nothing (no header either)
        let groups = vec![
            (
                "stan".to_string(),
                vec![(0i64, "Не визначено".to_string()), (5i64, "Чинний".to_string())],
            ),
            ("typs".to_string(), Vec::new()),
            ("organs".to_string(), vec![(1i64, "Президент".to_string())]),
        ];
        let lines = dict_plain_lines(&groups, true, false);
        assert_eq!(
            lines,
            vec![
                "stan (стани):".to_string(),
                "  0  Не визначено".to_string(),
                "  5  Чинний".to_string(),
                "organs (органи):".to_string(),
                "  1  Президент".to_string(),
            ]
        );
        // the `--out` file body equals exactly those stdout lines with the
        // Excel-friendly BOM prefix and one trailing newline (the bytes the
        // cmd_dict --out branch writes)
        let body = format!("\u{FEFF}{}\n", lines.join("\n"));
        assert!(body.starts_with('\u{FEFF}'));
        assert_eq!(&body.as_bytes()[3..], format!("{}\n", lines.join("\n")).as_bytes());
        // single-kind mode (--kind): the same rows, no group header
        let single = vec![("stan".to_string(), vec![(5i64, "Чинний".to_string())])];
        assert_eq!(dict_plain_lines(&single, false, false), vec!["  5  Чинний".to_string()]);
        // an unknown kind's caption repeats its own name
        let weird = vec![("xyz".to_string(), vec![(7i64, "Етикетка".to_string())])];
        assert_eq!(
            dict_plain_lines(&weird, true, false),
            vec!["xyz (xyz):".to_string(), "  7  Етикетка".to_string()]
        );
        // --search filtered every group to zero rows: the single
        // «нічого не знайдено» line replaces the listing on stdout AND in
        // the BOM body (this mode's established zero rule, unchanged)
        let none = vec![
            ("stan".to_string(), Vec::new()),
            ("typs".to_string(), Vec::new()),
        ];
        assert_eq!(dict_plain_lines(&none, true, true), vec!["нічого не знайдено".to_string()]);
        // without --search an all-empty listing renders no lines at all:
        // empty stdout and a BOM-only `--out` file
        assert!(dict_plain_lines(&none, true, false).is_empty());
    }

    #[test]
    fn dict_json_body_renders_dicts_and_empty_items() {
        // stdout prints EXACTLY this body (+ newline); the `--out` file
        // receives the same bytes BOM-less (+ trailing newline). Map keys
        // sort, so «items» precedes «kind» (the historical inline shape).
        let groups = vec![(
            "stan".to_string(),
            vec![(0i64, "Не визначено".to_string()), (5i64, "Чинний".to_string())],
        )];
        let body = dict_json_body(&groups).unwrap();
        assert_eq!(
            body,
            "{\n  \"dicts\": [\n    {\n      \"items\": [\n        {\n          \"id\": 0,\n          \"label\": \"Не визначено\"\n        },\n        {\n          \"id\": 5,\n          \"label\": \"Чинний\"\n        }\n      ],\n      \"kind\": \"stan\"\n    }\n  ]\n}"
        );
        assert!(!body.starts_with('\u{FEFF}'), "JSON body stays BOM-less");
        // an empty group is a normal result: `"items": []`, not an error
        let empty = dict_json_body(&[("typs".to_string(), Vec::new())]).unwrap();
        assert!(empty.contains("\"kind\": \"typs\""), "{empty}");
        assert!(empty.contains("\"items\": []"), "{empty}");
        assert!(dict_json_body(&[]).unwrap().contains("\"dicts\": []"));
    }

    #[test]
    fn dict_fresh_json_renders_counts_and_zero() {
        // pretty {"fresh": {kind: n}} over the (kind, count) pairs; Map
        // orders keys, empty tables report 0 (plain report: «порожньо»)
        let body = dict_fresh_json(&[("typs", 138), ("stan", 10), ("organs", 0)]).unwrap();
        assert_eq!(
            body,
            "{\n  \"fresh\": {\n    \"organs\": 0,\n    \"stan\": 10,\n    \"typs\": 138\n  }\n}"
        );
        assert!(!body.starts_with('\u{FEFF}'), "JSON report stays BOM-less");
        // no kinds at all: still a well-formed empty report
        assert_eq!(dict_fresh_json(&[]).unwrap(), "{\n  \"fresh\": {}\n}");
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
    fn backfill_summary_lines_renders_counters() {
        // live run: the single counter line, exact wording, comma/semicolon
        // separators (the bytes stdout prints and the `--out` file carries)
        assert_eq!(
            backfill_summary_lines(3, 5, 1, 1234, false),
            vec!["отримано 3, вже в кеші 5, помилок 1; кеш тепер: 1234 тіл".to_string()]
        );
        // dry run: the same line with the «(сухий прогін)» tail
        assert_eq!(
            backfill_summary_lines(0, 2, 0, 7, true),
            vec!["отримано 0, вже в кеші 2, помилок 0; кеш тепер: 7 тіл (сухий прогін)".to_string()]
        );
        // zero-run shape: all counters render their zeros (never omitted)
        assert_eq!(
            backfill_summary_lines(0, 0, 0, 0, false),
            vec!["отримано 0, вже в кеші 0, помилок 0; кеш тепер: 0 тіл".to_string()]
        );
    }

    #[test]
    fn backfill_out_note_formats_lines_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//b.txt");
        let line = backfill_out_note(1, file);
        assert!(line.starts_with("[backfill] записано "));
        assert!(line.contains("1"));
        assert!(line.contains("рядків звіту"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
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

    #[test]
    fn grep_body_merges_context_blocks() {
        // ten lines, case-insensitive matches at lines 3 and 7 (4 apart)
        let body = "l1\nl2\nПодатковий A\nl4\nl5\nl6\nподатковий B\nl8\nl9\nl10";
        // context 0: only the two match lines, 1-based numbers + flags
        assert_eq!(
            grep_body(body, "податковий", 0),
            vec![
                (3, "Податковий A".to_string(), true),
                (7, "податковий B".to_string(), true),
            ]
        );
        // matches 4 lines apart: context 1 keeps the untouched neighbours
        // (is_match = false) and the blocks stay DISJOINT (gap at line 5)
        let kept = grep_body(body, "податковий", 1);
        assert_eq!(
            kept,
            vec![
                (2, "l2".to_string(), false),
                (3, "Податковий A".to_string(), true),
                (4, "l4".to_string(), false),
                (6, "l6".to_string(), false),
                (7, "податковий B".to_string(), true),
                (8, "l8".to_string(), false),
            ]
        );
        // context 3: the windows [1..6] and [4..10] overlap → one MERGED
        // gap-free ascending run of all ten lines, match flags preserved
        let merged = grep_body(body, "податковий", 3);
        assert_eq!(merged.len(), 10);
        assert_eq!(merged[0].0, 1);
        for w in merged.windows(2) {
            assert_eq!(w[1].0, w[0].0 + 1, "merged blocks must be gap-free");
        }
        assert_eq!(merged.iter().filter(|(_, _, m)| *m).count(), 2);
        assert!(merged[2].2 && merged[6].2, "lines 3 and 7 are the matches");
        assert!(!merged[0].2 && !merged[9].2, "context lines are not matches");
        // no match ⇒ nothing kept
        assert!(grep_body(body, "немає-такого", 1).is_empty());
        // the needle is trimmed once and case-folded (plain to_lowercase)
        assert_eq!(grep_body("Кодекс", "  КОДЕКС ", 0).len(), 1);
    }

    #[test]
    fn grep_render_separates_blocks() {
        // two disjoint blocks (gap at line 5): «--» between them,
        // every kept line prefixed «{n}: {line}»
        let kept = vec![
            (2, "l2".to_string(), false),
            (3, "Податковий A".to_string(), true),
            (4, "l4".to_string(), false),
            (6, "l6".to_string(), false),
            (7, "податковий B".to_string(), true),
        ];
        assert_eq!(
            grep_render(&kept),
            vec![
                "2: l2".to_string(),
                "3: Податковий A".to_string(),
                "4: l4".to_string(),
                "--".to_string(),
                "6: l6".to_string(),
                "7: податковий B".to_string(),
            ]
        );
        // one consecutive block: no separator at the edges
        let one = vec![(1, "a".to_string(), true), (2, "b".to_string(), false)];
        assert_eq!(
            grep_render(&one),
            vec!["1: a".to_string(), "2: b".to_string()]
        );
        // empty input renders no lines
        assert!(grep_render(&[]).is_empty());
        // the stderr note printed after `--grep --out` has written the file
        let file = std::path::Path::new("out//g.txt");
        let note = text_grep_out_note(5, file);
        assert_eq!(
            note,
            format!("[text] записано 5 рядків → {}", file.display())
        );
        assert!(note.starts_with("[text] записано "));
        assert!(note.contains("рядків"));
        assert!(note.contains('→'));
        assert!(note.contains(&file.display().to_string()));
    }

    #[test]
    fn toc_entries_recognizes_sections_and_skips_prose() {
        // a «Стаття …» line beyond the 120-char cap: a body paragraph, not
        // a heading
        let long = format!("Стаття 99. {}", "дуже довгий текст ".repeat(12));
        assert!(long.chars().count() > 120, "the probe line exceeds the cap");
        let body = [
            "РОЗДІЛ I. Загальні положення",
            "глава 2. Інше",
            "стаття 1. Визначення",
            "  Стаття 2.   Із відступами   ",
            long.as_str(),
            "Звичайний абзац про податки та збори.",
            "Розділяй берег — це проза, не заголовок",
            "Посилання на статтю 5 іншого акта.",
            "",
            "   \t ",
        ]
        .join("\n");
        let entries = toc_entries(&body);
        // recognized: keyword as the whole first word, any case; trimmed
        // text stored; the over-long «Стаття» line and every prose line
        // (including the «Розділяй …» word-boundary trap) are skipped
        assert_eq!(
            entries.iter().map(|e| e.text.as_str()).collect::<Vec<_>>(),
            vec![
                "РОЗДІЛ I. Загальні положення",
                "глава 2. Інше",
                "стаття 1. Визначення",
                "Стаття 2.   Із відступами",
            ]
        );
    }

    #[test]
    fn toc_entries_indent_levels_and_order() {
        let body = "Розділ I. Загальні засади\nСтаття 1. Перша\nСтаття 2. Друга\n\
                    Глава 3. Особлива частина\nСтаття 3. Третя";
        let entries = toc_entries(body);
        // document order preserved, levels from the GUI classification
        // («Розділ/Глава» → 1, «Стаття» → 2)
        assert_eq!(
            entries.iter().map(|e| e.level).collect::<Vec<_>>(),
            vec![1, 2, 2, 1, 2]
        );
        assert_eq!(entries[0].text, "Розділ I. Загальні засади");
        assert_eq!(entries[4].text, "Стаття 3. Третя");
        // the shared table body: header + level 1 flush left, level 2 indented
        let direct = vec![
            TocEntry { level: 1, text: "Розділ I".into() },
            TocEntry { level: 2, text: "Стаття 1".into() },
        ];
        assert_eq!(
            toc_lines(&direct),
            vec![
                "2 розділів(и) та статей:".to_string(),
                "Розділ I".to_string(),
                "  Стаття 1".to_string(),
            ]
        );
        // an act without sections: empty body → empty outline (valid, n=0)
        assert!(toc_entries("").is_empty());
        assert!(toc_entries("\n  \n\t\n").is_empty());
        assert_eq!(
            toc_lines(&toc_entries("проза без заголовків")),
            vec!["0 розділів(и) та статей:".to_string()]
        );
    }

    #[test]
    fn toc_out_note_formats_count_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//t.txt");
        let line = toc_out_note(45, file);
        assert_eq!(
            line,
            format!("[toc] записано 45 заголовків → {}", file.display())
        );
        assert!(line.starts_with("[toc] записано "));
        assert!(line.contains("45"));
        assert!(line.contains("заголовків"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
        // the house zero-heading rule: the note still formats with n = 0
        assert_eq!(
            toc_out_note(0, file),
            format!("[toc] записано 0 заголовків → {}", file.display())
        );
    }

    #[test]
    fn verify_lines_all_ok_renders_healthy_verdict() {
        let ok = |name: &str, detail: &str| VerifyCheck {
            name: name.into(),
            status: VerifyStatus::Ok,
            detail: detail.into(),
        };
        let checks = vec![
            ok("цілість SQLite", "ok"),
            ok("зовнішні ключі", "ok"),
            ok("документи", "295340"),
            ok("повнотекстовий індекс", "295340"),
            ok("дублікати nreg", "немає"),
            ok("кеш текстів", "12"),
        ];
        let lines = verify_lines(&checks);
        assert_eq!(lines.len(), 7, "six checks plus the verdict line");
        // every check line carries the plain ok prefix (house style: no
        // decorative glyphs) in the «{mark} {name}: {detail}» shape
        for line in &lines[..6] {
            assert!(line.starts_with("ok "), "{line}");
        }
        assert_eq!(lines[2], "ok документи: 295340");
        assert_eq!(lines[4], "ok дублікати nreg: немає");
        // the verdict is derived inside the builder: no Fail → healthy
        assert!(lines[6].contains("база здорова"), "{}", lines[6]);
        assert!(lines[6].starts_with("вердикт: "), "{}", lines[6]);
        // no checks at all: still just the healthy verdict line
        assert_eq!(verify_lines(&[]), vec!["вердикт: база здорова".to_string()]);
    }

    #[test]
    fn verify_lines_mixed_statuses_mark_problems() {
        let check = |name: &str, status: VerifyStatus, detail: &str| VerifyCheck {
            name: name.into(),
            status,
            detail: detail.into(),
        };
        let mixed = vec![
            check("цілість SQLite", VerifyStatus::Ok, "ok"),
            check("документи", VerifyStatus::Warn, "0 — індекс порожній"),
            check("повнотекстовий індекс", VerifyStatus::Fail, "відсутній"),
        ];
        let lines = verify_lines(&mixed);
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0], "ok цілість SQLite: ok");
        // statuses render as the plain «увага»/«помилка» prefixes
        assert!(lines[1].starts_with("увага "), "{}", lines[1]);
        assert!(lines[1].contains("0 — індекс порожній"));
        assert!(lines[2].starts_with("помилка "), "{}", lines[2]);
        assert!(lines[2].contains("відсутній"));
        // one Fail anywhere → the problems verdict
        assert!(lines[3].starts_with("вердикт: "), "{}", lines[3]);
        assert!(lines[3].contains("є проблеми"), "{}", lines[3]);
        // warnings alone stay healthy: no Fail → «база здорова»
        let warned = vec![
            check("цілість SQLite", VerifyStatus::Ok, "ok"),
            check("документи", VerifyStatus::Warn, "0 — індекс порожній"),
        ];
        let lines = verify_lines(&warned);
        assert!(lines[1].starts_with("увага "), "{}", lines[1]);
        assert!(lines[2].contains("база здорова"), "{}", lines[2]);
    }

    #[test]
    fn verify_out_note_formats_both_verdicts() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//v.txt");
        let line = verify_out_note(true, file);
        assert_eq!(
            line,
            format!("[verify] база здорова → {}", file.display())
        );
        assert!(line.starts_with("[verify] "));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
        let line = verify_out_note(false, file);
        assert_eq!(
            line,
            format!("[verify] є проблеми → {}", file.display())
        );
        assert!(line.starts_with("[verify] "));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
    }

    #[test]
    fn dict_stats_lines_renders_header_kinds_and_empty_flavor() {
        // three kinds in the stan/typs/organs order: header, one line per
        // kind, and the zero-row kind renders the «словник порожній» warn
        // flavor (informational — the command still exits 0)
        let stats = [
            DictStat { kind: "stan", label: "стани", rows: 10 },
            DictStat { kind: "typs", label: "типи", rows: 0 },
            DictStat { kind: "organs", label: "органи", rows: 799 },
        ];
        assert_eq!(
            dict_stats_lines(&stats),
            vec![
                "словники довідників:".to_string(),
                "стани: 10 значень".to_string(),
                "типи: 0 значень — словник порожній".to_string(),
                "органи: 799 значень".to_string(),
            ]
        );
        // input order is preserved even when it is not the default one
        let flipped = [
            DictStat { kind: "organs", label: "органи", rows: 1 },
            DictStat { kind: "stan", label: "стани", rows: 2 },
        ];
        assert_eq!(
            dict_stats_lines(&flipped),
            vec![
                "словники довідників:".to_string(),
                "органи: 1 значень".to_string(),
                "стани: 2 значень".to_string(),
            ]
        );
        // no kinds at all: the header still renders (the report always has
        // at least one line, so the `--out` file is never BOM-alone)
        assert_eq!(dict_stats_lines(&[]), vec!["словники довідників:".to_string()]);
    }

    #[test]
    fn dict_stats_json_renders_dicts_and_total() {
        // stdout prints EXACTLY this body (+ newline); the `--out` file
        // receives the same bytes BOM-less (+ trailing newline). Map keys
        // sort, so «dicts» precedes «total» and «kind»/«label»/«rows» keep
        // their alphabetical order inside every entry.
        let stats = [
            DictStat { kind: "stan", label: "стани", rows: 10 },
            DictStat { kind: "typs", label: "типи", rows: 138 },
            DictStat { kind: "organs", label: "органи", rows: 0 },
        ];
        let body = dict_stats_json(&stats);
        assert_eq!(
            body,
            "{\n  \"dicts\": [\n    {\n      \"kind\": \"stan\",\n      \"label\": \"стани\",\n      \"rows\": 10\n    },\n    {\n      \"kind\": \"typs\",\n      \"label\": \"типи\",\n      \"rows\": 138\n    },\n    {\n      \"kind\": \"organs\",\n      \"label\": \"органи\",\n      \"rows\": 0\n    }\n  ],\n  \"total\": 148\n}"
        );
        assert!(!body.starts_with('\u{FEFF}'), "JSON body stays BOM-less");
        // no kinds at all: a well-formed empty report with total 0
        assert_eq!(dict_stats_json(&[]), "{\n  \"dicts\": [],\n  \"total\": 0\n}");
    }

    #[test]
    fn dict_stats_out_note_formats_total_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//ds.txt");
        let line = dict_stats_out_note(947, file);
        assert_eq!(
            line,
            format!("[dict-stats] записано 947 значень → {}", file.display())
        );
        assert!(line.starts_with("[dict-stats] "));
        assert!(line.contains("947"));
        assert!(line.contains("значень"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
        // the informational zero rule: the note still formats with total = 0
        assert_eq!(
            dict_stats_out_note(0, file),
            format!("[dict-stats] записано 0 значень → {}", file.display())
        );
    }

    #[test]
    fn dicts_lines_renders_headers_rows_verdict_and_empty() {
        // unfiltered: «Словник {kind}:» header + one «{id}  {label}» row
        // per entry in input order (the SQL orders id ASC) + the «разом»
        // verdict over the RENDERED rows (post-`--limit`, the `annots`
        // rule)
        let rows = vec![
            (1i64, "Чинний".to_string()),
            (5i64, "Втратив чинність".to_string()),
        ];
        assert_eq!(
            dicts_lines("stan", None, &rows),
            vec![
                "Словник stan:".to_string(),
                "1  Чинний".to_string(),
                "5  Втратив чинність".to_string(),
                "разом: 2".to_string(),
            ]
        );
        // filtered: the query echoes into the header
        assert_eq!(
            dicts_lines("organs", Some("міністер"), &rows[..1]),
            vec![
                "Словник organs: міністер".to_string(),
                "1  Чинний".to_string(),
                "разом: 1".to_string(),
            ]
        );
        // empty dictionary / zero matches: the informational empty triple
        // (header + «нічого не знайдено» + «разом: 0», exit 0) — the
        // header always renders, so the `--out` file is never BOM-alone
        assert_eq!(
            dicts_lines("typs", Some("гадя"), &[]),
            vec![
                "Словник typs: гадя".to_string(),
                "нічого не знайдено".to_string(),
                "разом: 0".to_string(),
            ]
        );
        assert_eq!(
            dicts_lines("stan", None, &[]),
            vec![
                "Словник stan:".to_string(),
                "нічого не знайдено".to_string(),
                "разом: 0".to_string(),
            ]
        );
    }

    #[test]
    fn dicts_json_renders_alphabetical_keys_and_null_query() {
        // stdout prints EXACTLY this body (+ newline); the `--out` file
        // receives the same bytes BOM-less (+ trailing newline). Map keys
        // sort, so «count» precedes «kind» precedes «query» precedes
        // «rows», and the row keys keep alphabetical order inside every
        // entry; `count` counts the LISTED rows (post-`--limit`)
        let rows = vec![
            (1i64, "Чинний".to_string()),
            (5i64, "Втратив чинність".to_string()),
        ];
        let body = dicts_json("stan", Some("чинн"), &rows);
        assert_eq!(
            body,
            "{\n  \"count\": 2,\n  \"kind\": \"stan\",\n  \"query\": \"чинн\",\n  \"rows\": [\n    {\n      \"id\": 1,\n      \"label\": \"Чинний\"\n    },\n    {\n      \"id\": 5,\n      \"label\": \"Втратив чинність\"\n    }\n  ]\n}"
        );
        assert!(!body.starts_with('\u{FEFF}'), "JSON body stays BOM-less");
        // unfiltered + no rows: the query renders as JSON null (the
        // `card`/`dates` null-field style) and the rows array is empty
        assert_eq!(
            dicts_json("typs", None, &[]),
            "{\n  \"count\": 0,\n  \"kind\": \"typs\",\n  \"query\": null,\n  \"rows\": []\n}"
        );
    }

    #[test]
    fn dicts_out_note_formats_count_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//d.txt");
        let line = dicts_out_note(947, file);
        assert_eq!(
            line,
            format!("[dicts] записано 947 записів → {}", file.display())
        );
        assert!(line.starts_with("[dicts] "));
        assert!(line.contains("947"));
        assert!(line.contains("записів"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
        // the informational zero rule: the note still formats with 0 rows
        assert_eq!(
            dicts_out_note(0, file),
            format!("[dicts] записано 0 записів → {}", file.display())
        );
    }

    #[test]
    fn dicts_kind_and_limit_validation() {
        // `dicts KIND` folds the three canonical portal tokens
        // case-insensitively and trims surrounding spaces (the GUI home
        // panel shows стани/типи/органи only as display hints, so no
        // Ukrainian input aliases exist); garbage fails with the house
        // kind-error message (exit 1 through `main`); `--limit` clamps to
        // 1..=1000 (never a hard error, the `docs_limit` budget idiom)
        assert_eq!(normalize_dicts_kind("stan").unwrap(), "stan");
        assert_eq!(normalize_dicts_kind("TYPs").unwrap(), "typs");
        assert_eq!(normalize_dicts_kind(" Organs ").unwrap(), "organs");
        assert_eq!(normalize_dicts_kind("ORGANS").unwrap(), "organs");
        let err = normalize_dicts_kind("garbage").unwrap_err().to_string();
        assert_eq!(err, "невідомий словник garbage (stan|typs|organs)");
        assert_eq!(dicts_limit(100), 100);
        assert_eq!(dicts_limit(1), 1);
        assert_eq!(dicts_limit(1000), 1000);
        assert_eq!(dicts_limit(0), 1);
        assert_eq!(dicts_limit(1001), 1000);
        assert_eq!(dicts_limit(usize::MAX), 1000);
    }

    #[test]
    fn recent_lines_renders_header_rows_and_empty() {
        // header + one compact block per row: «[{kind}] {title||nreg} ·
        // {created_at}» (stored UTC stamp verbatim) + the indented text
        // (trimmed, newlines folded, 60-char cap like `annot list`); an
        // unindexed nreg (empty title) renders its own nreg
        let rows = vec![
            RecentRow {
                nreg: "1556-18".into(),
                kind: "highlight".into(),
                created_at: "2026-10-29 09:00:00".into(),
                text: "Стаття 3 — Вища освіта".into(),
                title: "Про вищу освіту".into(),
            },
            RecentRow {
                nreg: "2341-14".into(),
                kind: "comment".into(),
                created_at: "2026-10-28 07:30:00".into(),
                text: "  корисний\nзакон  ".into(),
                title: String::new(),
            },
        ];
        let lines = recent_lines(&rows);
        assert_eq!(
            lines,
            vec![
                "Останні анотації (2):".to_string(),
                "[highlight] Про вищу освіту · 2026-10-29 09:00:00".to_string(),
                "  Стаття 3 — Вища освіта".to_string(),
                "[comment] 2341-14 · 2026-10-28 07:30:00".to_string(),
                "  корисний закон".to_string(),
            ]
        );
        // a 200-char text is capped at 60 chars (the `annot list` style)
        let long = RecentRow {
            nreg: "1-1".into(),
            kind: "comment".into(),
            created_at: "2026-10-30 00:00:00".into(),
            text: "д".repeat(200),
            title: String::new(),
        };
        let lines = recent_lines(&[long]);
        assert_eq!(lines.len(), 3, "header + row line + one text line");
        assert_eq!(lines[1], "[comment] 1-1 · 2026-10-30 00:00:00");
        assert_eq!(lines[2].chars().count(), 2 + 60, "indented 60-char text");
        // an empty text (bookmark) renders no text line: header + row line
        let bm = RecentRow {
            nreg: "9-9".into(),
            kind: "bookmark".into(),
            created_at: "2026-10-30 10:00:00".into(),
            text: "   ".into(),
            title: "Титул".into(),
        };
        assert_eq!(
            recent_lines(&[bm]),
            vec![
                "Останні анотації (1):".to_string(),
                "[bookmark] Титул · 2026-10-30 10:00:00".to_string(),
            ]
        );
        // zero rows: the stdout-only «немає» line (the `--out` file then
        // carries the BOM alone, the house `search --out` rule)
        assert_eq!(
            recent_lines(&[]),
            vec!["Останніх анотацій немає".to_string()]
        );
    }

    #[test]
    fn recent_json_body_renders_rows_and_empty() {
        // stdout prints EXACTLY this body (+ newline); the `--out` file
        // receives the same bytes BOM-less (+ trailing newline). Map keys
        // sort (the house JSON style).
        let rows = vec![RecentRow {
            nreg: "1556-18".into(),
            kind: "highlight".into(),
            created_at: "2026-10-29 09:00:00".into(),
            text: "Стаття 3 — Вища освіта".into(),
            title: "Про вищу освіту".into(),
        }];
        let body = recent_json_body(&rows);
        assert_eq!(
            body,
            "[\n  {\n    \"created_at\": \"2026-10-29 09:00:00\",\n    \"kind\": \"highlight\",\n    \"nreg\": \"1556-18\",\n    \"text\": \"Стаття 3 — Вища освіта\",\n    \"title\": \"Про вищу освіту\"\n  }\n]"
        );
        assert!(!body.starts_with('\u{FEFF}'), "JSON body stays BOM-less");
        // zero annotations: a plain empty array
        assert_eq!(recent_json_body(&[]), "[]");
    }

    #[test]
    fn recent_out_note_formats_count_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//r.txt");
        let line = recent_out_note(5, file);
        assert_eq!(line, format!("[recent] записано 5 анотацій → {}", file.display()));
        assert!(line.starts_with("[recent] записано "));
        assert!(line.contains("5"));
        assert!(line.contains("анотацій"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
        // the house zero-result rule: the note still formats with n = 0
        assert_eq!(
            recent_out_note(0, file),
            format!("[recent] записано 0 анотацій → {}", file.display())
        );
    }

    #[test]
    fn recent_json_out_note_formats_json_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//r.json");
        let line = recent_json_out_note(file);
        assert_eq!(line, format!("[recent] записано JSON → {}", file.display()));
        assert!(line.starts_with("[recent] записано "));
        assert!(line.contains("JSON"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
    }

    #[test]
    fn favorites_lines_groups_by_nreg_and_sorts() {
        // The input is deliberately shuffled: the renderers reorder it
        // through favorites_sorted (the query base is nreg ASC, id ASC and
        // the sorts are stable, so the 1556-18 pair keeps its order).
        let rows = vec![
            FavRow {
                nreg: "2341-14".into(),
                kind: "highlight".into(),
                text: "Стаття 3 — Вища освіта".into(),
                created_at: "2026-10-27 10:00:00".into(),
                title: String::new(),
            },
            FavRow {
                nreg: "1556-18".into(),
                kind: "comment".into(),
                text: "  корисний\nзакон  ".into(),
                created_at: "2026-10-28 07:30:00".into(),
                title: "Про вищу освіту".into(),
            },
            FavRow {
                nreg: "1556-18".into(),
                kind: "bookmark".into(),
                text: "   ".into(),
                created_at: "2026-10-29 09:00:00".into(),
                title: "Про вищу освіту".into(),
            },
        ];
        // default: grouped by nreg ASC — one «{title||nreg}:» header per
        // document (unindexed 2341-14 renders its own nreg), rows indented
        // under it, an empty bookmark text renders no text line
        assert_eq!(
            favorites_lines(&rows, FavSort::Nreg),
            vec![
                "Обрані анотації (3):".to_string(),
                "Про вищу освіту:".to_string(),
                "  [comment] 2026-10-28 07:30:00".to_string(),
                "    корисний закон".to_string(),
                "  [bookmark] 2026-10-29 09:00:00".to_string(),
                "2341-14:".to_string(),
                "  [highlight] 2026-10-27 10:00:00".to_string(),
                "    Стаття 3 — Вища освіта".to_string(),
            ]
        );
        // title sort: flat like recent_lines, ordered by title||nreg
        // (digits sort before Ukrainian letters)
        assert_eq!(
            favorites_lines(&rows, FavSort::Title),
            vec![
                "Обрані анотації (3):".to_string(),
                "[highlight] 2341-14 · 2026-10-27 10:00:00".to_string(),
                "  Стаття 3 — Вища освіта".to_string(),
                "[comment] Про вищу освіту · 2026-10-28 07:30:00".to_string(),
                "  корисний закон".to_string(),
                "[bookmark] Про вищу освіту · 2026-10-29 09:00:00".to_string(),
            ]
        );
        // new sort: flat, created_at DESC (the recent direction through
        // this command's non-default path)
        assert_eq!(
            favorites_lines(&rows, FavSort::New),
            vec![
                "Обрані анотації (3):".to_string(),
                "[bookmark] Про вищу освіту · 2026-10-29 09:00:00".to_string(),
                "[comment] Про вищу освіту · 2026-10-28 07:30:00".to_string(),
                "  корисний закон".to_string(),
                "[highlight] 2341-14 · 2026-10-27 10:00:00".to_string(),
                "  Стаття 3 — Вища освіта".to_string(),
            ]
        );
    }

    #[test]
    fn favorites_lines_kind_filter_and_empty() {
        // --kind canonicalises through normalize_fav_kind (the GUI chip
        // mapping): case-insensitive English tokens plus the Ukrainian
        // aliases, singular and plural
        assert_eq!(normalize_fav_kind("Comment").unwrap(), "comment");
        assert_eq!(normalize_fav_kind(" highlight ").unwrap(), "highlight");
        assert_eq!(normalize_fav_kind("КОМЕНТАР").unwrap(), "comment");
        assert_eq!(normalize_fav_kind("виділення").unwrap(), "highlight");
        assert_eq!(normalize_fav_kind("закладки").unwrap(), "bookmark");
        // unknown kinds are a clear error (exit 1 through main)
        assert!(normalize_fav_kind("note").is_err());
        assert!(normalize_fav_kind("").is_err());
        let rows = vec![
            FavRow {
                nreg: "1556-18".into(),
                kind: "comment".into(),
                text: "  корисний\nзакон  ".into(),
                created_at: "2026-10-28 07:30:00".into(),
                title: "Про вищу освіту".into(),
            },
            FavRow {
                nreg: "2341-14".into(),
                kind: "highlight".into(),
                text: "Стаття 3".into(),
                created_at: "2026-10-27 10:00:00".into(),
                title: String::new(),
            },
        ];
        // cmd_favorites keeps rows whose kind matches the canonical token
        // (case-insensitive): «коментарі» keeps only the comment row
        let want = normalize_fav_kind("коментарі").unwrap();
        let kept: Vec<FavRow> = rows
            .iter()
            .filter(|r| r.kind.eq_ignore_ascii_case(want))
            .cloned()
            .collect();
        assert_eq!(
            favorites_lines(&kept, FavSort::Nreg),
            vec![
                "Обрані анотації (1):".to_string(),
                "Про вищу освіту:".to_string(),
                "  [comment] 2026-10-28 07:30:00".to_string(),
                "    корисний закон".to_string(),
            ]
        );
        // a kind with no rows (this data has no bookmarks): the filter
        // empties the set and the table collapses to the stdout-only
        // «немає» line (the --out file then carries the BOM alone)
        let want = normalize_fav_kind("закладка").unwrap();
        let kept: Vec<FavRow> = rows
            .iter()
            .filter(|r| r.kind.eq_ignore_ascii_case(want))
            .cloned()
            .collect();
        assert!(kept.is_empty());
        assert_eq!(
            favorites_lines(&kept, FavSort::Nreg),
            vec!["Обраних анотацій немає".to_string()]
        );
        assert_eq!(
            favorites_lines(&[], FavSort::Title),
            vec!["Обраних анотацій немає".to_string()]
        );
    }

    #[test]
    fn favorites_json_body_grouped_and_flat() {
        // the same three rows as the table test, shuffled base order
        let rows = vec![
            FavRow {
                nreg: "2341-14".into(),
                kind: "highlight".into(),
                text: "Стаття 3 — Вища освіта".into(),
                created_at: "2026-10-27 10:00:00".into(),
                title: String::new(),
            },
            FavRow {
                nreg: "1556-18".into(),
                kind: "comment".into(),
                text: "  корисний\nзакон  ".into(),
                created_at: "2026-10-28 07:30:00".into(),
                title: "Про вищу освіту".into(),
            },
            FavRow {
                nreg: "1556-18".into(),
                kind: "bookmark".into(),
                text: "   ".into(),
                created_at: "2026-10-29 09:00:00".into(),
                title: "Про вищу освіту".into(),
            },
        ];
        // grouped (the nreg default): one array per nreg + the total
        let body = favorites_json_body(&rows, FavSort::Nreg);
        assert!(!body.starts_with('\u{FEFF}'), "JSON body stays BOM-less");
        assert!(body.contains("\n  \"grouped\""), "pretty-printed body");
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["total"], 3);
        assert!(v.get("items").is_none(), "grouped shape has no items key");
        let groups = v["grouped"].as_object().unwrap();
        assert_eq!(groups.keys().collect::<Vec<_>>(), vec!["1556-18", "2341-14"]);
        let group = groups["1556-18"].as_array().unwrap();
        assert_eq!(group.len(), 2, "same-nreg rows stay together");
        assert_eq!(group[0]["kind"], "comment");
        assert_eq!(group[0]["created_at"], "2026-10-28 07:30:00");
        assert_eq!(group[0]["text"], "  корисний\nзакон  ", "raw text verbatim");
        assert_eq!(group[0]["title"], "Про вищу освіту");
        assert_eq!(group[1]["kind"], "bookmark");
        assert_eq!(groups["2341-14"].as_array().unwrap().len(), 1);
        // flat shapes: title sort renders items[] in title order + total
        let body = favorites_json_body(&rows, FavSort::Title);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(v.get("grouped").is_none(), "flat shape has no grouped key");
        assert_eq!(v["total"], 3);
        let items = v["items"].as_array().unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0]["nreg"], "2341-14", "title-ordered first");
        assert_eq!(items[2]["kind"], "bookmark");
        // new sort: also flat, created_at DESC
        let body = favorites_json_body(&rows, FavSort::New);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        let items = v["items"].as_array().unwrap();
        assert_eq!(items[0]["kind"], "bookmark", "newest first");
        assert_eq!(items[2]["kind"], "highlight");
        // empty: both shapes keep the total at 0 (the «немає» line is
        // stdout-only and never enters the JSON)
        assert_eq!(
            favorites_json_body(&[], FavSort::Nreg),
            "{\n  \"grouped\": {},\n  \"total\": 0\n}"
        );
        assert_eq!(
            favorites_json_body(&[], FavSort::New),
            "{\n  \"items\": [],\n  \"total\": 0\n}"
        );
    }

    #[test]
    fn favorites_out_note_formats_count_and_json() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//f.txt");
        let line = favorites_out_note(5, file);
        assert_eq!(line, format!("[favorites] записано 5 анотацій → {}", file.display()));
        assert!(line.starts_with("[favorites] записано "));
        assert!(line.contains("5"));
        assert!(line.contains("анотацій"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
        // the house zero-result rule: the note still formats with n = 0
        assert_eq!(
            favorites_out_note(0, file),
            format!("[favorites] записано 0 анотацій → {}", file.display())
        );
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//f.json");
        let line = favorites_json_out_note(file);
        assert_eq!(line, format!("[favorites] записано JSON → {}", file.display()));
        assert!(line.starts_with("[favorites] записано "));
        assert!(line.contains("JSON"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
    }

    #[test]
    fn hits_lines_numbers_rows_and_skips_empty_extras() {
        // header + numbered «{i}. {nazva} [{nreg}]» rows with an indented
        // muted extras line («Стан: … · №… · від … · ред. …», dates through
        // fmt_date); a row whose fields are empty renders NO extras line
        let rows = vec![
            rada_db::SearchHit {
                nreg: "2755-17".into(),
                nazva: "Податковий кодекс".into(),
                status: 5,
                org_date: "20101202".into(),
                org_num: "2755-VIII".into(),
                poddat: "20231102".into(),
            },
            rada_db::SearchHit {
                nreg: "1-1".into(),
                nazva: "Порожня картка".into(),
                status: 0,
                org_date: String::new(),
                org_num: "   ".into(),
                poddat: String::new(),
            },
        ];
        assert_eq!(
            hits_lines(&rows),
            vec![
                "Знайдено 2 документів:".to_string(),
                "1. Податковий кодекс [2755-17]".to_string(),
                "  Стан: Чинний · №2755-VIII · від 02.12.2010 · ред. 02.11.2023".to_string(),
                "2. Порожня картка [1-1]".to_string(),
            ]
        );
    }

    #[test]
    fn hits_md_lines_numbers_bold_links_and_extras() {
        // exact md shape: «# Знайдено {n} документів» header, blank line,
        // then «{i}. **{nazva}** — [`{nreg}`](portal link)» with the URL
        // built from the nreg, plus one nested bullet per non-empty extra
        let rows = vec![
            rada_db::SearchHit {
                nreg: "16-17".into(),
                nazva: "Про Конституцію".into(),
                status: 5,
                org_date: "19960628".into(),
                org_num: "14/1996-ВР".into(),
                poddat: String::new(),
            },
            rada_db::SearchHit {
                nreg: "9-9".into(),
                nazva: "Без extras".into(),
                status: 0,
                org_date: String::new(),
                org_num: String::new(),
                poddat: String::new(),
            },
        ];
        assert_eq!(
            hits_md_lines(&rows),
            vec![
                "# Знайдено 2 документів".to_string(),
                "".to_string(),
                "1. **Про Конституцію** — [`16-17`](https://zakon.rada.gov.ua/laws/show/16-17)"
                    .to_string(),
                "   - Стан: Чинний".to_string(),
                "   - №14/1996-ВР".to_string(),
                "   - від 28.06.1996".to_string(),
                "2. **Без extras** — [`9-9`](https://zakon.rada.gov.ua/laws/show/9-9)".to_string(),
            ]
        );
    }

    #[test]
    fn hits_out_note_formats_count_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//h.txt");
        let line = hits_out_note(3, file);
        assert_eq!(
            line,
            format!("[hits] записано 3 документів → {}", file.display())
        );
        assert!(line.starts_with("[hits] записано "));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
        // the house zero-result rule: the note still formats with n = 0
        assert_eq!(
            hits_out_note(0, file),
            format!("[hits] записано 0 документів → {}", file.display())
        );
    }

    #[test]
    fn normalize_hits_format_accepts_aliases_and_rejects() {
        // txt|md with the text/markdown aliases, case- and whitespace-tolerant
        assert_eq!(normalize_hits_format("txt").unwrap(), HitsFormat::Txt);
        assert_eq!(normalize_hits_format("TEXT").unwrap(), HitsFormat::Txt);
        assert_eq!(normalize_hits_format(" Text ").unwrap(), HitsFormat::Txt);
        assert_eq!(normalize_hits_format("md").unwrap(), HitsFormat::Md);
        assert_eq!(normalize_hits_format("Markdown").unwrap(), HitsFormat::Md);
        // invalid: the clear Ukrainian error listing the accepted values
        let err = normalize_hits_format("pdf").unwrap_err().to_string();
        assert_eq!(err, "формат pdf: використовуйте txt або md");
        assert!(normalize_hits_format("").is_err());
        assert!(normalize_hits_format("html").is_err());
    }

    #[test]
    fn dates_lines_renders_years_undated_and_verdict() {
        // header + one «{year}: {count}» line per year in input order
        // (the SQL sorts ascending), counts render plain like the
        // `stats`/`verify` numbers, the «без дати» line appears only when
        // undated > 0, and the verdict names the newest year (the
        // informational rule: the command still exits 0)
        let rows = vec![
            YearRow { year: "2024".into(), count: 120000 },
            YearRow { year: "2025".into(), count: 500 },
            YearRow { year: "2026".into(), count: 7 },
        ];
        assert_eq!(
            dates_lines(&rows, 120607, 3),
            vec![
                "Розподіл за роками (дата прийняття):".to_string(),
                "2024: 120000".to_string(),
                "2025: 500".to_string(),
                "2026: 7".to_string(),
                "без дати: 3".to_string(),
                "найновіший рік: 2026".to_string(),
            ]
        );
        // undated = 0: the «без дати» line is skipped entirely
        let lines = dates_lines(&rows, 120607, 0);
        assert!(!lines.iter().any(|l| l.starts_with("без дати")));
        assert_eq!(lines.last().unwrap(), "найновіший рік: 2026");
        // empty index: the header still renders (the `--out` file is never
        // BOM-alone) and the verdict flips to «індекс порожній»
        assert_eq!(
            dates_lines(&[], 0, 0),
            vec![
                "Розподіл за роками (дата прийняття):".to_string(),
                "індекс порожній".to_string(),
            ]
        );
    }

    #[test]
    fn dates_json_renders_years_total_and_null() {
        // stdout prints EXACTLY this body (+ newline); the `--out` file
        // receives the same bytes BOM-less (+ trailing newline). Map keys
        // sort, so «newest»/«total»/«undated»/«years» keep their
        // alphabetical order, as do «count»/«year» inside every entry.
        let rows = vec![
            YearRow { year: "2025".into(), count: 2 },
            YearRow { year: "2026".into(), count: 3 },
        ];
        let body = dates_json(&rows, 6, 1);
        assert_eq!(
            body,
            "{\n  \"newest\": \"2026\",\n  \"total\": 6,\n  \"undated\": 1,\n  \"years\": [\n    {\n      \"count\": 2,\n      \"year\": \"2025\"\n    },\n    {\n      \"count\": 3,\n      \"year\": \"2026\"\n    }\n  ]\n}"
        );
        assert!(!body.starts_with('\u{FEFF}'), "JSON body stays BOM-less");
        // empty histogram: a well-formed empty report, newest is null
        assert_eq!(
            dates_json(&[], 0, 0),
            "{\n  \"newest\": null,\n  \"total\": 0,\n  \"undated\": 0,\n  \"years\": []\n}"
        );
    }

    #[test]
    fn dates_out_note_formats_years_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//d.txt");
        let line = dates_out_note(37, file);
        assert_eq!(
            line,
            format!("[dates] записано 37 років → {}", file.display())
        );
        assert!(line.starts_with("[dates] "));
        assert!(line.contains("37"));
        assert!(line.contains("років"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
        // the informational zero rule: the note still formats with 0 years
        assert_eq!(
            dates_out_note(0, file),
            format!("[dates] записано 0 років → {}", file.display())
        );
    }

    #[test]
    fn normalize_month_year_rejects_garbage() {
        // accepted: the absent option (None → the caller resolves the
        // newest index year, the `dates` verdict rule) and exactly four
        // ASCII digits
        assert_eq!(normalize_month_year(None).unwrap(), None);
        assert_eq!(
            normalize_month_year(Some("2026")).unwrap(),
            Some("2026".to_string())
        );
        // rejected: anything that is not РРРР — the exact house error
        // text (exit 1 through `main`)
        for bad in ["26", "abcd", ""] {
            let err = normalize_month_year(Some(bad)).unwrap_err().to_string();
            assert_eq!(err, "рік має бути РРРР (наприклад 2026)");
        }
    }

    #[test]
    fn months_lines_renders_all_twelve_months_and_total() {
        // header + a STABLE 12-line grid: every month '01'..='12' gets
        // its «{year}-{month}: {count}» line even when the histogram has
        // no row for it (the renderer zero-fills the absent months),
        // then the «разом» verdict over the twelve rendered counts
        // (informational — the command still exits 0)
        let rows = vec![
            MonthRow { month: "03".into(), count: 4 },
            MonthRow { month: "11".into(), count: 2 },
        ];
        let mut expected = vec!["Документи 2026 за місяцями:".to_string()];
        for m in 1..=12 {
            let count = match m {
                3 => 4,
                11 => 2,
                _ => 0,
            };
            expected.push(format!("2026-{m:02}: {count}"));
        }
        expected.push("разом: 6".to_string());
        assert_eq!(months_lines("2026", &rows), expected);
        // all-zero year: the header + 12 zero rows + «разом: 0» still
        // render (the informational rule, the `dates` empty idiom)
        let lines = months_lines("1701", &[]);
        assert_eq!(lines.len(), 14);
        assert_eq!(lines[0], "Документи 1701 за місяцями:");
        assert_eq!(lines[1], "1701-01: 0");
        assert_eq!(lines[12], "1701-12: 0");
        assert_eq!(lines[13], "разом: 0");
    }

    #[test]
    fn months_json_renders_months_and_total() {
        // stdout prints EXACTLY this body (+ newline); the `--out` file
        // receives the same bytes BOM-less (+ trailing newline). Map keys
        // sort, so «months»/«total»/«year» keep their alphabetical order,
        // as do «count»/«month» inside every entry; the twelve entries
        // are the zero-filled '01'..='12' grid (the `months_lines`
        // stable-grid decision) and «total» sums the twelve counts
        let rows = vec![MonthRow { month: "07".into(), count: 7 }];
        let body = months_json("2026", &rows);
        let entries: Vec<String> = (1..=12)
            .map(|m| {
                let count = if m == 7 { 7 } else { 0 };
                format!("    {{\n      \"count\": {count},\n      \"month\": \"{m:02}\"\n    }}")
            })
            .collect();
        let expected = format!(
            "{{\n  \"months\": [\n{}\n  ],\n  \"total\": 7,\n  \"year\": \"2026\"\n}}",
            entries.join(",\n")
        );
        assert_eq!(body, expected);
        assert!(!body.starts_with('\u{FEFF}'), "JSON body stays BOM-less");
        // all-zero year: a well-formed 12-entry zero report
        let body = months_json("1701", &[]);
        assert!(body.contains("\"total\": 0"));
        assert!(body.contains("\"year\": \"1701\""));
        assert_eq!(body.matches("\"month\"").count(), 12);
    }

    #[test]
    fn months_out_note_formats_year_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//m.txt");
        let line = months_out_note("2026", file);
        assert_eq!(
            line,
            format!("[months] записано 2026 рік → {}", file.display())
        );
        assert!(line.starts_with("[months] "));
        assert!(line.contains("2026"));
        assert!(line.contains("рік"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
        // the informational zero rule: the note still formats for an
        // all-zero year
        assert_eq!(
            months_out_note("1701", file),
            format!("[months] записано 1701 рік → {}", file.display())
        );
    }

    #[test]
    fn normalize_year_arg_rejects_garbage() {
        // accepted: the absent option (None → the caller resolves the
        // oldest / newest index year, the `dates` MIN/MAX rule, the `0`
        // sentinel) and exactly four ASCII digits
        assert_eq!(normalize_year_arg(None).unwrap(), 0);
        assert_eq!(normalize_year_arg(Some("2026")).unwrap(), 2026);
        // rejected: anything that is not РРРР — the exact house error
        // text (exit 1 through `main`)
        for bad in ["26", "abcd", ""] {
            let err = normalize_year_arg(Some(bad)).unwrap_err().to_string();
            assert_eq!(err, "рік має бути РРРР (наприклад 2026)");
        }
        // the closed-range rule: from <= to accepted, a `--from` later
        // than the `--to` rejected with the exact house error text
        assert!(check_year_range(2020, 2026).is_ok());
        assert!(check_year_range(2026, 2026).is_ok());
        let err = check_year_range(2026, 2020).unwrap_err().to_string();
        assert_eq!(err, "початок періоду пізніший за кінець");
    }

    #[test]
    fn years_lines_renders_range_rows_and_omits_zero_years() {
        // header + one «{yyyy}: {count}» line per in-range year ascending
        // (the SQL order, counts render plain), then the «разом» verdict
        // over the SHOWN rows, then the «без дати» line only when the
        // whole-index undated count is positive (the `dates` rule).
        // Unlike `months` the grid is NOT zero-filled: 2025 — inside the
        // range, absent from the histogram — simply has no row, and the
        // out-of-range 2019 and the unparseable «abcd» rows drop out
        // (informational — the command still exits 0)
        let rows = vec![
            YearRow { year: "2019".into(), count: 5 },
            YearRow { year: "2024".into(), count: 7283 },
            YearRow { year: "2026".into(), count: 5492 },
            YearRow { year: "abcd".into(), count: 1 },
        ];
        assert_eq!(
            years_lines(2020, 2026, &rows, 14669),
            vec![
                "Документи за роками:".to_string(),
                "2024: 7283".to_string(),
                "2026: 5492".to_string(),
                "разом: 12775".to_string(),
                "без дати: 14669".to_string(),
            ]
        );
        // empty range: header + «немає документів за цей період» +
        // «разом: 0» + the undated line (the `recent` zero rule, exit 0)
        assert_eq!(
            years_lines(1900, 1950, &rows, 3),
            vec![
                "Документи за роками:".to_string(),
                "немає документів за цей період".to_string(),
                "разом: 0".to_string(),
                "без дати: 3".to_string(),
            ]
        );
        // the `dates` rule: a fully dated index (undated == 0) prints no
        // «без дати» line at all
        assert_eq!(
            years_lines(2026, 2026, &rows, 0),
            vec![
                "Документи за роками:".to_string(),
                "2026: 5492".to_string(),
                "разом: 5492".to_string(),
            ]
        );
    }

    #[test]
    fn years_json_renders_rows_and_count_total() {
        // stdout prints EXACTLY this body (+ newline); the `--out` file
        // receives the same bytes BOM-less (+ trailing newline). Map keys
        // sort, so «count_total»/«from»/«rows»/«to»/«undated» keep their
        // alphabetical order, as do «count»/«year» inside every entry;
        // «rows» carries only the in-range rows (the `years_lines`
        // omit-zero-years rule) and «count_total» sums the SHOWN rows
        let rows = vec![
            YearRow { year: "2019".into(), count: 5 },
            YearRow { year: "2024".into(), count: 4 },
            YearRow { year: "2026".into(), count: 2 },
        ];
        let body = years_json(2020, 2026, &rows, 9);
        let entries: Vec<String> = [("2024", 4), ("2026", 2)]
            .iter()
            .map(|(year, count)| {
                format!("    {{\n      \"count\": {count},\n      \"year\": \"{year}\"\n    }}")
            })
            .collect();
        let expected = format!(
            "{{\n  \"count_total\": 6,\n  \"from\": \"2020\",\n  \"rows\": [\n{}\n  ],\n  \"to\": \"2026\",\n  \"undated\": 9\n}}",
            entries.join(",\n")
        );
        assert_eq!(body, expected);
        assert!(!body.starts_with('\u{FEFF}'), "JSON body stays BOM-less");
        // empty range: a well-formed zero report with no rows
        let body = years_json(1900, 1950, &rows, 3);
        assert!(body.contains("\"count_total\": 0"));
        assert!(body.contains("\"rows\": []"));
        assert!(body.contains("\"from\": \"1900\""));
        assert!(body.contains("\"to\": \"1950\""));
    }

    #[test]
    fn years_out_note_formats_period_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//y.txt");
        let line = years_out_note(2020, 2026, file);
        assert_eq!(
            line,
            format!("[years] записано період 2020–2026 → {}", file.display())
        );
        assert!(line.starts_with("[years] "));
        assert!(line.contains("2020–2026"));
        assert!(line.contains("період"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
        // the informational zero rule: the note still formats for the
        // all-zero `0000–0000` range
        assert_eq!(
            years_out_note(0, 0, file),
            format!("[years] записано період 0000–0000 → {}", file.display())
        );
    }

    #[test]
    fn orgs_lines_renders_ranked_rows_and_total() {
        // header + one «{label} [{org_num}]: {count}» line per publisher in
        // input order (the SQL sorts by count descending — the ranking
        // lives in the order, no rank prefix, mirroring the
        // `dates`/`dict-stats` body), an unresolved org_num (the empty /
        // 0 / dict-miss case, already mapped by the caller) renders the
        // «— не вказано» placeholder, then the «всього документів» line
        // (informational — the command still exits 0)
        let rows = vec![
            OrgRow {
                org_num: "2".into(),
                label: "Кабінет Міністрів України".into(),
                count: 89343,
            },
            OrgRow {
                org_num: "4".into(),
                label: "Президент України".into(),
                count: 48780,
            },
            OrgRow {
                org_num: "0".into(),
                label: "— не вказано".into(),
                count: 32857,
            },
        ];
        assert_eq!(
            orgs_lines(&rows, 295340, None),
            vec![
                "Найкращі видавці (за документами):".to_string(),
                "Кабінет Міністрів України [2]: 89343".to_string(),
                "Президент України [4]: 48780".to_string(),
                "— не вказано [0]: 32857".to_string(),
                "всього документів: 295340".to_string(),
            ]
        );
        // empty index: the header and the zero total still render (the
        // `--out` file is never BOM-alone)
        assert_eq!(
            orgs_lines(&[], 0, None),
            vec![
                "Найкращі видавці (за документами):".to_string(),
                "всього документів: 0".to_string(),
            ]
        );
    }

    #[test]
    fn orgs_json_renders_orgs_and_total() {
        // stdout prints EXACTLY this body (+ newline); the `--out` file
        // receives the same bytes BOM-less (+ trailing newline). Map keys
        // sort, so «orgs» precedes «total» and «count»/«label»/«org_num»
        // keep their alphabetical order inside every entry.
        let rows = vec![
            OrgRow {
                org_num: "2".into(),
                label: "Кабінет Міністрів України".into(),
                count: 2,
            },
            OrgRow {
                org_num: "".into(),
                label: "— не вказано".into(),
                count: 3,
            },
        ];
        let body = orgs_json(&rows, 5, None);
        assert_eq!(
            body,
            "{\n  \"orgs\": [\n    {\n      \"count\": 2,\n      \"label\": \"Кабінет Міністрів України\",\n      \"org_num\": \"2\"\n    },\n    {\n      \"count\": 3,\n      \"label\": \"— не вказано\",\n      \"org_num\": \"\"\n    }\n  ],\n  \"total\": 5\n}"
        );
        assert!(!body.starts_with('\u{FEFF}'), "JSON body stays BOM-less");
        // empty histogram: a well-formed empty report with total 0
        assert_eq!(orgs_json(&[], 0, None), "{\n  \"orgs\": [],\n  \"total\": 0\n}");
    }

    #[test]
    fn orgs_out_note_formats_count_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//o.txt");
        let line = orgs_out_note(20, file);
        assert_eq!(
            line,
            format!("[orgs] записано 20 видавців → {}", file.display())
        );
        assert!(line.starts_with("[orgs] "));
        assert!(line.contains("20"));
        assert!(line.contains("видавців"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
        // the informational zero rule: the note still formats with 0 rows
        assert_eq!(
            orgs_out_note(0, file),
            format!("[orgs] записано 0 видавців → {}", file.display())
        );
    }

    #[test]
    fn orgs_lines_year_filter_renders_year_header_and_rows() {
        // `--year yyyy`: the header becomes «Видавці документів {yyyy}
        // року:», the rows and the «всього документів» verdict keep
        // today's rendering (the total is the filtered document count),
        // and the unfiltered header stays exactly as today
        let rows = vec![
            OrgRow {
                org_num: "2".into(),
                label: "Кабінет Міністрів України".into(),
                count: 7000,
            },
            OrgRow {
                org_num: "".into(),
                label: "— не вказано".into(),
                count: 283,
            },
        ];
        assert_eq!(
            orgs_lines(&rows, 7283, Some("2024")),
            vec![
                "Видавці документів 2024 року:".to_string(),
                "Кабінет Міністрів України [2]: 7000".to_string(),
                "— не вказано []: 283".to_string(),
                "всього документів: 7283".to_string(),
            ]
        );
        // zero filtered rows: the year header and the zero verdict still
        // render (the `--out` file is never BOM-alone)
        assert_eq!(
            orgs_lines(&[], 0, Some("2024")),
            vec![
                "Видавці документів 2024 року:".to_string(),
                "всього документів: 0".to_string(),
            ]
        );
        // unfiltered: today's header verbatim
        assert_eq!(
            orgs_lines(&[], 0, None),
            vec![
                "Найкращі видавці (за документами):".to_string(),
                "всього документів: 0".to_string(),
            ]
        );
    }

    #[test]
    fn orgs_json_year_filter_adds_year_key_alphabetically() {
        // with `--year` the body gains "year": "yyyy"; map keys sort, so
        // «orgs» < «total» < «year» — the year renders last, after total
        let rows = vec![OrgRow {
            org_num: "2".into(),
            label: "Кабінет Міністрів України".into(),
            count: 5,
        }];
        let body = orgs_json(&rows, 5, Some("2024"));
        assert_eq!(
            body,
            "{\n  \"orgs\": [\n    {\n      \"count\": 5,\n      \"label\": \"Кабінет Міністрів України\",\n      \"org_num\": \"2\"\n    }\n  ],\n  \"total\": 5,\n  \"year\": \"2024\"\n}"
        );
        assert!(!body.starts_with('\u{FEFF}'), "JSON body stays BOM-less");
        // absent year: exactly today's shape, no "year" key at all
        assert_eq!(
            orgs_json(&rows, 5, None),
            "{\n  \"orgs\": [\n    {\n      \"count\": 5,\n      \"label\": \"Кабінет Міністрів України\",\n      \"org_num\": \"2\"\n    }\n  ],\n  \"total\": 5\n}"
        );
    }

    #[test]
    fn orgs_year_arg_accepts_four_digits_and_rejects_the_rest() {
        // `orgs --year` reuses the `years`/`months` validator: absent is
        // the whole-index report, exactly four ASCII digits pass,
        // everything else — two digits, letters, the empty string —
        // fails with the house message (exit 1 through `main`)
        assert_eq!(normalize_year_arg(None).unwrap(), 0);
        assert_eq!(normalize_year_arg(Some("2024")).unwrap(), 2024);
        for bad in ["26", "abcd", ""] {
            let err = normalize_year_arg(Some(bad)).unwrap_err().to_string();
            assert_eq!(err, "рік має бути РРРР (наприклад 2026)");
        }
    }

    #[test]
    fn docs_list_lines_renders_header_rows_and_verdict() {
        // header «Документи видавця {org}:» + one
        // «{dd.mm.yyyy}  {nreg}  {nazva}» line per document in input
        // order (the SQL orders by org_date DESC, nreg ASC — the
        // `recent` docs row idiom) + the «разом» verdict over the
        // RENDERED rows (post-limit, the `recent` rule)
        let rows = vec![
            RecentDocRow {
                nreg: "123/26".into(),
                title: "Про щоось".into(),
                org_date: "20261029".into(),
            },
            RecentDocRow {
                nreg: "9/26".into(),
                title: "Інше".into(),
                org_date: "20261027".into(),
            },
        ];
        // resolved dictionary label: the header names the label
        assert_eq!(
            docs_list_lines(Some("Кабінет Міністрів України"), None, &rows),
            vec![
                "Документи видавця Кабінет Міністрів України:".to_string(),
                "29.10.2026  123/26  Про щоось".to_string(),
                "27.10.2026  9/26  Інше".to_string(),
                "разом: 2".to_string(),
            ]
        );
        // dict miss / non-numeric key: the header names the raw key
        assert_eq!(
            docs_list_lines(Some("2"), None, &rows[..1]),
            vec![
                "Документи видавця 2:".to_string(),
                "29.10.2026  123/26  Про щоось".to_string(),
                "разом: 1".to_string(),
            ]
        );
        // unknown publisher / empty year window: header + the «немає»
        // line + «разом: 0» still render (the informational empty rule,
        // the `--out` file is never BOM-alone)
        assert_eq!(
            docs_list_lines(Some("999999"), None, &[]),
            vec![
                "Документи видавця 999999:".to_string(),
                "немає документів за цей період".to_string(),
                "разом: 0".to_string(),
            ]
        );
    }

    #[test]
    fn docs_list_json_renders_alphabetical_keys_and_optional_year() {
        // stdout prints EXACTLY this body (+ newline); the `--out` file
        // receives the same bytes BOM-less (+ trailing newline). Map keys
        // sort, so «docs» < «org» < «org_label» < «total» (< «year») and
        // «date»/«nreg»/«title» keep their alphabetical order inside
        // every entry; `date` is the raw 8-digit org_date and `total`
        // counts the listed rows (the «разом» rule)
        let rows = vec![RecentDocRow {
            nreg: "123/24".into(),
            title: "Про бюджет".into(),
            org_date: "20240517".into(),
        }];
        let body = docs_list_json(Some("2"), Some("Кабінет Міністрів України"), None, &rows);
        assert_eq!(
            body,
            "{\n  \"docs\": [\n    {\n      \"date\": \"20240517\",\n      \"nreg\": \"123/24\",\n      \"title\": \"Про бюджет\"\n    }\n  ],\n  \"org\": \"2\",\n  \"org_label\": \"Кабінет Міністрів України\",\n  \"total\": 1\n}"
        );
        assert!(!body.starts_with('\u{FEFF}'), "JSON body stays BOM-less");
        // with `--year` the body gains "year": "yyyy"; map keys sort, so
        // «year» lands last, after «total» (the `orgs` rule)
        let with_year =
            docs_list_json(Some("2"), Some("Кабінет Міністрів України"), Some("2024"), &rows);
        assert!(
            with_year.ends_with(",\n  \"year\": \"2024\"\n}"),
            "the year key renders last"
        );
        let v: serde_json::Value = serde_json::from_str(&with_year).unwrap();
        assert_eq!(v["year"], "2024");
        assert_eq!(v["total"], 1);
        assert_eq!(v["docs"][0]["date"], "20240517");
        // absent year: exactly today's shape, no "year" key at all
        assert!(!body.contains("\"year\""), "absent year omits the key");
        // unknown publisher: a well-formed empty body with total 0
        assert_eq!(
            docs_list_json(Some("999999"), Some("999999"), None, &[]),
            "{\n  \"docs\": [],\n  \"org\": \"999999\",\n  \"org_label\": \"999999\",\n  \"total\": 0\n}"
        );
    }

    #[test]
    fn docs_list_out_note_formats_count_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//d.txt");
        let line = docs_list_out_note(50, file);
        assert_eq!(
            line,
            format!("[docs] записано 50 документів → {}", file.display())
        );
        assert!(line.starts_with("[docs] "));
        assert!(line.contains("50"));
        assert!(line.contains("документів"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
        // the informational zero rule: the note still formats with 0 rows
        assert_eq!(
            docs_list_out_note(0, file),
            format!("[docs] записано 0 документів → {}", file.display())
        );
    }

    #[test]
    fn docs_list_year_arg_and_limit_validation() {
        // `docs --year` reuses the `years`/`months`/`orgs` validator and
        // `--limit` the house clamp: absent is the whole-archive report,
        // exactly four ASCII digits pass, everything else — two digits,
        // letters, the empty string — fails with the house message
        // (exit 1 through `main`); the limit clamps to 1..=500 (never a
        // hard error, the `hits`/`recent` budget idiom)
        assert_eq!(normalize_year_arg(None).unwrap(), 0);
        assert_eq!(normalize_year_arg(Some("2024")).unwrap(), 2024);
        for bad in ["26", "abcd", ""] {
            let err = normalize_year_arg(Some(bad)).unwrap_err().to_string();
            assert_eq!(err, "рік має бути РРРР (наприклад 2026)");
        }
        assert_eq!(docs_limit(50), 50);
        assert_eq!(docs_limit(1), 1);
        assert_eq!(docs_limit(500), 500);
        assert_eq!(docs_limit(0), 1);
        assert_eq!(docs_limit(501), 500);
        assert_eq!(docs_limit(usize::MAX), 500);
    }

    #[test]
    fn docs_list_lines_year_only_renders_year_header_rows_and_verdict() {
        // `--year` without `--org`: the header names just the year
        // («Документи {yyyy} року:», the `orgs --year` header family),
        // the rows keep the `recent` docs idiom and the «разом» verdict
        // counts the RENDERED rows (post-limit, the `recent` rule)
        let rows = vec![
            RecentDocRow {
                nreg: "123/24".into(),
                title: "Про бюджет".into(),
                org_date: "20241230".into(),
            },
            RecentDocRow {
                nreg: "9/24".into(),
                title: "Інше".into(),
                org_date: "20240102".into(),
            },
        ];
        assert_eq!(
            docs_list_lines(None, Some("2024"), &rows),
            vec![
                "Документи 2024 року:".to_string(),
                "30.12.2024  123/24  Про бюджет".to_string(),
                "02.01.2024  9/24  Інше".to_string(),
                "разом: 2".to_string(),
            ]
        );
        // empty year window: header + the «немає» line + «разом: 0»
        // still render (the informational empty rule, the `--out` file
        // is never BOM-alone)
        assert_eq!(
            docs_list_lines(None, Some("2024"), &[]),
            vec![
                "Документи 2024 року:".to_string(),
                "немає документів за цей період".to_string(),
                "разом: 0".to_string(),
            ]
        );
    }

    #[test]
    fn docs_list_lines_and_json_combine_org_and_year_with_alphabetical_keys() {
        // org + year: the header joins both filters —
        // «Документи видавця {label}: {yyyy} року:» (caller passes the
        // resolved label; the raw key flows through the same slot when
        // the dictionary had nothing)
        let rows = vec![RecentDocRow {
            nreg: "123/24".into(),
            title: "Про бюджет".into(),
            org_date: "20240517".into(),
        }];
        assert_eq!(
            docs_list_lines(Some("Кабінет Міністрів України"), Some("2024"), &rows),
            vec![
                "Документи видавця Кабінет Міністрів України: 2024 року:".to_string(),
                "17.05.2024  123/24  Про бюджет".to_string(),
                "разом: 1".to_string(),
            ]
        );
        assert_eq!(
            docs_list_lines(Some("1"), Some("2024"), &rows)[0],
            "Документи видавця 1: 2024 року:"
        );
        // combined JSON: the `org`/`org_label` keys stay and map keys
        // sort, so «docs» < «org» < «org_label» < «total» < «year»
        let combined =
            docs_list_json(Some("2"), Some("Кабінет Міністрів України"), Some("2024"), &rows);
        assert_eq!(
            combined,
            "{\n  \"docs\": [\n    {\n      \"date\": \"20240517\",\n      \"nreg\": \"123/24\",\n      \"title\": \"Про бюджет\"\n    }\n  ],\n  \"org\": \"2\",\n  \"org_label\": \"Кабінет Міністрів України\",\n  \"total\": 1,\n  \"year\": \"2024\"\n}"
        );
        // year-only JSON: the year describes no publisher, so the
        // `org`/`org_label` keys are omitted (documented choice; keys
        // stay alphabetical either way): «docs» < «total» < «year»
        let body = docs_list_json(None, None, Some("2024"), &rows);
        assert_eq!(
            body,
            "{\n  \"docs\": [\n    {\n      \"date\": \"20240517\",\n      \"nreg\": \"123/24\",\n      \"title\": \"Про бюджет\"\n    }\n  ],\n  \"total\": 1,\n  \"year\": \"2024\"\n}"
        );
        assert!(!body.contains("\"org\""), "no publisher keys without --org");
        assert!(!body.contains("\"org_label\""), "no publisher keys without --org");
        assert!(!body.starts_with('\u{FEFF}'), "JSON body stays BOM-less");
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["year"], "2024");
        assert_eq!(v["total"], 1);
        assert_eq!(v["docs"][0]["date"], "20240517");
    }

    #[test]
    fn docs_requires_a_filter_and_year_still_validates() {
        // neither `--org` nor `--year` is the only new hard error:
        // «вкажіть --org або --year» (exit 1 through `main`, checked
        // after the missing-index error, before any query); each filter
        // alone — and both together — pass. The empty string still
        // counts as a given `--org` (an exact key match, not an absent
        // filter). `--year` keeps flowing through the house
        // `normalize_year_arg` validator untouched
        let err = check_docs_filters(None, None).unwrap_err().to_string();
        assert_eq!(err, "вкажіть --org або --year");
        assert!(check_docs_filters(Some("2"), None).is_ok());
        assert!(check_docs_filters(None, Some("2024")).is_ok());
        assert!(check_docs_filters(Some("2"), Some("2024")).is_ok());
        assert!(check_docs_filters(Some(""), None).is_ok());
        assert_eq!(normalize_year_arg(None).unwrap(), 0);
        assert_eq!(normalize_year_arg(Some("2024")).unwrap(), 2024);
        for bad in ["26", "abcd", ""] {
            let err = normalize_year_arg(Some(bad)).unwrap_err().to_string();
            assert_eq!(err, "рік має бути РРРР (наприклад 2026)");
        }
    }

    #[test]
    fn annots_doc_lines_renders_header_rows_verdict_and_empty() {
        // header «Анотації документа {nreg}:» + one `recent`-style
        // two-line block per row through the shared `recent_row_lines`
        // renderer («[{kind}] {title||nreg} · {created_at}» + the
        // indented 60-char folded text; a bookmark with empty text
        // renders no text line) + «разом: N» over the RENDERED rows
        // (post-`--limit`, the `recent` rule)
        let rows = vec![
            RecentRow {
                nreg: "1556-18".into(),
                kind: "highlight".into(),
                created_at: "2026-10-29 09:00:00".into(),
                text: "Стаття 3 — Вища освіта".into(),
                title: "Про вищу освіту".into(),
            },
            RecentRow {
                nreg: "1556-18".into(),
                kind: "bookmark".into(),
                created_at: "2026-10-28 07:30:00".into(),
                text: "  ".into(),
                title: "Про вищу освіту".into(),
            },
        ];
        assert_eq!(
            annots_doc_lines("1556-18", &rows),
            vec![
                "Анотації документа 1556-18:".to_string(),
                "[highlight] Про вищу освіту · 2026-10-29 09:00:00".to_string(),
                "  Стаття 3 — Вища освіта".to_string(),
                "[bookmark] Про вищу освіту · 2026-10-28 07:30:00".to_string(),
                "разом: 2".to_string(),
            ]
        );
        // an unindexed nreg (empty title): the row renders its own nreg
        // (the shared `title||nreg` rule)
        let anon = RecentRow {
            nreg: "9-9".into(),
            kind: "comment".into(),
            created_at: "2026-10-30 10:00:00".into(),
            text: "нотатка".into(),
            title: String::new(),
        };
        assert_eq!(
            annots_doc_lines("9-9", &[anon]),
            vec![
                "Анотації документа 9-9:".to_string(),
                "[comment] 9-9 · 2026-10-30 10:00:00".to_string(),
                "  нотатка".to_string(),
                "разом: 1".to_string(),
            ]
        );
        // doc without annotations / unknown nreg: the informational
        // empty triple (header + «немає анотацій» + «разом: 0», exit 0),
        // the `docs` zero-rows rule — the header always renders
        assert_eq!(
            annots_doc_lines("999999", &[]),
            vec![
                "Анотації документа 999999:".to_string(),
                "немає анотацій".to_string(),
                "разом: 0".to_string(),
            ]
        );
    }

    #[test]
    fn annots_doc_json_renders_alphabetical_keys_and_total() {
        // stdout prints EXACTLY this body (+ newline); the `--out` file
        // receives the same bytes BOM-less (+ trailing newline). Map keys
        // sort, so «annots» < «doc» < «total» and the shared
        // `recent-annots` row keys «created_at»/«kind»/«nreg»/«text»/
        // «title» keep their alphabetical order inside every entry;
        // `total` counts the listed rows (the «разом» rule, post-limit)
        let rows = vec![RecentRow {
            nreg: "1556-18".into(),
            kind: "comment".into(),
            created_at: "2026-10-29 09:00:00".into(),
            text: "корисний".into(),
            title: "Про вищу освіту".into(),
        }];
        let body = annots_doc_json("1556-18", &rows);
        assert_eq!(
            body,
            "{\n  \"annots\": [\n    {\n      \"created_at\": \"2026-10-29 09:00:00\",\n      \"kind\": \"comment\",\n      \"nreg\": \"1556-18\",\n      \"text\": \"корисний\",\n      \"title\": \"Про вищу освіту\"\n    }\n  ],\n  \"doc\": \"1556-18\",\n  \"total\": 1\n}"
        );
        assert!(!body.starts_with('\u{FEFF}'), "JSON body stays BOM-less");
        // doc without annotations / unknown nreg: a well-formed empty
        // body with total 0
        assert_eq!(
            annots_doc_json("999999", &[]),
            "{\n  \"annots\": [],\n  \"doc\": \"999999\",\n  \"total\": 0\n}"
        );
    }

    #[test]
    fn annots_doc_out_note_formats_count_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//a.txt");
        let line = annots_doc_out_note(30, file);
        assert_eq!(
            line,
            format!("[annots] записано 30 анотацій → {}", file.display())
        );
        assert!(line.starts_with("[annots] "));
        assert!(line.contains("30"));
        assert!(line.contains("анотацій"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
        // the informational zero rule: the note still formats with 0 rows
        assert_eq!(
            annots_doc_out_note(0, file),
            format!("[annots] записано 0 анотацій → {}", file.display())
        );
    }

    #[test]
    fn annots_kind_and_limit_validation() {
        // `annots --kind` folds through the shared `favorites --kind`
        // mapping: the three Ukrainian labels (plus their plural and the
        // English canonical forms, like the GUI chips) are accepted
        // case-insensitively and trimmed; garbage fails with the house
        // validation message (exit 1 through `main`); `--limit` clamps
        // to 1..=500 (never a hard error, the `docs_limit` budget idiom)
        assert_eq!(normalize_annots_kind("коментар").unwrap(), "comment");
        assert_eq!(normalize_annots_kind("ВИДІЛЕННЯ").unwrap(), "highlight");
        assert_eq!(normalize_annots_kind(" Закладка ").unwrap(), "bookmark");
        assert_eq!(normalize_annots_kind("коментарі").unwrap(), "comment");
        assert_eq!(normalize_annots_kind("закладки").unwrap(), "bookmark");
        assert_eq!(normalize_annots_kind("Comment").unwrap(), "comment");
        let err = normalize_annots_kind("garbage").unwrap_err().to_string();
        assert_eq!(
            err,
            "невідомий тип анотації garbage (comment|highlight|bookmark)"
        );
        assert_eq!(annots_limit(100), 100);
        assert_eq!(annots_limit(1), 1);
        assert_eq!(annots_limit(500), 500);
        assert_eq!(annots_limit(0), 1);
        assert_eq!(annots_limit(501), 500);
        assert_eq!(annots_limit(usize::MAX), 500);
    }

    #[test]
    fn recent_docs_lines_renders_header_rows_and_verdict() {
        // header + one «{dd.mm.yyyy}  {nreg}  {title}» line per document
        // in input order (the SQL orders by org_date DESC, nreg ASC) +
        // the «разом» verdict over the RENDERED rows (post-limit,
        // informational — the command still exits 0)
        let rows = vec![
            RecentDocRow {
                nreg: "123/26".into(),
                title: "Про щоось".into(),
                org_date: "20261029".into(),
            },
            RecentDocRow {
                nreg: "9/26".into(),
                title: "Інше".into(),
                org_date: "20261027".into(),
            },
        ];
        assert_eq!(
            recent_docs_lines(Some(("20261023", "20261029")), &rows),
            vec![
                "Свіжі документи (23.10.2026–29.10.2026):".to_string(),
                "29.10.2026  123/26  Про щоось".to_string(),
                "27.10.2026  9/26  Інше".to_string(),
                "разом: 2".to_string(),
            ]
        );
        // zero qualifying documents: header + the «немає» line +
        // «разом: 0» still render (the informational empty rule, the
        // `--out` file is never BOM-alone)
        assert_eq!(
            recent_docs_lines(Some(("20261023", "20261029")), &[]),
            vec![
                "Свіжі документи (23.10.2026–29.10.2026):".to_string(),
                "немає документів за цей період".to_string(),
                "разом: 0".to_string(),
            ]
        );
        // an index without a single valid 8-digit date: the header
        // drops the range (nothing to anchor the window on)
        assert_eq!(
            recent_docs_lines(None, &[]),
            vec![
                "Свіжі документи:".to_string(),
                "немає документів за цей період".to_string(),
                "разом: 0".to_string(),
            ]
        );
    }

    #[test]
    fn recent_docs_json_renders_pretty_body_with_alphabetical_keys() {
        // stdout prints EXACTLY this body (+ newline); the `--out` file
        // receives the same bytes BOM-less (+ trailing newline). Map keys
        // sort, so «days»/«docs»/«from»/«to» and «date»/«nreg»/«title»
        // keep their alphabetical order inside every entry; «date» is the
        // dd.mm.yyyy rendering, «from»/«to» the raw YYYYMMDD bounds.
        let rows = vec![RecentDocRow {
            nreg: "123/26".into(),
            title: "Про щоось".into(),
            org_date: "20261029".into(),
        }];
        let body = recent_docs_json(7, Some(("20261023", "20261029")), &rows);
        assert_eq!(
            body,
            "{\n  \"days\": 7,\n  \"docs\": [\n    {\n      \"date\": \"29.10.2026\",\n      \"nreg\": \"123/26\",\n      \"title\": \"Про щоось\"\n    }\n  ],\n  \"from\": \"20261023\",\n  \"to\": \"20261029\"\n}"
        );
        assert!(!body.starts_with('\u{FEFF}'), "JSON body stays BOM-less");
        // no valid dated row to anchor on: the zero report with both
        // bounds null (the `dates` "newest": null idiom)
        assert_eq!(
            recent_docs_json(7, None, &[]),
            "{\n  \"days\": 7,\n  \"docs\": [],\n  \"from\": null,\n  \"to\": null\n}"
        );
    }

    #[test]
    fn recent_docs_out_note_formats_count_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//r.txt");
        let line = recent_docs_out_note(7, file);
        assert_eq!(
            line,
            format!("[recent] записано 7 документів → {}", file.display())
        );
        assert!(line.starts_with("[recent] "));
        assert!(line.contains("7"));
        assert!(line.contains("документів"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
        // the informational zero rule: the note still formats with 0 rows
        assert_eq!(
            recent_docs_out_note(0, file),
            format!("[recent] записано 0 документів → {}", file.display())
        );
    }

    #[test]
    fn recent_cutoff_validates_days_and_computes_inclusive_window() {
        // --days accepts 1 / 7 / 365 (whitespace trimmed, absent keeps
        // the 7 default); 0 / 366 / «abcd» / empty are hard errors with
        // the house Ukrainian wording (exit 1 through `main`)
        assert_eq!(normalize_recent_days(None).unwrap(), 7);
        assert_eq!(normalize_recent_days(Some("1")).unwrap(), 1);
        assert_eq!(normalize_recent_days(Some(" 7 ")).unwrap(), 7);
        assert_eq!(normalize_recent_days(Some("365")).unwrap(), 365);
        for bad in ["0", "366", "abcd", ""] {
            let err = normalize_recent_days(Some(bad)).unwrap_err();
            assert_eq!(err.to_string(), "днів має бути 1–365");
        }
        // --limit mirrors the bound rule: 100 default, 1..=500 accepted,
        // 0 / 501 / «abcd» rejected
        assert_eq!(normalize_recent_limit(None).unwrap(), 100);
        assert_eq!(normalize_recent_limit(Some("500")).unwrap(), 500);
        for bad in ["0", "501", "abcd", ""] {
            let err = normalize_recent_limit(Some(bad)).unwrap_err();
            assert_eq!(err.to_string(), "ліміт має бути 1–500");
        }
        // the window is inclusive and spans exactly `days` calendar
        // days ending AT the anchor (never the system clock): --days 1
        // is the anchor day itself, leap Februaries cross correctly,
        // and an impossible anchor validates to None
        assert_eq!(recent_cutoff("20261029", 1).unwrap(), "20261029");
        assert_eq!(recent_cutoff("20261029", 7).unwrap(), "20261023");
        assert_eq!(recent_cutoff("20260101", 7).unwrap(), "20251226");
        assert_eq!(recent_cutoff("20240301", 2).unwrap(), "20240229");
        assert_eq!(recent_cutoff("20230301", 2).unwrap(), "20230228");
        assert_eq!(recent_cutoff("20240101", 366).unwrap(), "20230101");
        // month-end and year-end walks (historic regression: the old
        // Julian-flavoured ymd_to_days formula collapsed 20261031 into
        // 20261101; ymd_to_days now delegates to this canonical pair
        // and the inverse battery below pins that behaviour)
        assert_eq!(recent_cutoff("20261101", 2).unwrap(), "20261031");
        assert_eq!(recent_cutoff("20261031", 31).unwrap(), "20261001");
        assert_eq!(recent_cutoff("20260101", 31).unwrap(), "20251202");
        assert_eq!(recent_cutoff("20261301", 7), None);
        assert_eq!(recent_cutoff("20260230", 7), None);
        assert_eq!(recent_cutoff("abcd", 7), None);
    }

    #[test]
    fn statuses_lines_renders_rows_and_total() {
        // header + one «{label} [{id}]: {count}» line per status in input
        // order (the SQL sorts by count descending — the ranking lives in
        // the order, no rank prefix, mirroring the `dates`/`dict-stats`
        // body), an unresolved status (the 0 / dict-miss-with-empty-label
        // case, already mapped by the caller) renders the «— не визначено»
        // placeholder, then the «всього документів» line (informational —
        // the command still exits 0)
        let rows = vec![
            StatusRow {
                id: 5,
                label: "Чинний".into(),
                count: 200000,
            },
            StatusRow {
                id: 1,
                label: "Втратив чинність".into(),
                count: 95339,
            },
            StatusRow {
                id: 0,
                label: "— не визначено".into(),
                count: 1,
            },
        ];
        assert_eq!(
            statuses_lines(&rows, 295340),
            vec![
                "Розподіл за статусами:".to_string(),
                "Чинний [5]: 200000".to_string(),
                "Втратив чинність [1]: 95339".to_string(),
                "— не визначено [0]: 1".to_string(),
                "всього документів: 295340".to_string(),
            ]
        );
        // empty index: the header still renders (the `--out` file is never
        // BOM-alone) and the verdict flips to «індекс порожній» (the
        // `dates_lines` empty idiom)
        assert_eq!(
            statuses_lines(&[], 0),
            vec![
                "Розподіл за статусами:".to_string(),
                "індекс порожній".to_string(),
            ]
        );
    }

    #[test]
    fn statuses_json_renders_statuses_and_total() {
        // stdout prints EXACTLY this body (+ newline); the `--out` file
        // receives the same bytes BOM-less (+ trailing newline). Map keys
        // sort, so «statuses» precedes «total» and «count»/«id»/«label»
        // keep their alphabetical order inside every entry.
        let rows = vec![
            StatusRow {
                id: 5,
                label: "Чинний".into(),
                count: 2,
            },
            StatusRow {
                id: 0,
                label: "— не визначено".into(),
                count: 3,
            },
        ];
        let body = statuses_json(&rows, 5);
        assert_eq!(
            body,
            "{\n  \"statuses\": [\n    {\n      \"count\": 2,\n      \"id\": 5,\n      \"label\": \"Чинний\"\n    },\n    {\n      \"count\": 3,\n      \"id\": 0,\n      \"label\": \"— не визначено\"\n    }\n  ],\n  \"total\": 5\n}"
        );
        assert!(!body.starts_with('\u{FEFF}'), "JSON body stays BOM-less");
        // empty histogram: a well-formed empty report with total 0
        assert_eq!(
            statuses_json(&[], 0),
            "{\n  \"statuses\": [],\n  \"total\": 0\n}"
        );
    }

    #[test]
    fn statuses_out_note_formats_count_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//s.txt");
        let line = statuses_out_note(10, file);
        assert_eq!(
            line,
            format!("[statuses] записано 10 статусів → {}", file.display())
        );
        assert!(line.starts_with("[statuses] "));
        assert!(line.contains("10"));
        assert!(line.contains("статусів"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
        // the informational zero rule: the note still formats with 0 rows
        assert_eq!(
            statuses_out_note(0, file),
            format!("[statuses] записано 0 статусів → {}", file.display())
        );
    }

    #[test]
    fn cache_search_json_body_renders_term_hits_and_empty() {
        // stdout prints EXACTLY this body (+ newline); the `--out` file
        // receives the same bytes BOM-less (+ trailing newline). Map keys
        // sort, so «hits» precedes «term» and «hits»/«nreg»/«snippet»/
        // «title» keep their alphabetical order inside every entry.
        let rows = vec![
            (
                rada_db::textcache::CacheHit {
                    nreg: "2755-17".into(),
                    hits: 3,
                    snippet: "…податок…".into(),
                },
                "Податковий кодекс".to_string(),
            ),
            (
                rada_db::textcache::CacheHit {
                    nreg: "9-9".into(),
                    hits: 1,
                    snippet: "податок".into(),
                },
                String::new(), // nreg missing from `docs`: empty title
            ),
        ];
        let body = cache_search_json_body("податок", &rows).unwrap();
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["term"], "податок");
        assert_eq!(v["hits"][0]["nreg"], "2755-17");
        assert_eq!(v["hits"][0]["title"], "Податковий кодекс");
        assert_eq!(v["hits"][0]["hits"], 3);
        assert_eq!(v["hits"][0]["snippet"], "…податок…");
        assert_eq!(v["hits"][1]["title"], "");
        assert!(body.starts_with('{'), "pretty object, like the other JSON bodies");
        assert!(!body.starts_with('\u{FEFF}'), "JSON body stays BOM-less");
        // zero hits: a well-formed empty body (the `dict --json` empty
        // style) — the stdout-only «збігів … немає» line is NOT part of it
        let empty = cache_search_json_body("ніщо", &[]).unwrap();
        let v: serde_json::Value = serde_json::from_str(&empty).unwrap();
        assert_eq!(v["term"], "ніщо");
        assert_eq!(v["hits"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn cache_search_json_out_note_formats_json_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//c.json");
        let line = cache_search_json_out_note(file);
        assert_eq!(line, format!("[cache] записано JSON → {}", file.display()));
        assert!(line.starts_with("[cache] "));
        assert!(line.contains("записано JSON"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
    }

    #[test]
    fn cache_stats_lines_renders_counts_and_megabytes() {
        // the bare `cache` report: one line, double-space separators, MB
        // with two decimals — stdout prints it verbatim and the `--out`
        // file receives the same bytes BOM-prefixed (+ trailing newline)
        assert_eq!(
            cache_stats_lines(5, 5, 5_075_943),
            vec!["rows=5  total_rows=5  size=4.84 MB".to_string()]
        );
        // exact-MB value formats with the historical two-decimal rule
        assert_eq!(
            cache_stats_lines(3, 10, 1_048_576),
            vec!["rows=3  total_rows=10  size=1.00 MB".to_string()]
        );
        // empty cache still renders its line: the `--out` file is never
        // BOM-alone
        assert_eq!(
            cache_stats_lines(0, 0, 0),
            vec!["rows=0  total_rows=0  size=0.00 MB".to_string()]
        );
    }

    #[test]
    fn cache_stats_out_note_formats_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//cache.txt");
        let line = cache_stats_out_note(file);
        assert_eq!(line, format!("[cache] записано → {}", file.display()));
        assert!(line.starts_with("[cache] "));
        assert!(line.contains("записано"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
    }

    #[test]
    fn arrivals_lines_renders_header_markers_and_tail() {
        // stdout prints EXACTLY these lines (each + newline); the `--out`
        // file receives the same bytes BOM-prefixed (+ trailing newline)
        let doc = |nreg: &str, status: &str, nazva: &str| rada_core::models::UpdatedDoc {
            num: 1,
            nreg: nreg.into(),
            nazva: nazva.into(),
            status: status.into(),
            publics: String::new(),
            link: String::new(),
            size: None,
            file: String::new(),
        };
        let feed = vec![
            doc("n1", "Чинний", "Акт перший"),
            doc("n2", "Не чинний", "Акт другий"),
            doc("n3", "Чинний", "Акт третій"),
        ];
        let new = vec!["n2".to_string()];
        // limit truncates the rows but not the header; the unseen nreg gets
        // the «* » marker, seen ones the «   » prefix; the tail counts the
        // rows beyond the limit
        assert_eq!(
            arrivals_lines(&feed, &new, 2),
            vec![
                "3 документів, нових: 1".to_string(),
                "  n1  [Чинний]  Акт перший".to_string(),
                "* n2  [Не чинний]  Акт другий".to_string(),
                "… ще 1 див. --limit".to_string(),
            ]
        );
        // no truncation: no tail line
        assert_eq!(arrivals_lines(&feed, &new, 30).len(), 4);
        // empty feed: the header always renders (the `--out` file is never
        // BOM-alone)
        assert_eq!(
            arrivals_lines(&[], &[], 30),
            vec!["0 документів, нових: 0".to_string()]
        );
    }

    #[test]
    fn arrivals_json_body_renders_items_and_new_nregs() {
        // stdout prints EXACTLY this body (+ newline); the `--out` file
        // receives the same bytes BOM-less (+ trailing newline)
        let doc = |nreg: &str| rada_core::models::UpdatedDoc {
            num: 7,
            nreg: nreg.into(),
            nazva: "Про тест".into(),
            status: "Чинний".into(),
            publics: "20241115".into(),
            link: "https://zakon.rada.gov.ua/go/1-2024/ed20241115".into(),
            size: Some(123),
            file: "d1.htm".into(),
        };
        let feed = vec![doc("n1")];
        let new = vec!["n1".to_string()];
        let body = arrivals_json_body(&feed, &new).unwrap();
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["items"][0]["nreg"], "n1");
        assert_eq!(v["items"][0]["nazva"], "Про тест");
        assert_eq!(v["new_nregs"][0], "n1");
        assert!(!body.starts_with('\u{FEFF}'), "JSON body stays BOM-less");
        // empty feed: a well-formed empty body (the shared empty style)
        let body = arrivals_json_body(&[], &[]).unwrap();
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["items"].as_array().unwrap().len(), 0);
        assert_eq!(v["new_nregs"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn arrivals_json_out_note_formats_json_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//a.json");
        let line = arrivals_json_out_note(file);
        assert_eq!(line, format!("[arrivals] записано JSON → {}", file.display()));
        assert!(line.starts_with("[arrivals] "));
        assert!(line.contains("записано JSON"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
    }

    #[test]
    fn resolve_json_body_renders_candidates_and_empty() {
        // stdout prints EXACTLY this body (+ newline); the `--out` file
        // receives the same bytes BOM-less (+ trailing newline). Map keys
        // sort: «candidates» wraps the serialized [`rada_db::SearchHit`]
        // field order.
        let hits = vec![rada_db::SearchHit {
            nreg: "2755-17".into(),
            nazva: "Податковий кодекс".into(),
            status: 5,
            org_date: "20101202".into(),
            org_num: "2755-VIII".into(),
            poddat: "20231102".into(),
        }];
        let body = resolve_json_body(&hits).unwrap();
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["candidates"][0]["nreg"], "2755-17");
        assert_eq!(v["candidates"][0]["nazva"], "Податковий кодекс");
        assert_eq!(v["candidates"][0]["status"], 5);
        assert!(!body.starts_with('\u{FEFF}'), "JSON body stays BOM-less");
        // the live-path shape: an empty local candidate list renders the
        // well-formed `{"candidates": []}` (the shared empty style)
        let body = resolve_json_body(&[]).unwrap();
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["candidates"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn resolve_json_out_note_formats_json_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//r.json");
        let line = resolve_json_out_note(file);
        assert_eq!(line, format!("[resolve] записано JSON → {}", file.display()));
        assert!(line.starts_with("[resolve] "));
        assert!(line.contains("записано JSON"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
    }

    #[test]
    fn status_lines_renders_all_rows_and_unknown_import() {
        // stdout prints EXACTLY these lines (each + newline); the `--out`
        // file receives the same bytes BOM-prefixed (+ trailing newline)
        let db = std::path::Path::new("data//zakon-index.sqlite");
        let lines = status_lines(
            db,
            10,
            Some("2026-10-01 10:00:00"),
            2_097_152, // exactly 2 MB
            [("stan", 10i64), ("typs", 138), ("organs", 799)],
        );
        assert_eq!(
            lines,
            vec![
                format!("index      : {}", db.display()),
                "documents  : 10".to_string(),
                "last import: 2026-10-01 10:00:00".to_string(),
                "db size    : 2.0 MB".to_string(),
                "dict stan   : 10 entries".to_string(),
                "dict typs   : 138 entries".to_string(),
                "dict organs : 799 entries".to_string(),
            ]
        );
        // every row always renders: a missing import is «unknown», an empty
        // index is a full zero report (the `--out` file is never BOM-alone)
        let lines = status_lines(
            std::path::Path::new("x.sqlite"),
            0,
            None,
            0,
            [("stan", 0), ("typs", 0), ("organs", 0)],
        );
        assert_eq!(lines[2], "last import: unknown");
        assert_eq!(lines[3], "db size    : 0.0 MB");
        assert_eq!(lines.len(), 7);
    }

    #[test]
    fn status_out_note_formats_arrow_and_path() {
        // double separator keeps the test path-separator-stable via display()
        let file = std::path::Path::new("out//st.txt");
        let line = status_out_note(file);
        assert_eq!(line, format!("[status] записано → {}", file.display()));
        assert!(line.starts_with("[status] "));
        assert!(line.contains("записано"));
        assert!(line.contains('→'));
        assert!(line.contains(&file.display().to_string()));
    }

    #[test]
    fn ymd_to_days_is_the_exact_inverse_of_days_to_ymd() {
        // epoch: the pair must agree at 0 <-> 19700101 (the pre-fix
        // Julian-flavoured formula returned 2440496 at 19700101, and
        // days_to_ymd(2440496) = 86511108 — not a round trip)
        assert_eq!(ymd_to_days("19700101"), Some(0));
        assert_eq!(days_to_ymd(0), "19700101");
        // round-trip battery: leap days, month ends, year boundaries,
        // historic and far-future dates all survive ymd -> days -> ymd
        let battery = [
            "20240229", "20000229", "19000228", "20260131", "20261031", "20261231", "20260101",
            "20270101", "17100101", "21001231",
        ];
        for s in battery {
            let z = ymd_to_days(s).unwrap_or_else(|| panic!("{s} rejected"));
            assert_eq!(days_to_ymd(z), s, "round trip {s}");
        }
        // strict consecutive-day monotonicity (the pre-fix formula
        // collapsed 20261031 and 20261101 onto the same 2461254, and
        // 20261231 and 20270101 onto the same 2461315)
        let d30 = ymd_to_days("20261030").unwrap();
        let d31 = ymd_to_days("20261031").unwrap();
        let n01 = ymd_to_days("20261101").unwrap();
        assert!(d30 < d31 && d31 < n01, "strictly increasing");
        assert_eq!(d31 - d30, 1);
        assert_eq!(n01 - d31, 1);
        assert_eq!(
            ymd_to_days("20270101").unwrap() - ymd_to_days("20261231").unwrap(),
            1
        );
        // Feb -> Mar steps exactly one day, leap and non-leap alike
        // (the pre-fix formula drifted +3 here: 2461011 - 2461008)
        assert_eq!(
            ymd_to_days("20260301").unwrap() - ymd_to_days("20260228").unwrap(),
            1
        );
        assert_eq!(
            ymd_to_days("20240301").unwrap() - ymd_to_days("20240229").unwrap(),
            1
        );
        assert_eq!(
            ymd_to_days("20240229").unwrap() - ymd_to_days("20240228").unwrap(),
            1
        );
        // impossible / malformed inputs keep being rejected
        for bad in [
            "20250229", "20260230", "20261301", "20260000", "abcd", "2026103", "202610313",
        ] {
            assert_eq!(ymd_to_days(bad), None, "{bad} rejected");
        }
    }

    #[test]
    fn ymd_to_days_agrees_with_recent_days_from_civil_on_the_battery() {
        // one canonical day count for the whole CLI; the absolute values
        // were cross-checked against an independent [datetime] oracle
        let battery = [
            "20240229", "20000229", "19000228", "20260131", "20261030", "20261031", "20261101",
            "20261231", "20260101", "20270101", "17100101", "21001231", "20260227", "20260228",
            "20260301", "19700101",
        ];
        for s in battery {
            assert_eq!(ymd_to_days(s), recent_days_from_civil(s), "{s}");
        }
        // absolute epoch-day anchors from the independent oracle
        assert_eq!(ymd_to_days("20261031"), Some(20757));
        assert_eq!(ymd_to_days("20261101"), Some(20758));
        assert_eq!(ymd_to_days("17100101"), Some(-94963));
        assert_eq!(ymd_to_days("21001231"), Some(47846));
        // rejections agree too (identical validation contract)
        for bad in ["", "20250229", "20261301", "1234567", "20260230"] {
            assert_eq!(ymd_to_days(bad), recent_days_from_civil(bad), "{bad}");
        }
    }

    #[test]
    fn filter_since_day_math_is_exact_across_month_end_and_leap_boundaries() {
        // filter_since is the shipped consumer of ymd_to_days day
        // DIFFERENCES. Pre-fix the formula collapsed 20261031 into
        // 20261101 (days=0 wrongly kept yesterday) and drifted +3 from
        // Feb 28 to Mar 1 (days=2 wrongly dropped 20260228 vs 20260302)
        let docs: Vec<(&str, &str)> = vec![
            ("/ed20261101", ""), // 0 today — kept even at days 0
            ("/ed20261031", ""), // 1 yesterday — kept from days 1
            ("/ed20261030", ""), // 2 two days back — kept from days 2
        ];
        assert_eq!(filter_since(&docs, 0, "20261101"), vec![0]);
        assert_eq!(filter_since(&docs, 1, "20261101"), vec![0, 1]);
        assert_eq!(filter_since(&docs, 2, "20261101"), vec![0, 1, 2]);
        // Feb -> Mar of a non-leap year: pre-fix drift made 20260228
        // look FOUR days older than 20260302 instead of two
        let febmar: Vec<(&str, &str)> = vec![
            ("/ed20260302", ""), // 0 today
            ("/ed20260301", ""), // 1 yesterday
            ("/ed20260228", ""), // 2 two days back (2026 is not a leap year)
            ("/ed20260227", ""), // 3 three days back — dropped at days 2
        ];
        assert_eq!(filter_since(&febmar, 2, "20260302"), vec![0, 1, 2]);
        assert_eq!(filter_since(&febmar, 3, "20260302"), vec![0, 1, 2, 3]);
        // leap day: 20240229 is exactly one day before 20240301
        let leap: Vec<(&str, &str)> = vec![("/ed20240229", ""), ("/ed20240228", "")];
        assert_eq!(filter_since(&leap, 1, "20240301"), vec![0]);
        assert_eq!(filter_since(&leap, 2, "20240301"), vec![0, 1]);
    }

    #[test]
    fn search_date_window_endpoints_are_inclusive_and_match_day_numbers() {
        // `search --date-from/--date-to` bounds rows with INCLUSIVE
        // string comparisons on the 8-digit org_date (rada_db::search:
        // `org_date >= ?1 AND org_date <= ?2`). That string windowing
        // is calendar-correct exactly while ymd_to_days is strictly
        // monotonic and epoch-consistent: pin that day-number window
        // membership equals SQL string-range membership on every
        // candidate across a Feb/Mar boundary, both endpoints included
        let (from, to) = ("20260226", "20260302");
        let zf = ymd_to_days(from).unwrap();
        let zt = ymd_to_days(to).unwrap();
        let candidates = [
            "20260225", "20260226", "20260227", "20260228", "20260301", "20260302", "20260303",
        ];
        for s in candidates {
            let z = ymd_to_days(s).unwrap();
            assert_eq!(
                zf <= z && z <= zt,
                from <= s && s <= to,
                "endpoint-inclusive window membership {s}"
            );
        }
        // both endpoints belong to the window (inclusive-endpoint rule)
        assert!(from <= from && from <= to && to <= to);
        // the day-level inclusive window helper: 237 days ending at
        // 20261023 start exactly on 20260301 (236 calendar days back,
        // oracle-verified) and span 237 consecutive day numbers
        let cutoff = recent_cutoff("20261023", 237).unwrap();
        assert_eq!(cutoff, "20260301");
        assert_eq!(ymd_to_days("20261023").unwrap() - ymd_to_days(&cutoff).unwrap(), 236);
    }
}
