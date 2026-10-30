//! GUI application state: pages, background worker plumbing, message pump.
//!
//! Threading: egui is immediate mode and single-threaded, so all blocking
//! work (SQLite reads, paced live API calls, snapshot downloads) runs on
//! short-lived worker threads that report back through an `mpsc` channel.
//! Stale results are filtered with per-request generation counters.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;
use rada_core::models::{LawCardJson, UpdatedDoc};
use rada_core::RadaClient;

use crate::views;

/// Awaited result as `Result<T, String>` (thread-crossable error text).
pub type Outcome<T> = Result<T, String>;

/// Background result messages.
pub enum Msg {
    Status(IndexStatus),
    /// `(rows, body bytes)` from the offline text cache (home page row).
    CacheStats((i64, i64)),
    /// Rows of one adoption year for the home «Документи за роками» drill-down:
    /// `(org_date YYYYMMDD, nreg, nazva)`, newest first, capped at 200.
    YearDocs {
        run: u64,
        yyyy: String,
        res: Result<Vec<(String, String, String)>, String>,
    },
    /// Result of the home page cache purge: deleted rows.
    CachePurged(Outcome<usize>),
    Dicts {
        stan: Vec<(i64, String)>,
        typs: Vec<(i64, String)>,
        organs: Vec<(i64, String)>,
    },
    Search {
        run: u64,
        res: Outcome<(Vec<rada_db::SearchHit>, i64)>,
        /// Nregs of the hits whose body text is in the offline cache
        /// (the «кеш» badge); empty on error or for older senders.
        cached: Vec<String>,
    },
    /// Per-option counts for the filter columns of a search (advisory:
    /// errors never disturb the hit list).
    Facets {
        run: u64,
        res: Outcome<rada_db::FacetCounts>,
    },
    /// Offline full-text scan of the cached bodies: `(hit, index title)`.
    CacheSearch {
        run: u64,
        res: Outcome<Vec<(rada_db::textcache::CacheHit, String)>>,
    },
    Card {
        run: u64,
        res: Outcome<LawCardJson>,
    },
    Text {
        run: u64,
        /// `(text, from_cache)` — the offline cache answers when live fails.
        res: Outcome<(String, bool)>,
    },
    /// Both texts of the edition comparison, in (A, B) order.
    Compare {
        run: u64,
        res: Outcome<(String, String)>,
    },
    Annotations {
        nreg: String,
        res: Outcome<Vec<rada_db::Annotation>>,
    },
    /// `(kind, result)` — kind lets the toast match the saved annotation type.
    AnnotationSaved(String, Outcome<()>),
    AnnotationDeleted(Outcome<bool>),
    Arrivals {
        run: u64,
        /// `(docs, from_cache)` — the saved snapshot answers when live fails.
        res: Outcome<(Vec<UpdatedDoc>, bool)>,
    },
    /// Every annotation + document title (favorites page).
    Favorites(Outcome<Vec<(rada_db::Annotation, String)>>),
    /// Annotations imported from a JSON file (count imported).
    AnnImported(Outcome<usize>),
    /// Live bulk-export progress (done / total documents).
    LiveExport { done: u64, total: u64 },
    SyncLine(String),
    SyncDone(Outcome<(usize, usize)>),
    FetchProgress(FetchProg),
    FetchDone(Outcome<PathBuf>),
    /// Local gzip backup of the index finished (file, bytes written).
    Backup(Outcome<(PathBuf, u64)>),
    /// Restore from a user-picked local snapshot file finished.
    LocalRestore(Outcome<PathBuf>),
    /// Full-dump init finished (documents imported).
    InitDone(Outcome<usize>),
    Resolved {
        run: u64,
        res: Option<String>,
    },
    Exported(Outcome<PathBuf>),
    /// The query was a document number found exactly in the index —
    /// open it in the reader directly (offline number search).
    OpenNreg(rada_db::SearchHit),
    /// «Пов'язані документи» parsed from the live page.
    Related {
        run: u64,
        nreg: String,
        res: Outcome<Vec<rada_core::related::RelatedDoc>>,
    },
    /// Individual-file export of checked results finished.
    SelectedExported { written: usize, failed: usize, dir: PathBuf },
    /// Dictionaries-only live fetch finished (auto-run at startup when empty).
    /// Ok(list of failed dict kinds; empty = all loaded), Err = fatal.
    DictsFetched(Outcome<Vec<String>>),
}

pub enum FetchProg {
    /// Resolving the asset URL.
    Resolve,
    /// Download in progress (bytes so far).
    Download { done: u64 },
    /// Gunzip + integrity check.
    Restore,
}

#[derive(Clone)]
pub struct IndexStatus {
    pub docs: i64,
    pub last_import: Option<String>,
    /// Newest `org_date` (YYYYMMDD) among indexed cards: the freshest data
    /// actually fetched from rada — the calendar anchor.
    pub max_date: Option<String>,
    pub size_bytes: u64,
    pub dicts: Vec<(String, i64)>,
    /// Freshest cards by adoption date (offline, index-only):
    /// `(org_date YYYYMMDD, nreg, nazva)`, newest first, capped at 14.
    pub fresh: Vec<(String, String, String)>,
    /// Whole-index per-year document counts (offline, index-only):
    /// `(yyyy, count)`, ascending, only full 8-digit dates; the home page
    /// renders the last 15 present years (CLI `years` / web landing twin).
    pub years: Vec<(String, i64)>,
}

/// One open document in the reader's tab strip. Only the ACTIVE tab keeps
/// a live `ReaderState` (in `App::reader`); inactive tabs park theirs in
/// `saved`. Tabs restored from settings.ini start as lazy shells
/// (`saved: None`) and fetch their text on first activation.
#[derive(Clone)]
pub struct ReaderTab {
    pub nreg: String,
    pub title: String,
    pub saved: Option<views::ReaderState>,
}

/// Push a closed tab `(nreg, title)` onto the redo stack (most recent last):
/// a repeat of the same nreg moves to the top instead of doubling up, and
/// the stack is capped at 10 (oldest dropped).
fn closed_push(stack: &mut Vec<(String, String)>, item: (String, String)) {
    if let Some(pos) = stack.iter().position(|(n, _)| *n == item.0) {
        stack.remove(pos);
    }
    stack.push(item);
    while stack.len() > 10 {
        stack.remove(0);
    }
}

/// Pop the most recently closed tab for reopening; empty stack → `None`.
fn closed_pop(stack: &mut Vec<(String, String)>) -> Option<(String, String)> {
    stack.pop()
}

/// Which app page is on screen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Home,
    Search,
    Reader,
    Arrivals,
    Favorites,
}

/// Alt+digit quick switching: 1=Головна, 2=Пошук, 3=Читалка, 4=Оновлення,
/// 5=Обране. Other digits (and 0) map to `None`.
pub fn page_for_alt_digit(digit: u8) -> Option<Page> {
    match digit {
        1 => Some(Page::Home),
        2 => Some(Page::Search),
        3 => Some(Page::Reader),
        4 => Some(Page::Arrivals),
        5 => Some(Page::Favorites),
        _ => None,
    }
}

/// Launch-time options (CLI `--start`, `--query`, `--nreg`, `--cache-search`).
#[derive(Default)]
pub struct Startup {
    /// `home` | `search` | `arrivals`
    pub start_page: Option<String>,
    /// Pre-fill and run a title search on startup.
    pub query: Option<String>,
    /// Open this document (nreg) in the reader on startup.
    pub nreg: Option<String>,
    /// Run an offline cache full-text search for this term on startup.
    pub cache_query: Option<String>,
}

pub struct App {
    pub db_path: PathBuf,
    pub pace_secs: u64,
    pub client: Arc<RadaClient>,
    rx: std::sync::mpsc::Receiver<Msg>,
    tx: Sender<Msg>,
    run_gen: u64,

    pub page: Page,
    pub home: views::HomeState,
    pub search: views::SearchState,
    pub reader: Option<views::ReaderState>,
    /// Reader tab strip: every tab opened this session (and restored ones).
    /// `reader` mirrors the active tab's live state.
    pub tabs: Vec<ReaderTab>,
    pub active_tab: Option<usize>,
    /// Recently closed tabs `(nreg, title)`, oldest first; Ctrl+Shift+T pops
    /// the last entry back. Cap 10.
    pub closed_tabs: Vec<(String, String)>,
    /// Results list the reader was opened from (for prev/next paging).
    pub nav: Option<(Vec<rada_db::SearchHit>, usize)>,
    pub arrivals: views::ArrivalsState,
    pub fav: views::FavoritesState,
    pub dicts: views::Dicts,
    pub index_status: Option<IndexStatus>,
    /// Offline text-cache size `(rows, body bytes)` (home page row).
    pub cache_stats: Option<(i64, i64)>,
    pub toasts: Vec<Toast>,
    /// «Про програму» dialog visibility.
    pub show_about: bool,
    /// «Гарячі клавіші» sheet visibility (topbar «клавіші»).
    pub show_help: bool,
    /// Last opened doc / last query, persisted for "Continue".
    pub settings: crate::settings::Settings,
    /// Signature of the visuals currently applied (theme id + OS theme),
    /// so `refresh_theme` is a no-op while nothing changes.
    theme_sig: String,
    /// Widget a global shortcut (Ctrl+F) asked to focus this frame.
    focus_req: Option<FocusReq>,
}

/// Widget that a global shortcut asked to focus.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FocusReq {
    ReaderFind,
    SearchQuery,
    ArrivalsFilter,
    FavoritesFilter,
}

pub struct Toast {
    pub text: String,
    pub color: egui::Color32,
    pub born: Instant,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        db_path: PathBuf,
        pace: Option<u64>,
        startup: Startup,
    ) -> Self {
        apply_spacing(&cc.egui_ctx);
        let (tx, rx) = mpsc::channel();
        let settings = crate::settings::load();
        // CLI --pace wins; otherwise the persisted choice; otherwise 5 s.
        let pace_secs = pace.or(if settings.pace > 0 { Some(settings.pace) } else { None }).unwrap_or(5);
        let mut app = Self {
            db_path,
            pace_secs,
            client: Arc::new(RadaClient::with_pacing(Duration::from_secs(pace_secs))),
            rx,
            tx,
            run_gen: 0,
            page: Page::Home,
            home: views::HomeState::default(),
            search: views::SearchState::default(),
            reader: None,
            tabs: Vec::new(),
            active_tab: None,
            closed_tabs: Vec::new(),
            nav: None,
            arrivals: views::ArrivalsState::default(),
            fav: views::FavoritesState::default(),
            dicts: views::Dicts::default(),
            index_status: None,
            cache_stats: None,
            toasts: Vec::new(),
            show_about: false,
            show_help: false,
            settings,
            theme_sig: String::new(),
            focus_req: None,
        };
        if startup.query.is_none() && startup.nreg.is_none() && startup.cache_query.is_none() {
            app.search.query = app.settings.last_query.clone();
        }
        // The arrivals text filter survives restarts.
        app.arrivals.filter = app.settings.arr_filter.clone();
        // Open reader tabs come back as lazy shells (no fetch until clicked).
        app.tabs = app
            .settings
            .tabs
            .iter()
            .map(|(n, t)| ReaderTab {
                nreg: n.clone(),
                title: t.clone(),
                saved: None,
            })
            .collect();
        // So does the Favorites view: kind/sort chips, grouping, filter.
        app.fav.kind_sel = app.settings.fav_kind.min(3);
        app.fav.sort_sel = app.settings.fav_sort.min(2);
        if let Some(g) = app.settings.fav_grouped {
            app.fav.grouped = g;
        }
        app.fav.filter = app.settings.fav_filter.clone();
        // So does the chosen result sort («date_desc» etc., lenient parse).
        app.search.sort = rada_db::search::Sort::parse(&app.settings.sort);
        // And the dictionary filters of the last searched query (0 = unset).
        app.search.f_status = (app.settings.f_status != 0).then_some(app.settings.f_status);
        app.search.f_typ = (app.settings.f_typ != 0).then_some(app.settings.f_typ);
        app.search.f_org = (app.settings.f_org != 0).then_some(app.settings.f_org);
        app.search.f_org_label = app.settings.f_org_label.clone();
        app.theme_sig = String::new();
        app.refresh_theme(&cc.egui_ctx);
        app.refresh_index_state();
        app.refresh_dicts();
        app.apply_startup(startup);
        app
    }

    /// Apply launch-time options (deep link into a page / search / document).
    fn apply_startup(&mut self, s: Startup) {
        if let Some(n) = s.nreg {
            self.open_reader_by_nreg(&n, "(відкрито з рядка команди)", 0, "", "", "");
            return;
        }
        if let Some(q) = s.query {
            self.search.query = q;
            self.run_search();
            self.page = Page::Search;
            return;
        }
        if let Some(t) = s.cache_query {
            self.search.query = t;
            self.run_cache_search();
            self.page = Page::Search;
            return;
        }
        if let Some(p) = s.start_page {
            match p.to_lowercase().as_str() {
                "search" => self.page = Page::Search,
                "arrivals" => self.page = Page::Arrivals,
                "favorites" => self.page = Page::Favorites,
                "home" => self.page = Page::Home,
                _ => {}
            }
        }
    }

    // -----------------------------------------------------------------
    // plumbing
    // -----------------------------------------------------------------

    /// New generation counter for a request; responses from older runs
    /// are dropped on arrival.
    fn next_run(&mut self) -> u64 {
        self.run_gen += 1;
        self.run_gen
    }

    pub fn toast(&mut self, text: impl Into<String>, color: egui::Color32) {
        self.toasts.push(Toast {
            text: text.into(),
            color,
            born: Instant::now(),
        });
        if self.toasts.len() > 5 {
            self.toasts.remove(0);
        }
    }

    /// Keyboard reader-zoom control (Ctrl+Plus / Ctrl+Minus / Ctrl+0).
    /// `delta` is ±10; 0 resets to the default (stored as 0 = 100%).
    pub fn zoom_reader(&mut self, delta: i32) {
        self.settings.zoom = if delta == 0 {
            0
        } else {
            views::reader::zoom_step(self.settings.zoom, delta)
        };
        crate::settings::save(&self.settings);
        let shown = if self.settings.zoom == 0 {
            100
        } else {
            self.settings.zoom
        };
        self.toast(format!("читалка: {shown}%"), ok());
    }

    pub fn spawn<F>(&self, f: F)
    where
        F: FnOnce(Sender<Msg>) + Send + 'static,
    {
        let tx = self.tx.clone();
        std::thread::spawn(move || f(tx));
    }

    /// True (exactly once) if this frame's shortcut asked for `want` focus.
    pub fn take_focus(&mut self, want: FocusReq) -> bool {
        if self.focus_req == Some(want) {
            self.focus_req = None;
            true
        } else {
            false
        }
    }

    /// (Re)apply the color theme when the id changed or, for `auto`, when the
    /// OS flipped between light and dark. Cheap to call every frame.
    pub fn refresh_theme(&mut self, ctx: &egui::Context) {
        let sys = ctx.system_theme();
        let sys_s = match sys {
            Some(egui::Theme::Light) => "l",
            Some(egui::Theme::Dark) => "d",
            None => "-",
        };
        let sig = format!("{}{sys_s}", self.settings.theme_id());
        if sig != self.theme_sig {
            self.theme_sig = sig;
            let id = self.settings.theme_id().to_string();
            crate::themes::apply(ctx, &id, sys);
            ctx.request_repaint();
        }
    }

    /// Switch the theme, persisting the choice.
    pub fn set_theme(&mut self, id: &str) {
        self.settings.theme = id.to_string();
        crate::settings::save(&self.settings);
    }

    pub fn refresh_index_state(&mut self) {
        let db = self.db_path.clone();
        self.spawn(move |tx| {
            let _ = tx.send(Msg::Status(read_index_status(&db)));
            // Text-cache size rides along with the index status refresh.
            if let Ok(conn) = rada_db::open(&db) {
                if let Ok(pair) = rada_db::textcache::stats(&conn) {
                    let _ = tx.send(Msg::CacheStats(pair));
                }
            }
        });
    }

    /// Open the home «Документи за роками» drill-down: the newest 200 cards
    /// of one adoption year, queried offline straight from the index.
    pub fn open_year_docs(&mut self, yyyy: &str) {
        let run = self.next_run();
        self.home.year_docs = Some(crate::views::home::YearDocs {
            yyyy: yyyy.to_string(),
            run,
            loading: true,
            rows: Vec::new(),
            error: None,
            open: true,
        });
        let db = self.db_path.clone();
        let yyyy = yyyy.to_string();
        self.spawn(move |tx| {
            let res = rada_db::open(&db)
                .map_err(|e| e.to_string())
                .and_then(|conn| {
                    let mut ps = conn
                        .prepare(
                            "SELECT org_date, nreg, nazva FROM docs \
                             WHERE org_date GLOB ?1 \
                             ORDER BY org_date DESC, nreg ASC LIMIT 200",
                        )
                        .map_err(|e| e.to_string())?;
                    let rows = ps
                        .query_map([format!("{yyyy}[0-9][0-9][0-9][0-9]")], |r| {
                            Ok((
                                r.get::<_, String>(0)?,
                                r.get::<_, String>(1)?,
                                r.get::<_, String>(2)?,
                            ))
                        })
                        .map_err(|e| e.to_string())?
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|e| e.to_string())?;
                    Ok(rows)
                });
            let _ = tx.send(Msg::YearDocs { run, yyyy, res });
        });
    }

    /// Delete every cached document body (home page «очистити кеш»), then
    /// refresh the cache row. Annotations and the index are untouched.
    pub fn purge_text_cache(&mut self) {
        let db = self.db_path.clone();
        self.spawn(move |tx| {
            let res = rada_db::open(&db)
                .and_then(|conn| rada_db::textcache::purge(&conn, None))
                .map_err(|e| e.to_string());
            let _ = tx.send(Msg::CachePurged(res));
        });
    }

    pub fn refresh_dicts(&mut self) {        let db = self.db_path.clone();
        self.spawn(move |tx| {
            let msg = match rada_db::open(&db) {
                Ok(conn) => Msg::Dicts {
                    stan: rada_db::dict::all_labels(&conn, "stan"),
                    typs: rada_db::dict::all_labels(&conn, "typs"),
                    organs: rada_db::dict::all_labels(&conn, "organs"),
                },
                Err(_) => Msg::Dicts {
                    stan: Vec::new(),
                    typs: Vec::new(),
                    organs: Vec::new(),
                },
            };
            let _ = tx.send(msg);
        });
    }

    /// Live-fetch only the dictionaries (3 paced requests) — run automatically
    /// when the index exists but the dict tables are empty (e.g. after a
    /// snapshot restore, where dicts are not part of the dump).
    pub fn fetch_dicts_live(&mut self) {
        let client = self.client.clone();
        let db = self.db_path.clone();
        self.spawn(move |tx| {
            let res = (|| {
                let mut conn = rada_db::open(&db).map_err(|e| e.to_string())?;
                let failed =
                    rada_db::sync::refresh_dicts(&mut conn, |kind| Ok(client.dictionary(kind)?))
                        .map_err(|e| format!("{e:#}"))?;
                Ok(failed)
            })();
            let _ = tx.send(Msg::DictsFetched(res));
        });
    }

    // -----------------------------------------------------------------
    // actions
    // -----------------------------------------------------------------

    pub fn run_search(&mut self) {
        let run = self.next_run();
        let db = self.db_path.clone();
        let q = self.search.build_query();
        if self.search.invalid_date {
            self.toast("дата: потрібен формат РРРРММДД (наприклад 20240115)", err());
        }
        self.search.run = run;
        self.search.loading = true;
        self.search.searched = true;
        self.search.cached.clear();
        crate::settings::remember_query(&mut self.settings, &self.search.query);
        // Snapshot the dictionary filters actually used in this search so the
        // next launch starts with the same domain (0 = unset).
        self.settings.f_status = self.search.f_status.unwrap_or(0);
        self.settings.f_typ = self.search.f_typ.unwrap_or(0);
        self.settings.f_org = self.search.f_org.unwrap_or(0);
        self.settings.f_org_label = self.search.f_org_label.clone();
        crate::settings::save(&self.settings);
        let nreg_guess = self.search.query.trim().to_string();
        let facets_q = q.clone();
        self.spawn(move |tx| {
            let conn = match rada_db::open(&db) {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(Msg::Search {
                        run,
                        res: Err(e.to_string()),
                        cached: Vec::new(),
                    });
                    return;
                }
            };
            // Offline number search: a query shaped like an nreg opens the
            // document directly when the index holds it exactly.
            if rada_db::looks_like_nreg(&nreg_guess) {
                if let Ok(Some(hit)) = rada_db::by_nreg(&conn, &nreg_guess) {
                    let _ = tx.send(Msg::OpenNreg(hit));
                    return;
                }
            }
            let res = rada_db::search::search(&conn, &q).map_err(|e| e.to_string());
            // «кеш» badge: which of these hits can already be read offline
            // (their body lives in the text cache). Best-effort: a cache
            // probe failure just means no badge.
            let cached: Vec<String> = res
                .as_ref()
                .map(|(hits, _)| {
                    hits.iter()
                        .filter(|h| rada_db::textcache::cached(&conn, &h.nreg).unwrap_or(false))
                        .map(|h| h.nreg.clone())
                        .collect()
                })
                .unwrap_or_default();
            let _ = tx.send(Msg::Search { run, res, cached });
        });
        // Advisory facet counts for the same query (separate worker + run id;
        // failures here never touch the hit list).
        let facets_run = self.next_run();
        let fdb = self.db_path.clone();
        self.search.facets_run = facets_run;
        self.search.facets_loading = true;
        self.search.facets = None;
        self.search.facets_for = self.search.query.trim().to_string();
        self.spawn(move |tx| {
            let res = (|| -> Outcome<rada_db::FacetCounts> {
                let conn = rada_db::open(&fdb).map_err(|e| e.to_string())?;
                rada_db::facets(&conn, &facets_q).map_err(|e| e.to_string())
            })();
            let _ = tx.send(Msg::Facets {
                run: facets_run,
                res,
            });
        });
    }

    /// Offline full-text search over the cached document bodies
    /// («Знайти в кеші»): Unicode case-insensitive substring scan ranked by
    /// occurrence count, current edition only (see textcache::search_bodies).
    pub fn run_cache_search(&mut self) {
        let term = self.search.query.trim().to_string();
        if term.is_empty() {
            self.toast("введіть текст для пошуку в кеші", err());
            return;
        }
        let run = self.next_run();
        let db = self.db_path.clone();
        self.search.cache_run = run;
        self.search.cache_loading = true;
        self.search.cache_done = false;
        self.search.cache_term = term.clone();
        self.search.cache_hits.clear();
        self.search.cache_error = None;
        self.spawn(move |tx| {
            let res = (|| -> Outcome<Vec<(rada_db::textcache::CacheHit, String)>> {
                let conn = rada_db::open(&db).map_err(|e| e.to_string())?;
                let hits = rada_db::textcache::search_bodies(&conn, &term, 50)
                    .map_err(|e| e.to_string())?;
                Ok(hits
                    .into_iter()
                    .map(|h| {
                        let title = rada_db::title_by_nreg(&conn, &h.nreg).unwrap_or_default();
                        (h, title)
                    })
                    .collect())
            })();
            let _ = tx.send(Msg::CacheSearch { run, res });
        });
    }

    pub fn reader_open_on(&self, nreg: &str) -> bool {
        self.reader
            .as_ref()
            .map(|r| r.hit.nreg == nreg)
            .unwrap_or(false)
    }

    /// Open a document in the reader: live card + text fetched in the
    /// background (paced by the shared client). Documents already present in
    /// the tab strip are re-activated instead of opened twice.
    pub fn open_reader(&mut self, hit: rada_db::SearchHit) {
        self.page = Page::Reader;
        self.nav = None;
        if let Some(i) = self.tabs.iter().position(|t| t.nreg == hit.nreg) {
            self.tab_activate(i);
            return;
        }
        self.tab_suspend_active();
        self.tabs.push(ReaderTab {
            nreg: hit.nreg.clone(),
            title: hit.nazva.clone(),
            saved: None,
        });
        self.active_tab = Some(self.tabs.len() - 1);
        self.remember_tabs();
        self.reader_launch(hit);
    }

    /// The shared launch sequence: remember the recents entry, build a fresh
    /// `ReaderState` and kick off the paced card/text fetches.
    fn reader_launch(&mut self, hit: rada_db::SearchHit) {
        crate::settings::remember(&mut self.settings, &hit.nreg, &hit.nazva);
        crate::settings::save(&self.settings);
        let mut r = views::ReaderState::new(hit);
        let card_run = self.next_run();
        let text_run = self.next_run();
        r.card_run = card_run;
        r.text_run = text_run;
        r.card_loading = true;
        r.text_loading = true;
        self.reader = Some(r);
        self.load_annotations();
        self.fetch_card(card_run);
        self.fetch_text(text_run, None);
    }

    fn tab_suspend_active(&mut self) {
        if let (Some(a), Some(r)) = (self.active_tab, self.reader.take()) {
            if a < self.tabs.len() {
                self.tabs[a].saved = Some(r);
            }
        }
    }

    /// Bring tab `i` to the front: park the current reader, then either
    /// restore the tab's parked state (text, find query and edition kept
    /// as-is) or fetch it from scratch when the tab is still a lazy shell.
    /// A tab suspended mid-fetch is re-armed here, because background results
    /// are routed only to the active reader.
    pub fn tab_activate(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        if self.active_tab == Some(i) && self.reader.is_some() {
            self.page = Page::Reader;
            return;
        }
        let st = self.tabs[i].saved.take();
        self.tab_suspend_active();
        self.active_tab = Some(i);
        self.page = Page::Reader;
        self.nav = None;
        match st {
            Some(st) => {
                let (redo_card, redo_text, edition) =
                    (st.card_loading, st.text_loading, st.cur_edition.clone());
                self.reader = Some(st);
                self.load_annotations();
                if redo_card {
                    let run = self.next_run();
                    if let Some(r) = self.reader.as_mut() {
                        r.card_run = run;
                        r.card_loading = true;
                    }
                    self.fetch_card(run);
                }
                if redo_text {
                    let run = self.next_run();
                    self.fetch_text(run, edition);
                }
            }
            None => {
                let (n, t) = (self.tabs[i].nreg.clone(), self.tabs[i].title.clone());
                self.reader_launch(rada_db::SearchHit {
                    nreg: n,
                    nazva: t,
                    status: 0,
                    org_date: String::new(),
                    org_num: String::new(),
                    poddat: String::new(),
                });
            }
        }
    }

    /// Close tab `i`; its left neighbour (or the new first tab) takes focus.
    /// The closed document lands on the redo stack (Ctrl+Shift+T).
    pub fn tab_close(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        let was_active = self.active_tab == Some(i);
        let closed = (self.tabs[i].nreg.clone(), self.tabs[i].title.clone());
        closed_push(&mut self.closed_tabs, closed);
        self.tabs.remove(i);
        if let Some(a) = self.active_tab {
            if a > i {
                self.active_tab = Some(a - 1);
            } else if a == i {
                self.active_tab = None;
            }
        }
        if was_active {
            self.reader = None;
            if !self.tabs.is_empty() {
                let j = i.min(self.tabs.len() - 1);
                self.tab_activate(j);
            }
        }
        self.remember_tabs();
    }

    /// Ctrl+Shift+T: reopen the most recently closed tab (redo stack).
    pub fn tab_reopen(&mut self) {
        if let Some((nreg, title)) = closed_pop(&mut self.closed_tabs) {
            if self.reader_open_on(&nreg) || self.tabs.iter().any(|t| t.nreg == nreg) {
                return;
            }
            self.open_reader(rada_db::SearchHit {
                nreg,
                nazva: title,
                status: 0,
                org_date: String::new(),
                org_num: String::new(),
                poddat: String::new(),
            });
        }
    }

    /// Ctrl+Tab / Ctrl+Shift+Tab: cycle through the open tabs (wrap-around).
    pub fn tab_cycle(&mut self, delta: i64) {
        if self.tabs.len() < 2 {
            return;
        }
        let n = self.tabs.len() as i64;
        let cur = self.active_tab.unwrap_or(0) as i64;
        let ni = (cur + delta).rem_euclid(n);
        self.tab_activate(ni as usize);
    }

    /// Persist the open tabs (up to 12) to settings for the next launch.
    fn remember_tabs(&mut self) {
        self.settings.tabs = self
            .tabs
            .iter()
            .take(12)
            .map(|t| (t.nreg.clone(), t.title.clone()))
            .collect();
        crate::settings::save(&self.settings);
    }

    /// Open the document at `pos` of the visible results list and remember
    /// the list so the reader can page through it (prev/next).
    pub fn open_reader_from(&mut self, hits: Vec<rada_db::SearchHit>, pos: usize) {
        if pos >= hits.len() {
            return;
        }
        self.open_reader(hits[pos].clone());
        self.nav = Some((hits, pos));
    }

    /// Prev/next document inside the remembered results list.
    pub fn nav_step(&mut self, delta: i64) {
        let Some((hits, i)) = self.nav.clone() else {
            return;
        };
        let ni = i as i64 + delta;
        if ni < 0 || ni >= hits.len() as i64 {
            return;
        }
        self.open_reader(hits[ni as usize].clone());
        self.nav = Some((hits, ni as usize));
    }

    pub fn open_reader_by_nreg(
        &mut self,
        nreg: &str,
        title: &str,
        status: i64,
        org_date: &str,
        org_num: &str,
        poddat: &str,
    ) {
        if self.reader_open_on(nreg) {
            self.page = Page::Reader;
            return;
        }
        let hit = rada_db::SearchHit {
            nreg: nreg.to_string(),
            nazva: title.to_string(),
            status,
            org_date: org_date.to_string(),
            org_num: org_num.to_string(),
            poddat: poddat.to_string(),
        };
        self.open_reader(hit);
    }

    fn fetch_card(&mut self, run: u64) {
        let client = self.client.clone();
        let nreg = self
            .reader
            .as_ref()
            .map(|r| r.hit.nreg.clone())
            .unwrap_or_default();
        self.spawn(move |tx| {
            let res = client.card(&nreg).map_err(|e| e.to_string());
            let _ = tx.send(Msg::Card { run, res });
        });
    }

    pub fn fetch_text(&mut self, run: u64, edition: Option<String>) {
        let client = self.client.clone();
        let nreg = self
            .reader
            .as_ref()
            .map(|r| r.hit.nreg.clone())
            .unwrap_or_default();
        if let Some(r) = self.reader.as_mut() {
            r.text_run = run;
            r.text_loading = true;
            r.cur_edition = edition.clone();
        }
        let db = self.db_path.clone();
        self.spawn(move |tx| {
            let key = edition.as_deref().unwrap_or("");
            let fetched = match &edition {
                Some(ed) => client.show_edition_text(&nreg, ed).map_err(|e| e.to_string()),
                None => client.show_text(&nreg).map_err(|e| e.to_string()),
            };
            // Success refreshes the offline cache; failure falls back to it.
            let res = match fetched {
                Ok(text) => {
                    if let Ok(conn) = rada_db::open(&db) {
                        let _ = rada_db::textcache::put(&conn, &nreg, key, &text);
                    }
                    Ok((text, false))
                }
                Err(e) => {
                    let hit = rada_db::open(&db)
                        .ok()
                        .and_then(|conn| {
                            rada_db::textcache::get(&conn, &nreg, key).ok().flatten()
                        });
                    match hit {
                        Some(text) => Ok((text, true)),
                        None => Err(e),
                    }
                }
            };
            let _ = tx.send(Msg::Text { run, res });
        });
    }

    /// F5 in the reader: re-fetch the current text (live request; success
    /// refreshes the offline cache, offline the cache answers instead).
    pub fn reload_reader(&mut self) {
        let Some(r) = self.reader.as_ref() else {
            return;
        };
        if r.text_loading {
            return;
        }
        let ed = r.cur_edition.clone();
        let run = self.next_run();
        self.fetch_text(run, ed);
    }

    /// Fetch «Пов'язані документи» for the open document (one paced request;
    /// parsing is the tested `rada_core::related::extract_related`).
    pub fn load_related(&mut self) {
        let Some(r) = self.reader.as_ref() else { return };
        if r.related_loading || r.related_loaded {
            return;
        }
        self.fetch_related();
    }

    /// Force-refetch the related list after a failure («оновити»).
    pub fn retry_related(&mut self) {
        if let Some(r) = self.reader.as_mut() {
            r.related_loaded = false;
            r.related_error = None;
        }
        self.fetch_related();
    }

    fn fetch_related(&mut self) {
        let Some(r) = self.reader.as_ref() else { return };
        if r.related_loading {
            return;
        }
        let nreg = r.hit.nreg.clone();
        let run = self.next_run();
        let client = self.client.clone();
        if let Some(r) = self.reader.as_mut() {
            r.related_run = run;
            r.related_loading = true;
            r.related_error = None;
        }
        self.spawn(move |tx| {
            let res = client.related(&nreg).map_err(|e| e.to_string());
            let _ = tx.send(Msg::Related { run, nreg, res });
        });
    }

    /// Switch the reader to a specific edition (or the current one when
    /// `edition` is `None`).
    pub fn switch_edition(&mut self, edition: Option<String>) {
        let Some(r) = self.reader.as_ref() else { return };
        if r.cur_edition == edition {
            return;
        }
        let card_run = r.card_run;
        let run = self.next_run();
        self.fetch_text(run, edition);
        // card_run unchanged: an in-flight card result stays valid.
        let _ = card_run;
    }

    /// Fetch both picked edition texts and render the compare report
    /// (two paced requests; the offline cache answers when live fails).
    pub fn compare_editions(&mut self) {
        let Some(r) = self.reader.as_ref() else { return };
        if r.cmp_loading {
            return;
        }
        let nreg = r.hit.nreg.clone();
        let (a, b) = (r.cmp_a.clone(), r.cmp_b.clone());
        let run = self.next_run();
        {
            let r = self.reader.as_mut().unwrap();
            r.cmp_run = run;
            r.cmp_loading = true;
            r.cmp_error = None;
            r.cmp_lines.clear();
        }
        let client = self.client.clone();
        let db = self.db_path.clone();
        self.spawn(move |tx| {
            let fetch_one = |edition: &Option<String>| -> Result<String, String> {
                let key = edition.as_deref().unwrap_or("");
                let fetched = match edition {
                    Some(ed) => client.show_edition_text(&nreg, ed).map_err(|e| e.to_string()),
                    None => client.show_text(&nreg).map_err(|e| e.to_string()),
                };
                match fetched {
                    Ok(text) => {
                        if let Ok(conn) = rada_db::open(&db) {
                            let _ = rada_db::textcache::put(&conn, &nreg, key, &text);
                        }
                        Ok(text)
                    }
                    Err(e) => rada_db::open(&db)
                        .ok()
                        .and_then(|conn| rada_db::textcache::get(&conn, &nreg, key).ok().flatten())
                        .ok_or(e),
                }
            };
            let res = fetch_one(&a).and_then(|ta| fetch_one(&b).map(|tb| (ta, tb)));
            let _ = tx.send(Msg::Compare { run, res });
        });
    }

    pub fn load_annotations(&mut self) {
        let db = self.db_path.clone();
        let nreg = self
            .reader
            .as_ref()
            .map(|r| r.hit.nreg.clone())
            .unwrap_or_default();
        self.spawn(move |tx| {
            let res = (|| -> Outcome<_> {
                let conn = rada_db::open(&db).map_err(|e| e.to_string())?;
                rada_db::annotations::list(&conn, &nreg).map_err(|e| e.to_string())
            })();
            let _ = tx.send(Msg::Annotations { nreg, res });
        });
    }

    pub fn save_annotation(&mut self, kind: &str, text: &str) {
        let Some(r) = self.reader.as_ref() else { return };
        let nreg = r.hit.nreg.clone();
        let span = r.selected_span();
        let t = text.trim().to_string();
        if kind != "bookmark" && t.is_empty() {
            self.toast("коментар/виділення потребують тексту", err());
            return;
        }
        let kind = kind.to_string();
        let span_start = span.as_ref().map(|s| s.0);
        let span_end = span.as_ref().map(|s| s.1);
        let db = self.db_path.clone();
        self.spawn(move |tx| {
            let res = (|| -> Outcome<()> {
                let mut conn = rada_db::open(&db).map_err(|e| e.to_string())?;
                rada_db::annotations::insert(
                    &mut conn,
                    &nreg,
                    &kind,
                    &t,
                    span_start,
                    span_end,
                )
                .map_err(|e| e.to_string())?;
                Ok(())
            })();
            let _ = tx.send(Msg::AnnotationSaved(kind, res));
        });
    }

    pub fn delete_annotation(&mut self, id: i64) {
        let db = self.db_path.clone();
        self.spawn(move |tx| {
            let res = (|| -> Outcome<bool> {
                let conn = rada_db::open(&db).map_err(|e| e.to_string())?;
                rada_db::annotations::remove(&conn, id).map_err(|e| e.to_string())
            })();
            let _ = tx.send(Msg::AnnotationDeleted(res));
        });
    }

    /// Rebuild the favorites list (all annotations + titles, newest first).
    pub fn load_favorites(&mut self) {
        self.fav.loading = true;
        self.fav.loaded = true;
        let db = self.db_path.clone();
        self.spawn(move |tx| {
            let _ = tx.send(Msg::Favorites(favorites_snapshot(&db, None)));
        });
    }

    /// Delete an annotation and re-list favorites from the *same* worker —
    /// a separate reload would race the delete.
    pub fn delete_annotation_fresh(&mut self, id: i64) {
        let db = self.db_path.clone();
        self.spawn(move |tx| {
            let _ = tx.send(Msg::Favorites(favorites_snapshot(&db, Some(id))));
        });
    }

    /// Replace the text of an existing annotation in place, then re-list it.
    pub fn edit_annotation(&mut self, id: i64, text: String) {
        let Some(r) = self.reader.as_ref() else { return };
        let nreg = r.hit.nreg.clone();
        let text = text.trim().to_string();
        if text.is_empty() {
            self.toast("текст анотації порожній", err());
            return;
        }
        let db = self.db_path.clone();
        self.spawn(move |tx| {
            let out = (|| -> Outcome<Vec<rada_db::Annotation>> {
                let conn = rada_db::open(&db).map_err(|e| e.to_string())?;
                if !rada_db::annotations::set_text(&conn, id, &text).map_err(|e| e.to_string())? {
                    return Err("анотацію не знайдено".to_string());
                }
                rada_db::annotations::list(&conn, &nreg).map_err(|e| e.to_string())
            })();
            let _ = tx.send(Msg::Annotations { nreg, res: out });
        });
    }

    /// Re-fetch the live card for the open document.
    pub fn retry_card(&mut self) {
        let run = self.next_run();
        if let Some(r) = self.reader.as_mut() {
            r.card_run = run;
            r.card_loading = true;
            r.card_error = None;
        }
        self.fetch_card(run);
    }

    /// Re-fetch the text for the open document (current edition).
    pub fn retry_text(&mut self) {
        let run = self.next_run();
        let ed = self.reader.as_ref().and_then(|r| r.cur_edition.clone());
        self.fetch_text(run, ed);
    }

    pub fn run_resolve(&mut self) {
        let text = self.search.resolve_input.trim().to_string();
        if text.is_empty() {
            return;
        }
        let run = self.next_run();
        let client = self.client.clone();
        self.search.resolve_run = run;
        self.search.resolve_loading = true;
        self.spawn(move |tx| {
            let res = client.resolve_name(&text);
            let _ = tx.send(Msg::Resolved { run, res });
        });
    }

    pub fn load_arrivals(&mut self) {
        let run = self.next_run();
        let client = self.client.clone();
        let db = self.db_path.clone();
        self.arrivals.run = run;
        self.arrivals.loading = true;
        self.spawn(move |tx| {
            let res = match client.updated_docs() {
                Ok(docs) => {
                    // Persist for offline viewing.
                    if let Ok(conn) = rada_db::open(&db) {
                        let _ = rada_db::sync::save_arrivals(&conn, &docs);
                    }
                    Ok((docs, false))
                }
                Err(e) => {
                    // Offline: fall back to the last saved snapshot.
                    let hit = rada_db::open(&db)
                        .ok()
                        .and_then(|conn| rada_db::sync::load_arrivals(&conn));
                    match hit {
                        Some((docs, _at)) => Ok((docs, true)),
                        None => Err(e.to_string()),
                    }
                }
            };
            let _ = tx.send(Msg::Arrivals { run, res });
        });
    }

    /// Incremental sync — same flow as the CLI `sync` command, with log lines.
    pub fn run_sync(&mut self) {
        if self.home.sync_running {
            return;
        }
        self.home.sync_running = true;
        self.home.sync_log.clear();
        let client = self.client.clone();
        let db = self.db_path.clone();
        self.spawn(move |tx| {
            let mut conn = match rada_db::open(&db) {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(Msg::SyncDone(Err(format!(
                        "не вдалося відкрити {}: {e}",
                        db.display()
                    ))));
                    return;
                }
            };
            let _ = tx.send(Msg::SyncLine("оновлення словників ...".into()));
            let dict_failed =
                match rada_db::sync::refresh_dicts(&mut conn, |kind| Ok(client.dictionary(kind)?))
                {
                    Ok(failed) => failed,
                    Err(e) => {
                        let _ = tx.send(Msg::SyncDone(Err(format!("{e:#}"))));
                        return;
                    }
                };
            if !dict_failed.is_empty() {
                let _ = tx.send(Msg::SyncLine(format!(
                    "словники: не завантажено {}",
                    dict_failed.join(", ")
                )));
            }
            let _ = tx.send(Msg::SyncLine("фід оновлень /laws/main/r.tsv ...".into()));
            let updates = match client.updated_docs() {
                Ok(u) => u,
                Err(e) => {
                    let _ = tx.send(Msg::SyncDone(Err(format!("{e}"))));
                    return;
                }
            };
            let _ = tx.send(Msg::SyncLine(format!(
                "{} документів у фіді", updates.len()
            )));
            let res = rada_db::sync::apply_updates(&mut conn, &updates, |nreg| {
                let j = client.card(nreg)?;
                Ok(rada_core::models::DocCard {
                    dokid: 0,
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
                })
            });
            match res {
                Ok((applied, failed)) => {
                    let _ = tx.send(Msg::SyncDone(Ok((applied, failed))));
                }
                Err(e) => {
                    let _ = tx.send(Msg::SyncDone(Err(format!("{e:#}"))));
                }
            }
        });
    }

    /// Download the published monthly snapshot and restore it to the db
    /// path. Existing user annotations are backed up first and re-attached.
    pub fn run_fetch_snapshot(&mut self) {
        if self.home.fetch_running {
            return;
        }
        self.home.fetch_running = true;
        self.home.fetch_prog = None;
        self.home.confirm_fetch = false;
        let db = self.db_path.clone();
        self.spawn(move |tx| {
            let keep = std::fs::metadata(&db)
                .ok()
                .and_then(|_| rada_db::open(&db).ok())
                .map(|conn| rada_db::annotations::all(&conn).unwrap_or_default())
                .unwrap_or_default();
            let n = keep.len();
            let _ = tx.send(Msg::FetchProgress(FetchProg::Resolve));

            let gh = rada_core::GitHub::default();
            let outcome = (|| -> anyhow::Result<PathBuf> {
                let url = gh.latest_snapshot_url("shevchukdp", "rada_client", None)?;
                let gz = db.with_extension("sqlite.gz.tmp");
                std::fs::remove_file(&gz).ok();
                let _ = tx.send(Msg::FetchProgress(FetchProg::Download { done: 0 }));
                gh.download_with_progress(&url, &gz, |done| {
                    let _ = tx.send(Msg::FetchProgress(FetchProg::Download { done }));
                })?;
                let _ = tx.send(Msg::FetchProgress(FetchProg::Restore));
                let restored = rada_db::snapshot::restore(&gz, &db)?;
                std::fs::remove_file(&gz).ok();
                if !keep.is_empty() {
                    let _ = tx.send(Msg::SyncLine(format!(
                        "повертаю {n} анотацій ..."
                    )));
                    let mut conn = rada_db::open(&restored)?;
                    for a in &keep {
                        let _ = rada_db::annotations::insert(
                            &mut conn,
                            &a.nreg,
                            &a.kind,
                            &a.text,
                            a.span_start,
                            a.span_end,
                        );
                    }
                }
                Ok(restored)
            })();
            let _ = tx.send(Msg::FetchDone(outcome.map_err(|e| format!("{e:#}"))));
        });
    }

    /// Save a gzip snapshot of the current index through a save dialog
    /// (same format as the published monthly snapshots).
    pub fn run_backup(&mut self) {
        if self.home.backup_busy {
            return;
        }
        let stamp = backup_stamp();
        let name = format!("rada-index-backup-{stamp}.sqlite.gz");
        let Some(path) = self.dlg_save(name) else {
            return;
        };
        self.home.backup_busy = true;
        let db = self.db_path.clone();
        self.spawn(move |tx| {
            let res = rada_db::snapshot::create(&db, &path)
                .map(|size| (path, size))
                .map_err(|e| format!("{e:#}"));
            let _ = tx.send(Msg::Backup(res));
        });
    }

    /// Replace the local index with a user-picked snapshot file
    /// (`.sqlite.gz`, e.g. one made by «Резервна копія»). Existing user
    /// annotations are backed up first and re-attached afterwards.
    pub fn run_restore_local(&mut self) {
        if self.home.backup_busy {
            return;
        }
        let Some(src) = self.dlg_open(&["gz", "sqlite"]) else {
            return;
        };
        self.home.backup_busy = true;
        self.home.confirm_restore = false;
        let db = self.db_path.clone();
        self.spawn(move |tx| {
            let keep = std::fs::metadata(&db)
                .ok()
                .and_then(|_| rada_db::open(&db).ok())
                .map(|conn| rada_db::annotations::all(&conn).unwrap_or_default())
                .unwrap_or_default();
            let n = keep.len();
            let outcome = (|| -> anyhow::Result<PathBuf> {
                let restored = rada_db::snapshot::restore(&src, &db)?;
                if !keep.is_empty() {
                    let _ = tx.send(Msg::SyncLine(format!("повертаю {n} анотацій ...")));
                    let mut conn = rada_db::open(&restored)?;
                    for a in &keep {
                        let _ = rada_db::annotations::insert(
                            &mut conn,
                            &a.nreg,
                            &a.kind,
                            &a.text,
                            a.span_start,
                            a.span_end,
                        );
                    }
                }
                Ok(restored)
            })();
            let _ = tx.send(Msg::LocalRestore(outcome.map_err(|e| format!("{e:#}"))));
        });
    }

    /// Build the full index from the official `doc.txt` dump: download it
    /// through the shared paced client (or pick a local dump file) and upsert
    /// every card. Annotations are untouched (separate table, keyed by nreg).
    pub fn run_init(&mut self, from_file: bool) {
        if self.home.init_running {
            return;
        }
        let dump_file = if from_file {
            let Some(p) = self.dlg_open(&["txt", "tsv", "dump"]) else {
                return;
            };
            Some(p)
        } else {
            None
        };
        self.home.init_running = true;
        self.home.confirm_init = false;
        let db = self.db_path.clone();
        let client = self.client.clone();
        self.spawn(move |tx| {
            let raw = match &dump_file {
                Some(p) => {
                    let _ = tx.send(Msg::SyncLine(format!(
                        "init: читаю дамп {} ...",
                        p.display()
                    )));
                    match std::fs::read(p) {
                        Ok(b) => b,
                        Err(e) => {
                            let _ = tx.send(Msg::InitDone(Err(format!("читання дампу: {e}"))));
                            return;
                        }
                    }
                }
                None => {
                    let _ = tx.send(Msg::SyncLine(
                        "init: завантажую офіційний doc.txt (~51 MB) ...".into(),
                    ));
                    match client.full_dump() {
                        Ok(b) => b,
                        Err(e) => {
                            let _ = tx.send(Msg::InitDone(Err(format!("завантаження: {e}"))));
                            return;
                        }
                    }
                }
            };
            let _ = tx.send(Msg::SyncLine(format!(
                "init: імпортую {:.1} MB ...",
                raw.len() as f64 / 1_048_576.0
            )));
            let outcome = (|| -> anyhow::Result<usize> {
                let mut conn = rada_db::open(&db)?;
                let n = rada_db::build::import_dump(&mut conn, &raw)?;
                let _ = conn.execute_batch("ANALYZE;");
                Ok(n)
            })();
            let _ = tx.send(Msg::InitDone(outcome.map_err(|e| format!("{e:#}"))));
        });
    }

    /// Save all annotations as a portable JSON file through a save dialog.
    pub fn export_annotations(&mut self) {
        let default_name = format!("rada-annotations-{}.json", backup_stamp());
        let Some(path) = self.dlg_save(default_name) else {
            return;
        };
        let db = self.db_path.clone();
        self.spawn(move |tx| {
            let res = (|| -> anyhow::Result<PathBuf> {
                let conn = rada_db::open(&db)?;
                let json = rada_db::annotations::export_json(&conn)?;
                std::fs::write(&path, json)?;
                Ok(path)
            })();
            let _ = tx.send(Msg::Exported(res.map_err(|e| format!("{e:#}"))));
        });
    }

    /// Export the CURRENTLY shown Favorites rows (filter + sort applied,
    /// web «експорт .txt» parity) as a UTF-8 .txt through a save dialog.
    pub fn export_favorites_txt(
        &mut self,
        rows: Vec<views::favorites::FavRow>,
        total: usize,
        needle: String,
        kind: usize,
    ) {
        let body = views::favorites::favorites_txt_report(&rows, total, &needle, kind);
        let Some(path) = self.dlg_save("annots.txt".to_string()) else {
            return;
        };
        self.spawn(move |tx| {
            let res = std::fs::write(&path, body.as_bytes())
                .map(|_| path)
                .map_err(|e| e.to_string());
            let _ = tx.send(Msg::Exported(res));
        });
    }

    /// Export the CURRENTLY shown Favorites rows (filter + sort applied,
    /// the web `favoritesCsv` twin) as a CSV file (`nreg,title,kind,
    /// created_at,text`, UTF-8 with BOM — the arrivals export's own format).
    pub fn export_favorites_csv(
        &mut self,
        rows: Vec<views::favorites::FavRow>,
        needle: String,
        kind: usize,
    ) {
        let body = views::favorites::favorites_csv(&rows, &needle, kind);
        if body.ends_with('\n') && body.lines().count() == 1 {
            self.toast("має бути хоча б одна анотація", err());
            return;
        }
        let Some(path) = self.dlg_save("favorites.csv".to_string()) else {
            return;
        };
        self.spawn(move |tx| {
            let res = std::fs::write(&path, body.as_bytes())
                .map(|_| path)
                .map_err(|e| e.to_string());
            let _ = tx.send(Msg::Exported(res));
        });
    }

    /// Import annotations from a JSON file (array or `{"annotations":[...]}`)
    /// picked through a file dialog. Existing rows are kept; valid entries are appended.
    pub fn import_annotations(&mut self) {
        let Some(path) = self.dlg_open(&["json"]) else {
            return;
        };
        let db = self.db_path.clone();
        self.spawn(move |tx| {
            let res = (|| -> anyhow::Result<usize> {
                let raw = std::fs::read_to_string(&path)?;
                let conn = rada_db::open(&db)?;
                rada_db::annotations::import_json(&conn, &raw)
            })();
            let _ = tx.send(Msg::AnnImported(res.map_err(|e| format!("{e:#}"))));
        });
    }

    /// Export the currently open document (md/txt/html) through a save dialog.
    /// Cached-text context snippet for `nreg` (favorites preview); `None`
    /// when the document text was never fetched into the cache.
    pub fn doc_preview(&self, nreg: &str, max_chars: usize) -> Option<String> {
        let conn = rada_db::open(&self.db_path).ok()?;
        rada_db::textcache::preview(&conn, nreg, "", max_chars)
    }

    /// Change the live-request pause at runtime (shared client + persisted).
    pub fn set_pace_secs(&mut self, secs: u64) {
        let secs = secs.clamp(1, 60);
        self.pace_secs = secs;
        self.client.set_pace(secs);
        self.settings.pace = secs;
        crate::settings::save(&self.settings);
    }

    /// Save dialog anchored at the last used folder (remembered on success).
    fn dlg_save(&mut self, name: String) -> Option<PathBuf> {
        let mut d = rfd::FileDialog::new().set_file_name(name);
        let dir = self.settings.export_dir.clone();
        if !dir.is_empty() && Path::new(&dir).is_dir() {
            d = d.set_directory(&dir);
        }
        let p = d.save_file();
        if let Some(p) = &p {
            if let Some(par) = p.parent() {
                self.settings.export_dir = par.display().to_string();
                crate::settings::save(&self.settings);
            }
        }
        p
    }

    /// Open-file dialog anchored at the last used folder.
    fn dlg_open(&mut self, exts: &[&str]) -> Option<PathBuf> {
        let mut d = rfd::FileDialog::new();
        if !exts.is_empty() {
            d = d.add_filter("файли", exts);
        }
        let dir = self.settings.export_dir.clone();
        if !dir.is_empty() && Path::new(&dir).is_dir() {
            d = d.set_directory(&dir);
        }
        let p = d.pick_file();
        if let Some(p) = &p {
            if let Some(par) = p.parent() {
                self.settings.export_dir = par.display().to_string();
                crate::settings::save(&self.settings);
            }
        }
        p
    }

    /// Folder picker anchored at the last used folder.
    fn dlg_folder(&mut self) -> Option<PathBuf> {
        let mut d = rfd::FileDialog::new();
        let dir = self.settings.export_dir.clone();
        if !dir.is_empty() && Path::new(&dir).is_dir() {
            d = d.set_directory(&dir);
        }
        let p = d.pick_folder();
        if let Some(p) = &p {
            self.settings.export_dir = p.display().to_string();
            crate::settings::save(&self.settings);
        }
        p
    }

    /// Print the current reader document: write its structured HTML to a
    /// temp file and open it in the system browser (user prints with
    /// Ctrl+P there — egui has no printer API).
    pub fn print_reader(&mut self, ctx: &egui::Context, plain: bool) {
        let Some(r) = self.reader.as_ref() else { return };
        let body = if plain {
            r.print_doc_plain()
        } else {
            r.export_doc("html")
        };
        let prefix = if plain { "rada-print-plain" } else { "rada-print" };
        let path =
            std::env::temp_dir().join(format!("{prefix}-{}.html", sanitize(&r.hit.nreg)));
        match std::fs::write(&path, &body) {
            Ok(_) => {
                let url = format!("file:///{}", path.display().to_string().replace('\\', "/"));
                ctx.open_url(egui::OpenUrl::new_tab(&url));
                self.toast("друковану версію відкрито в браузері", ok());
            }
            Err(e) => self.toast(format!("помилка друку: {e}"), err()),
        }
    }

    pub fn export_reader(&mut self, fmt: &str) {        let Some(r) = self.reader.as_ref() else { return };
        let body = r.export_doc(fmt);
        let default_name = format!("zakon-{}.{}", sanitize(&r.hit.nreg), fmt);
        let Some(path) = self.dlg_save(default_name) else {
            return;
        };
        self.spawn(move |tx| {
            let res = std::fs::write(&path, &body)
                .map(|_| path)
                .map_err(|e| e.to_string());
            let _ = tx.send(Msg::Exported(res));
        });
    }

    /// Export the open year drill-down window's rows as a CSV file
    /// (`date,nreg,nazva`, the arrivals CSV family).
    pub fn export_year_docs_csv(&mut self) {
        let Some(y) = self.home.year_docs.as_ref() else {
            return;
        };
        if y.rows.is_empty() {
            self.toast("має бути хоча б один документ", err());
            return;
        }
        let body = views::home::year_docs_csv(&y.rows);
        let name = format!("docs-{}.csv", y.yyyy);
        let Some(path) = self.dlg_save(name) else {
            return;
        };
        self.spawn(move |tx| {
            let res = std::fs::write(&path, &body)
                .map(|_| path)
                .map_err(|e| e.to_string());
            let _ = tx.send(Msg::Exported(res));
        });
    }

    /// Export the arrivals page (current filter applied) as a CSV file.
    pub fn export_arrivals_csv(&mut self, docs: Vec<rada_core::models::UpdatedDoc>) {
        if docs.is_empty() {
            self.toast("має бути хоча б один документ", err());
            return;
        }
        let body = views::arrivals::arrivals_csv(&docs);
        let Some(path) = self.dlg_save("arrivals.csv".to_string()) else {
            return;
        };
        self.spawn(move |tx| {
            let res = std::fs::write(&path, &body)
                .map(|_| path)
                .map_err(|e| e.to_string());
            let _ = tx.send(Msg::Exported(res));
        });
    }

    /// Export the compare-panel report (offline, from the already rendered
    /// lines) as a UTF-8 .txt through a save dialog.
    pub fn export_compare(
        &mut self,
        lines: &[String],
        nreg: &str,
        a: Option<&str>,
        b: Option<&str>,
    ) {
        if lines.is_empty() {
            self.toast("нічого експортувати — спочатку порівняйте", err());
            return;
        }
        let body = views::reader::cmp_export_body(lines, nreg, a, b);
        let default_name = format!("porivnyannya-{}.txt", sanitize(nreg));
        let Some(path) = self.dlg_save(default_name) else {
            return;
        };
        self.spawn(move |tx| {
            let res = std::fs::write(&path, body.as_bytes())
                .map(|_| path)
                .map_err(|e| e.to_string());
            let _ = tx.send(Msg::Exported(res));
        });
    }

    /// Export the current search results page (offline, from the index)
    /// as md/txt through a save dialog.
    pub fn export_search_list(&mut self, md: bool) {
        if self.search.hits.is_empty() {
            return;
        }
        let body = self.search.export_list(md);
        let ext = if md { "md" } else { "txt" };
        let q = self.search.query.trim();
        let name_q = if q.is_empty() { "all".to_string() } else { sanitize(q) };
        let default_name = format!("zakon-{name_q}-list.{ext}");
        let Some(path) = self.dlg_save(default_name) else {
            return;
        };
        self.spawn(move |tx| {
            let res = std::fs::write(&path, &body)
                .map(|_| path)
                .map_err(|e| e.to_string());
            let _ = tx.send(Msg::Exported(res));
        });
    }

    /// Export the CURRENT search results page as a CSV file (`nreg,nazva,
    /// status,org_num,org_date,poddat`, the arrivals export's own format).
    pub fn export_search_list_csv(&mut self) {
        if self.search.hits.is_empty() {
            return;
        }
        let body = views::search::search_rows_csv(&self.search.hits, &self.dicts.stan);
        let q = self.search.query.trim();
        let name_q = if q.is_empty() { "all".to_string() } else { sanitize(q) };
        let default_name = format!("zakon-{name_q}-list.csv");
        let Some(path) = self.dlg_save(default_name) else {
            return;
        };
        self.spawn(move |tx| {
            let res = std::fs::write(&path, &body)
                .map(|_| path)
                .map_err(|e| e.to_string());
            let _ = tx.send(Msg::Exported(res));
        });
    }

    /// Live export of the CHECKED results as individual numbered files
    /// (`NNN - number - title.ext`) into a chosen folder. Cache first, then
    /// paced live fetch (which refreshes the cache). Progress = LiveExport.
    pub fn export_selected_files(&mut self, md: bool) {
        if self.search.live_export.is_some() {
            return;
        }
        let sel: Vec<rada_db::SearchHit> = self
            .search
            .hits
            .iter()
            .filter(|h| self.search.selected.iter().any(|n| n == &h.nreg))
            .cloned()
            .collect();
        if sel.is_empty() {
            return;
        }
        let Some(dir) = self.dlg_folder() else {
            return;
        };
        let client = self.client.clone();
        let db = self.db_path.clone();
        let ext = if md { "md" } else { "txt" };
        self.search.live_export = Some((0, sel.len() as u64));
        self.spawn(move |tx| {
            let total = sel.len() as u64;
            let mut written = 0usize;
            let mut failed = 0usize;
            for (i, h) in sel.iter().enumerate() {
                let text = rada_db::open(&db)
                    .ok()
                    .and_then(|conn| rada_db::textcache::get(&conn, &h.nreg, "").ok().flatten())
                    .or_else(|| {
                        client.show_text(&h.nreg).ok().map(|t| {
                            if let Ok(conn) = rada_db::open(&db) {
                                let _ = rada_db::textcache::put(&conn, &h.nreg, "", &t);
                            }
                            t
                        })
                    });
                let num = if h.org_num.is_empty() { h.nreg.as_str() } else { h.org_num.as_str() };
                let title: String = h.nazva.chars().take(80).collect();
                let fname = format!("{:03} - {} - {}.{}", i + 1, sanitize(num), sanitize(&title), ext);
                let body = match &text {
                    Some(t) if md => format!(
                        "# {}\n\n> №{} ·рег. {} · від {} · ред. {}\n\n```\n{}\n```\n",
                        h.nazva,
                        h.org_num,
                        h.nreg,
                        crate::views::search::fmt_date(&h.org_date),
                        crate::views::search::fmt_date(&h.poddat),
                        t.trim_end()
                    ),
                    Some(t) => format!(
                        "{}\n№{} · рег. {} · від {} · ред. {}\n\n{}\n",
                        h.nazva,
                        h.org_num,
                        h.nreg,
                        crate::views::search::fmt_date(&h.org_date),
                        crate::views::search::fmt_date(&h.poddat),
                        t.trim_end()
                    ),
                    None if md => format!(
                        "# {}\n\n> №{} · текст отримати не вдалося (немає в кеші, live недоступний)\n",
                        h.nazva, h.nreg
                    ),
                    None => format!(
                        "{}\n№{} — текст отримати не вдалося (немає в кеші, live недоступний)\n",
                        h.nazva, h.nreg
                    ),
                };
                match std::fs::write(dir.join(&fname), &body) {
                    Ok(_) => written += 1,
                    Err(_) => failed += 1,
                }
                let _ = tx.send(Msg::LiveExport { done: (i + 1) as u64, total });
            }
            let _ = tx.send(Msg::SelectedExported { written, failed, dir });
        });
    }

    /// Live bulk export of the current results page: fetches each document's
    /// text (paced by the shared client, ~5 s/doc), appends saved annotations
    /// and writes md/txt. Progress arrives as `Msg::LiveExport`.
    pub fn export_search_list_live(&mut self, md: bool) {
        if self.search.live_export.is_some() || self.search.hits.is_empty() {
            return;
        }
        let hits = self.search.hits.clone();
        let ext = if md { "md" } else { "txt" };
        let q = self.search.query.trim().to_string();
        let name_q = if q.is_empty() {
            "all".to_string()
        } else {
            sanitize(&q)
        };
        let default_name = format!("zakon-{name_q}-full.{ext}");
        let Some(path) = self.dlg_save(default_name) else {
            return;
        };
        let client = self.client.clone();
        let db = self.db_path.clone();
        self.search.live_export = Some((0, hits.len() as u64));
        self.spawn(move |tx| {
            let total = hits.len() as u64;
            let mut body = String::new();
            if md {
                body.push_str(&format!(
                    "# Пошук: «{}» — повний текст, {} док.\n\n",
                    if q.is_empty() { "усі" } else { &q },
                    hits.len()
                ));
            } else {
                body.push_str(&format!(
                    "Пошук: «{}» — повний текст, {} док.\n\n\n",
                    if q.is_empty() { "усі" } else { &q },
                    hits.len()
                ));
            }
            for (i, h) in hits.iter().enumerate() {
                match client.show_text(&h.nreg) {
                    Ok(t) if md => {
                        body.push_str(&format!("## {} — №{}\n\n```\n{}\n```\n\n", h.nazva, h.nreg, t.trim_end()));
                    }
                    Ok(t) => {
                        body.push_str(&format!("{} — №{}\n\n{}\n\n\n", h.nazva, h.nreg, t.trim_end()));
                    }
                    Err(e) if md => {
                        body.push_str(&format!("## {} — №{}\n\n> текст отримати не вдалося: {e}\n\n", h.nazva, h.nreg));
                    }
                    Err(e) => {
                        body.push_str(&format!("{} — №{}\n(текст отримати не вдалося: {e})\n\n\n", h.nazva, h.nreg));
                    }
                }
                let _ = tx.send(Msg::LiveExport { done: (i + 1) as u64, total });
            }
            if let Ok(conn) = rada_db::open(&db) {
                let nregs: Vec<&str> = hits.iter().map(|h| h.nreg.as_str()).collect();
                if let Ok(anns) = rada_db::annotations::all(&conn) {
                    let mut lines = String::new();
                    for a in anns.iter().filter(|a| nregs.contains(&a.nreg.as_str())).rev() {
                        lines.push_str(&format!(
                            "{}  [{}]  {}\n{}\n\n",
                            a.nreg,
                            a.kind,
                            a.created_at,
                            a.text.trim()
                        ));
                    }
                    if !lines.is_empty() {
                        body.push_str(if md { "\n## Примітки та виділення\n\n" } else { "\nПримітки та виділення\n\n" });
                        body.push_str(&lines);
                    }
                }
            }
            let res = std::fs::write(&path, &body)
                .map(|_| path)
                .map_err(|e| e.to_string());
            let _ = tx.send(Msg::Exported(res));
        });
    }

    // -----------------------------------------------------------------
    // frame
    // -----------------------------------------------------------------

    fn handle_messages(&mut self) {
        for _ in 0..256 {
            match self.rx.try_recv() {
                Ok(msg) => self.apply(msg),
                Err(_) => break,
            }
        }
    }

    fn apply(&mut self, msg: Msg) {
        match msg {
            Msg::CacheStats(pair) => {
                self.cache_stats = Some(pair);
            }
            Msg::YearDocs { run, yyyy, res } => {
                // Route to the open drill-down only while it still shows the
                // same year and the same request (stale runs are dropped).
                if let Some(y) = self.home.year_docs.as_mut() {
                    if y.yyyy != yyyy || run != y.run {
                        return;
                    }
                    y.loading = false;
                    match res {
                        Ok(rows) => {
                            y.rows = rows;
                            y.error = None;
                        }
                        Err(e) => y.error = Some(e),
                    }
                }
            }
            Msg::CachePurged(res) => match res {
                Ok(n) => {
                    self.cache_stats = Some((0, 0));
                    self.toast(format!("кеш очищено: видалено {n} тіл"), ok());
                }
                Err(e) => self.toast(format!("не вдалося очистити кеш: {e}"), err()),
            },
            Msg::Status(st) => {
                let dicts_empty = st.dicts.iter().all(|(_, c)| *c == 0);
                let auto_fetch = st.docs > 0 && dicts_empty;
                self.index_status = Some(st);
                if auto_fetch {
                    self.toast(
                        "словники порожні — завантажую наживо (3 запити, ~15 с)",
                        gold(),
                    );
                    self.fetch_dicts_live();
                }
            }
            Msg::Dicts { stan, typs, organs } => {
                self.dicts = views::Dicts { stan, typs, organs };
            }
            Msg::DictsFetched(res) => match res {
                Ok(failed) => {
                    if failed.is_empty() {
                        self.toast("словники завантажено", ok());
                    } else {
                        self.toast(
                            format!("словники: не завантажено {}", failed.join(", ")),
                            err(),
                        );
                    }
                    self.refresh_dicts();
                    self.refresh_index_state();
                }
                Err(e) => {
                    self.toast(format!("словники: {e}"), err());
                }
            }
            Msg::OpenNreg(hit) => {
                self.search.loading = false;
                self.toast(
                    format!("документ відкрито за номером {}", hit.nreg),
                    ok(),
                );
                let keep = self.search.hits.clone();
                self.open_reader(hit.clone());
                // Keep the results list reachable with Alt+←/→ semantics
                // only when the opened doc is part of it.
                if keep.iter().any(|h| h.nreg == hit.nreg) {
                    let idx = keep.iter().position(|h| h.nreg == hit.nreg).unwrap();
                    self.nav = Some((keep, idx));
                }
            }
            Msg::Search { run, res, cached } => {
                if run == self.search.run {
                    self.search.loading = false;
                    match res {
                        Ok((hits, total)) => {
                            self.search.total = total;
                            self.search.hits = hits;
                            self.search.cached = cached;
                            self.search.selected.clear();
                            self.search.error = None;
                        }
                        Err(e) => {
                            self.search.hits.clear();
                            self.search.cached.clear();
                            self.search.error = Some(e);
                        }
                    }
                }
            }
            Msg::Facets { run, res } => {
                if run == self.search.facets_run {
                    self.search.facets_loading = false;
                    match res {
                        Ok(fc) => self.search.facets = Some(fc),
                        // Facets are advisory: on failure show nothing and
                        // leave the hit list untouched.
                        Err(_) => self.search.facets = None,
                    }
                }
            }
            Msg::CacheSearch { run, res } => {
                if run == self.search.cache_run {
                    self.search.cache_loading = false;
                    self.search.cache_done = true;
                    match res {
                        Ok(rows) => {
                            self.search.cache_hits = rows;
                            self.search.cache_error = None;
                        }
                        Err(e) => {
                            self.search.cache_hits.clear();
                            self.search.cache_error = Some(e);
                        }
                    }
                }
            }
            Msg::Card { run, res } => {
                let Some(r) = self.reader.as_mut() else { return };
                if run != r.card_run {
                    return;
                }
                r.card_loading = false;
                let mut sync_title: Option<(String, String)> = None;
                match res {
                    Ok(card) => {
                        sync_title = Some((r.hit.nreg.clone(), card.nazva.clone()));
                        r.editions = card.eds.clone();
                        r.card = Some(card);
                        r.card_error = None;
                    }
                    Err(e) => r.card_error = Some(e),
                }
                // Give the tab strip the real title when it only holds the
                // CLI placeholder or nothing at all.
                if let Some((nreg, title)) = sync_title {
                    if let Some(i) = self.tabs.iter().position(|t| t.nreg == nreg) {
                        let cur = self.tabs[i].title.clone();
                        if cur.trim().is_empty() || cur == "(відкрито з рядка команди)" {
                            self.tabs[i].title = title;
                        }
                    }
                }
            }
            Msg::Text { run, res } => {
                let Some(r) = self.reader.as_mut() else { return };
                if run != r.text_run {
                    return;
                }
                r.text_loading = false;
                match res {
                    Ok((t, cached)) => {
                        r.text = Some(t);
                        r.text_cached = cached;
                        r.text_error = None;
                    }
                    Err(e) => {
                        r.text = None;
                        r.text_cached = false;
                        r.text_error = Some(e);
                    }
                }
            }
            Msg::Compare { run, res } => {
                let Some(r) = self.reader.as_mut() else { return };
                if run != r.cmp_run {
                    return;
                }
                r.cmp_loading = false;
                match res {
                    Ok((ta, tb)) => {
                        let la = crate::views::reader::ed_label(&r.cmp_a);
                        let lb = crate::views::reader::ed_label(&r.cmp_b);
                        r.cmp_lines = crate::views::reader::cmp_report(&la, &ta, &lb, &tb);
                        r.cmp_error = None;
                    }
                    Err(e) => {
                        r.cmp_lines.clear();
                        r.cmp_error = Some(e);
                    }
                }
            }
            Msg::Related { run, nreg, res } => {
                let Some(r) = self.reader.as_mut() else { return };
                if run != r.related_run || r.hit.nreg != nreg {
                    return;
                }
                r.related_loading = false;
                r.related_loaded = true;
                match res {
                    Ok(list) => {
                        // Prefer the offline index title over the page's anchor text.
                        let list = match rada_db::open(&self.db_path) {
                            Ok(conn) => list
                                .into_iter()
                                .map(|mut d| {
                                    if let Some(t) = rada_db::title_by_nreg(&conn, &d.nreg) {
                                        if !t.trim().is_empty() {
                                            d.title = t;
                                        }
                                    }
                                    d
                                })
                                .collect(),
                            Err(_) => list,
                        };
                        r.related = list;
                        r.related_error = None;
                    }
                    Err(e) => {
                        r.related.clear();
                        r.related_error = Some(e);
                    }
                }
            }
            Msg::Annotations { nreg, res } => {
                let Some(r) = self.reader.as_mut() else { return };
                if r.hit.nreg != nreg {
                    return;
                }
                match res {
                    Ok(list) => {
                        r.annotations = list;
                        r.ann_error = None;
                    }
                    Err(e) => r.ann_error = Some(e),
                }
            }
            Msg::AnnotationSaved(kind, res) => match res {
                Ok(()) => {
                    if kind == "highlight" {
                        self.toast("додано виділення", ok());
                    } else {
                        self.toast("анотацію збережено", ok());
                    }
                    if let Some(r) = self.reader.as_mut() {
                        r.ann_text.clear();
                    }
                    self.load_annotations();
                }
                Err(e) => self.toast(format!("помилка: {e}"), err()),
            },
            Msg::AnnotationDeleted(res) => match res {
                Ok(true) => {
                    self.toast("анотацію видалено", ok());
                    self.load_annotations();
                }
                Ok(false) => self.toast("анотацію не знайдено", err()),
                Err(e) => self.toast(format!("помилка: {e}"), err()),
            },
            Msg::LiveExport { done, total } => {
                self.search.live_export = Some((done, total));
            }
            Msg::Favorites(res) => {
                self.fav.loading = false;
                match res {
                    Ok(items) => {
                        self.fav.items = items;
                        self.fav.error = None;
                    }
                    Err(e) => self.fav.error = Some(e),
                }
            }
            Msg::AnnImported(res) => {
                self.fav.loaded = false; // force reload on next visit
                match res {
                    Ok(n) => {
                        self.toast(format!("імпортовано {n} анотацій"), ok());
                        self.load_favorites();
                    }
                    Err(e) => self.toast(format!("імпорт: {e}"), err()),
                }
            }
            Msg::Arrivals { run, res } => {
                if run == self.arrivals.run {
                    self.arrivals.loading = false;
                    match res {
                        Ok((docs, from_cache)) => {
                            // Diff against the persisted seen-set: docs that
                            // were not in it get the «НОВЕ» badge, then the
                            // whole feed becomes seen (persisted).
                            let seen = self.settings.arr_seen.clone();
                            self.arrivals.new_nregs =
                                views::arrivals::unseen_docs(&docs, &seen);
                            if !from_cache {
                                let nregs: Vec<String> =
                                    docs.iter().map(|d| d.nreg.clone()).collect();
                                crate::settings::mark_arrivals_seen(&mut self.settings, nregs);
                                crate::settings::save(&self.settings);
                            }
                            self.arrivals.docs = docs;
                            self.arrivals.from_cache = from_cache;
                            self.arrivals.saved_at = if from_cache {
                                rada_db::open(&self.db_path)
                                    .ok()
                                    .and_then(|c| {
                                        rada_db::sync::load_arrivals(&c).map(|(_, at)| at)
                                    })
                            } else {
                                None
                            };
                            self.arrivals.fetched_at = Some(
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_secs())
                                    .unwrap_or(0),
                            );
                            self.arrivals.error = None;
                        }
                        Err(e) => {
                            self.arrivals.docs.clear();
                            self.arrivals.from_cache = false;
                            self.arrivals.saved_at = None;
                            self.arrivals.fetched_at = None;
                            self.arrivals.error = Some(e);
                        }
                    }
                }
            }
            Msg::SyncLine(line) => {
                let line = sync_log_line(now_secs(), &line);
                self.home.sync_log.push(line);
                if self.home.sync_log.len() > 40 {
                    self.home.sync_log.remove(0);
                }
            }
            Msg::SyncDone(res) => {
                self.home.sync_running = false;
                match res {
                    Ok((applied, failed)) => {
                        self.home
                            .sync_log
                            .push(sync_log_line(now_secs(), &format!("застосовано {applied}, невдалих {failed}")));
                        self.toast(format!("sync: +{applied} (невдалих {failed})"), ok());
                        self.refresh_index_state();
                        self.refresh_dicts();
                    }
                    Err(e) => {
                        self.home.sync_log.push(sync_log_line(now_secs(), &format!("помилка: {e}")));
                        self.toast(format!("sync: {e}"), err());
                    }
                }
            }
            Msg::FetchProgress(prog) => self.home.fetch_prog = Some(prog),
            Msg::FetchDone(res) => {
                self.home.fetch_running = false;
                self.home.fetch_prog = None;
                match res {
                    Ok(path) => {
                        self.toast(format!("індекс відновлено: {}", path.display()), ok());
                        self.refresh_index_state();
                        self.refresh_dicts();
                    }
                    Err(e) => self.toast(format!("fetch: {e}"), err()),
                }
            }
            Msg::Backup(res) => {
                self.home.backup_busy = false;
                match res {
                    Ok((path, size)) => {
                        let mb = size as f64 / 1_048_576.0;
                        self.toast(
                            format!("резервна копія: {} ({mb:.1} MB)", path.display()),
                            ok(),
                        );
                    }
                    Err(e) => self.toast(format!("резервна копія: {e}"), err()),
                }
            }
            Msg::LocalRestore(res) => {
                self.home.backup_busy = false;
                match res {
                    Ok(path) => {
                        self.toast(
                            format!("індекс відновлено з файлу: {}", path.display()),
                            ok(),
                        );
                        self.refresh_index_state();
                        self.refresh_dicts();
                    }
                    Err(e) => self.toast(format!("відновлення: {e}"), err()),
                }
            }
            Msg::InitDone(res) => {
                self.home.init_running = false;
                match res {
                    Ok(n) => {
                        self.home
                            .sync_log
                            .push(sync_log_line(now_secs(), &format!("init: імпортовано {n} документів")));
                        self.toast(format!("індекс побудовано: {n} док."), ok());
                        self.refresh_index_state();
                        self.refresh_dicts();
                    }
                    Err(e) => {
                        self.home.sync_log.push(sync_log_line(now_secs(), &format!("init: помилка {e}")));
                        self.toast(format!("init: {e}"), err());
                    }
                }
            }
            Msg::Resolved { run, res } => {
                if run == self.search.resolve_run {
                    self.search.resolve_loading = false;
                    self.search.resolve_result = res;
                }
            }
            Msg::Exported(res) => {
                self.search.live_export = None;
                match res {
                    Ok(path) => self.toast(format!("збережено: {}", path.display()), ok()),
                    Err(e) => self.toast(format!("помилка збереження: {e}"), err()),
                }
            }
            Msg::SelectedExported { written, failed, dir } => {
                self.search.live_export = None;
                let msg = if failed == 0 {
                    format!("файлів записано: {written} → {}", dir.display())
                } else {
                    format!("файлів: {written}, помилок: {failed} → {}", dir.display())
                };
                self.toast(msg, if failed == 0 { ok() } else { err() });
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_messages();
        self.refresh_theme(ctx);

        // ---- global keyboard shortcuts ---------------------------------
        // Ctrl+F: focus the find field (reader), the query field (search),
        // the arrivals filter or the favorites filter.
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::F)) {
            self.focus_req = match self.page {
                Page::Reader => Some(FocusReq::ReaderFind),
                Page::Search => Some(FocusReq::SearchQuery),
                Page::Arrivals => Some(FocusReq::ArrivalsFilter),
                Page::Favorites => Some(FocusReq::FavoritesFilter),
                _ => None,
            };
        }
        // Ctrl+K: jump to «Пошук» and focus the query field (web-style).
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::K)) {
            self.page = Page::Search;
            self.focus_req = Some(FocusReq::SearchQuery);
        }
        // F5 on the reader page: re-fetch the current text (cache refreshes
        // on success; offline the cache answers).
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F5))
            && self.page == Page::Reader
        {
            self.reload_reader();
        }
        // Ctrl+Enter on the arrivals page: open the top visible arrival.
        if self.page == Page::Arrivals
            && ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter)
                    || i.consume_key(egui::Modifiers::ALT, egui::Key::Enter)
            })
        {
            let f = self.arrivals.filter.clone();
            let fresh = self.arrivals.new_nregs.clone();
            let only_new = self.arrivals.only_new;
            if let Some(d) =
                crate::views::arrivals::visible_arrivals(&self.arrivals.docs, &f, &fresh, only_new)
                    .into_iter()
                    .next()
            {
                self.open_reader_by_nreg(&d.nreg, &d.nazva, 0, "", "", "");
            }
        }
        // Ctrl+Enter on the search page: open the top hit in the reader.
        if self.page == Page::Search
            && ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter)
                    || i.consume_key(egui::Modifiers::ALT, egui::Key::Enter)
            })
        {
            if let Some(idx) =
                crate::views::search::top_hit_index(&self.search.hits, self.search.searched)
            {
                let hits = self.search.hits.clone();
                self.open_reader_from(hits, idx);
            }
        }
        // Ctrl+M on the reader page: toggle форматований/простий text mode.
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::M))
            && self.page == Page::Reader
        {
            let (new_raw, label) = crate::views::reader::toggle_raw_mode(self.settings.raw);
            self.settings.raw = new_raw;
            crate::settings::save(&self.settings);
            self.toast(label, ok());
        }
        // Ctrl+B on the reader page: save a quick bookmark of the open act.
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::B))
            && self.page == Page::Reader
            && self.reader.is_some()
        {
            self.save_annotation("bookmark", "");
        }
        // Alt+Up / Alt+Down on the reader page: cycle the edition combo
        // (поточна ↔ dated editions, wrap-around; one paced text request).
        let ed_step = ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowDown) {
                Some(1i32)
            } else if i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowUp) {
                Some(-1i32)
            } else {
                None
            }
        });
        if let Some(step) = ed_step {
            if self.page == Page::Reader {
                let next = self.reader.as_ref().and_then(|r| {
                    let eds: Vec<String> =
                        r.editions.iter().map(|e| e.datred.to_string()).collect();
                    crate::views::reader::cycle_edition(&eds, r.cur_edition.as_deref(), step)
                });
                if let Some(ed) = next {
                    self.switch_edition(ed);
                }
            }
        }
        // Alt+1..5: quick page switching (Читалка only with an open document).
        let alt_page = ctx.input_mut(|i| {
            for (k, d) in [
                (egui::Key::Num1, 1u8),
                (egui::Key::Num2, 2),
                (egui::Key::Num3, 3),
                (egui::Key::Num4, 4),
                (egui::Key::Num5, 5),
            ] {
                if i.consume_key(egui::Modifiers::ALT, k) {
                    return page_for_alt_digit(d);
                }
            }
            None
        });
        if let Some(p) = alt_page {
            if p != Page::Reader || self.reader.is_some() {
                self.page = p;
            }
        }
        // Alt+Left/Right or Alt+PageUp/PageDown: prev/next document inside
        // the reader. The keys are consumed so PageUp/Down never also
        // scrolls the central area.
        let alt_prev = ctx.input_mut(|i| {
            let a = i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowLeft);
            let b = i.consume_key(egui::Modifiers::ALT, egui::Key::PageUp);
            a || b
        });
        let alt_next = ctx.input_mut(|i| {
            let a = i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowRight);
            let b = i.consume_key(egui::Modifiers::ALT, egui::Key::PageDown);
            a || b
        });
        if self.page == Page::Reader && self.nav.is_some() {
            if alt_prev {
                self.nav_step(-1);
            }
            if alt_next {
                self.nav_step(1);
            }
        }
        // Ctrl+Plus / Ctrl+Minus / Ctrl+0: reader text zoom (persisted).
        let z_up = ctx.input_mut(|i| {
            let a = i.consume_key(egui::Modifiers::COMMAND, egui::Key::Equals);
            let b = i.consume_key(egui::Modifiers::COMMAND, egui::Key::Plus);
            a || b
        });
        let z_down = ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Minus));
        let z_reset = ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Num0));
        if z_up {
            self.zoom_reader(10);
        } else if z_down {
            self.zoom_reader(-10);
        } else if z_reset {
            self.zoom_reader(0);
        }
        // Ctrl+Tab / Ctrl+Shift+Tab: cycle the reader's document tabs
        // (wrap-around). The Shift variant is consumed first. Only consumed
        // when the reader actually has a strip — plain Tab stays usable for
        // focus navigation everywhere else.
        let tabs_cycling = self.page == Page::Reader && self.tabs.len() > 1;
        let tab_shift = ctx.input_mut(|i| {
            if tabs_cycling {
                i.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::Tab,
                )
            } else {
                false
            }
        });
        let tab_plain = ctx.input_mut(|i| {
            if tabs_cycling {
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::Tab)
            } else {
                false
            }
        });
        if tab_shift {
            self.tab_cycle(-1);
        } else if tab_plain {
            self.tab_cycle(1);
        }
        // Ctrl+W: close the active reader tab (reader page only).
        let ctrl_w = ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::W));
        if ctrl_w && self.page == Page::Reader {
            if let Some(a) = self.active_tab {
                self.tab_close(a);
            }
        }
        // Ctrl+Shift+T: reopen the most recently closed reader tab (works
        // from any page, like the browser idiom it copies).
        let ctrl_shift_t = ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::T)
        });
        if ctrl_shift_t {
            self.tab_reopen();
        }
        // Ctrl+Shift+S: start the incremental sync from any page, whenever
        // every home worker is idle (sync/fetch/init share the db file).
        let ctrl_shift_s = ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::S)
        });
        if ctrl_shift_s
            && sync_shortcut_allowed(
                self.home.sync_running,
                self.home.fetch_running,
                self.home.init_running,
            )
        {
            self.run_sync();
        }

        // toast lifetime
        self.toasts
            .retain(|t| t.born.elapsed() < Duration::from_secs(4));
        if !self.toasts.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }

        views::topbar(self, ctx);

        // A reader page with no open document falls back to the search page.
        if self.page == Page::Reader && self.reader.is_none() {
            self.page = Page::Search;
        }

        match self.page {
            Page::Home => views::home::show(self, ctx),
            Page::Search => views::search::show(self, ctx),
            Page::Reader => views::reader::show(self, ctx),
            Page::Arrivals => views::arrivals::show(self, ctx),
            Page::Favorites => views::favorites::show(self, ctx),
        }

        views::statusbar(self, ctx);
        views::toasts(self, ctx);
    }
}

pub(crate) fn ok() -> egui::Color32 {
    crate::themes::ok()
}

pub(crate) fn err() -> egui::Color32 {
    crate::themes::err()
}

pub(crate) fn gold() -> egui::Color32 {
    crate::themes::accent()
}

/// All annotations (newest first) paired with document titles; optionally
/// deletes `delete_id` in the same worker to avoid a reload race.
fn favorites_snapshot(
    db: &std::path::Path,
    delete_id: Option<i64>,
) -> Outcome<Vec<(rada_db::Annotation, String)>> {
    let conn = rada_db::open(db).map_err(|e| e.to_string())?;
    if let Some(id) = delete_id {
        rada_db::annotations::remove(&conn, id).map_err(|e| e.to_string())?;
    }
    let anns = rada_db::annotations::all(&conn).map_err(|e| e.to_string())?;
    let mut items: Vec<(rada_db::Annotation, String)> = anns
        .into_iter()
        .map(|a| {
            let t = rada_db::title_by_nreg(&conn, &a.nreg).unwrap_or_default();
            (a, t)
        })
        .collect();
    items.sort_by(|(a, _), (b, _)| b.id.cmp(&a.id));
    Ok(items)
}

fn read_index_status(db: &PathBuf) -> IndexStatus {
    let meta = match std::fs::metadata(db) {
        Err(_) => {
            return IndexStatus {
                docs: 0,
                last_import: None,
                max_date: None,
                size_bytes: 0,
                dicts: Vec::new(),
                fresh: Vec::new(),
                years: Vec::new(),
            }
        }
        Ok(m) => m,
    };
    let mut st = IndexStatus {
        docs: 0,
        last_import: None,
        max_date: None,
        size_bytes: meta.len(),
        dicts: Vec::new(),
        fresh: Vec::new(),
        years: Vec::new(),
    };
    if let Ok(conn) = rada_db::open(db) {
        st.docs = rada_db::doc_count(&conn).unwrap_or(0);
        st.last_import = rada_db::last_import(&conn);
        st.max_date = conn
            .query_row(
                "SELECT MAX(NULLIF(org_date,'')) FROM docs",
                [],
                |r| r.get::<_, Option<String>>(0),
            )
            .ok()
            .flatten();
        // Offline «fresh by adoption date» card rows for the home page
        // (same anchor rules as the CLI `recent` command: full 8-digit dates).
        st.fresh = conn
            .prepare(
                "SELECT org_date, nreg, nazva FROM docs \
                 WHERE org_date GLOB '[0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9]' \
                 ORDER BY org_date DESC, nreg ASC LIMIT 14",
            )
            .and_then(|mut ps| {
                let rows = ps.query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                })?;
                rows.collect::<Result<Vec<_>, _>>()
            })
            .unwrap_or_default();
        // Offline per-year counts for the home mini histogram (same GLOB
        // anchor as the CLI `years`; the 15-row cap lives in the view).
        st.years = conn
            .prepare(
                "SELECT substr(org_date,1,4) AS y, COUNT(*) FROM docs \
                 WHERE org_date GLOB '[0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9]' \
                 GROUP BY y ORDER BY y ASC",
            )
            .and_then(|mut ps| {
                let rows = ps.query_map([], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
                })?;
                rows.collect::<Result<Vec<_>, _>>()
            })
            .unwrap_or_default();
        for kind in ["stan", "typs", "organs"] {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM dict WHERE kind=?1",
                    [kind],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            st.dicts.push((kind.into(), count));
        }
    }
    st
}

fn apply_spacing(ctx: &egui::Context) {
    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 5.0);
    });
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect()
}

/// Unix seconds since the epoch (UTC), 0 when the clock is broken.
pub(crate) fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// «HH:MM» UTC clock for the action-journal line stamps.
fn hhmm_from_secs(secs: u64) -> String {
    let rem = secs % 86_400;
    format!("{:02}:{:02}", rem / 3600, (rem % 3600) / 60)
}

/// One «Журнал дій» line: `[HH:MM] text` — the single timestamp shape.
fn sync_log_line(now_secs: u64, text: &str) -> String {
    format!("[{}] {text}", hhmm_from_secs(now_secs))
}

/// Whether the global Ctrl+Shift+S may start a sync: every home worker
/// (sync/fetch/init) must be idle — they all write the same db file.
fn sync_shortcut_allowed(sync_running: bool, fetch_running: bool, init_running: bool) -> bool {
    !sync_running && !fetch_running && !init_running
}

/// «20261027-1530» stamp for backup file names (UTC, no external crates).
fn backup_stamp() -> String {
    stamp_from_secs(now_secs())
}

/// Unix seconds -> «YYYYMMDD-HHMM» (UTC).
fn stamp_from_secs(secs: u64) -> String {
    // Civil-days algorithm (Howard Hinnant) -> Y-M-D from days since epoch.
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}{m:02}{d:02}-{:02}{:02}", rem / 3600, (rem % 3600) / 60)
}

#[cfg(test)]
mod tests {
    use super::{
        closed_pop, closed_push, hhmm_from_secs, page_for_alt_digit, stamp_from_secs,
        sync_log_line, sync_shortcut_allowed, Page,
    };

    #[test]
    fn sync_journal_lines_are_utc_stamped() {
        // midnight, noon, and a partial minute — zero-padded HH:MM.
        assert_eq!(hhmm_from_secs(0), "00:00");
        assert_eq!(hhmm_from_secs(12 * 3600), "12:00");
        assert_eq!(hhmm_from_secs(9 * 3600 + 5 * 60 + 59), "09:05");
        // wraps within the day, ignores whole days
        assert_eq!(hhmm_from_secs(86_400 + 3_661), "01:01");
        // the shared line shape
        assert_eq!(sync_log_line(9 * 3600 + 5 * 60, "фід оновлень ..."), "[09:05] фід оновлень ...");
    }

    #[test]
    fn alt_digits_map_to_the_five_pages() {
        assert_eq!(page_for_alt_digit(1), Some(Page::Home));
        assert_eq!(page_for_alt_digit(2), Some(Page::Search));
        assert_eq!(page_for_alt_digit(3), Some(Page::Reader));
        assert_eq!(page_for_alt_digit(4), Some(Page::Arrivals));
        assert_eq!(page_for_alt_digit(5), Some(Page::Favorites));
        assert_eq!(page_for_alt_digit(0), None);
        assert_eq!(page_for_alt_digit(6), None);
        assert_eq!(page_for_alt_digit(9), None);
    }

    #[test]
    fn stamp_formats_utc_civil_date() {
        // 1970-01-01 00:00:00
        assert_eq!(stamp_from_secs(0), "19700101-0000");
        // 2000-02-29 12:34:56 (leap day)
        let secs = 951_827_696u64;
        assert_eq!(stamp_from_secs(secs), "20000229-1234");
        // 2026-10-27 00:00:00
        assert_eq!(stamp_from_secs(20753 * 86_400), "20261027-0000");
    }

    #[test]
    fn closed_tab_stack_pops_most_recent_first() {
        let mut s: Vec<(String, String)> = Vec::new();
        assert_eq!(closed_pop(&mut s), None, "empty stack reopens nothing");
        closed_push(&mut s, ("1-1".into(), "Перший".into()));
        closed_push(&mut s, ("2-2".into(), "Другий".into()));
        assert_eq!(closed_pop(&mut s), Some(("2-2".into(), "Другий".into())));
        assert_eq!(closed_pop(&mut s), Some(("1-1".into(), "Перший".into())));
        assert_eq!(closed_pop(&mut s), None);
    }

    #[test]
    fn closed_tab_stack_dedupes_and_caps_at_ten() {
        let mut s: Vec<(String, String)> = Vec::new();
        closed_push(&mut s, ("1-1".into(), "А".into()));
        closed_push(&mut s, ("2-2".into(), "Б".into()));
        closed_push(&mut s, ("1-1".into(), "А-новіше".into()));
        // the repeat moved to the top instead of doubling up
        assert_eq!(s.len(), 2);
        assert_eq!(s[1], ("1-1".into(), "А-новіше".into()));
        // 12 pushes leave the 10 newest, oldest dropped first
        for i in 1..=12 {
            closed_push(&mut s, (format!("n{i}"), format!("Т{i}")));
        }
        assert_eq!(s.len(), 10);
        assert_eq!(s[0], ("n3".into(), "Т3".into()));
        assert_eq!(s[9], ("n12".into(), "Т12".into()));
    }

    #[test]
    fn sync_shortcut_needs_every_worker_idle() {
        assert!(sync_shortcut_allowed(false, false, false));
        assert!(!sync_shortcut_allowed(true, false, false), "sync running");
        assert!(!sync_shortcut_allowed(false, true, false), "fetch running");
        assert!(!sync_shortcut_allowed(false, false, true), "init running");
        assert!(!sync_shortcut_allowed(true, true, true));
    }
}
