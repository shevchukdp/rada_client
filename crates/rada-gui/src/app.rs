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
    Dicts {
        stan: Vec<(i64, String)>,
        typs: Vec<(i64, String)>,
        organs: Vec<(i64, String)>,
    },
    Search {
        run: u64,
        res: Outcome<(Vec<rada_db::SearchHit>, i64)>,
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
}

/// Which app page is on screen.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Home,
    Search,
    Reader,
    Arrivals,
    Favorites,
}

/// Launch-time options (CLI `--start`, `--query`, `--nreg`).
#[derive(Default)]
pub struct Startup {
    /// `home` | `search` | `arrivals`
    pub start_page: Option<String>,
    /// Pre-fill and run a title search on startup.
    pub query: Option<String>,
    /// Open this document (nreg) in the reader on startup.
    pub nreg: Option<String>,
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
    /// Results list the reader was opened from (for prev/next paging).
    pub nav: Option<(Vec<rada_db::SearchHit>, usize)>,
    pub arrivals: views::ArrivalsState,
    pub fav: views::FavoritesState,
    pub dicts: views::Dicts,
    pub index_status: Option<IndexStatus>,
    pub toasts: Vec<Toast>,
    /// «Про програму» dialog visibility.
    pub show_about: bool,
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
            nav: None,
            arrivals: views::ArrivalsState::default(),
            fav: views::FavoritesState::default(),
            dicts: views::Dicts::default(),
            index_status: None,
            toasts: Vec::new(),
            show_about: false,
            settings,
            theme_sig: String::new(),
            focus_req: None,
        };
        if startup.query.is_none() && startup.nreg.is_none() {
            app.search.query = app.settings.last_query.clone();
        }
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
        });
    }

    pub fn refresh_dicts(&mut self) {
        let db = self.db_path.clone();
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
        self.settings.last_query = self.search.query.trim().to_string();
        crate::settings::save(&self.settings);
        let nreg_guess = self.search.query.trim().to_string();
        self.spawn(move |tx| {
            let conn = match rada_db::open(&db) {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(Msg::Search { run, res: Err(e.to_string()) });
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
            let _ = tx.send(Msg::Search { run, res });
        });
    }

    pub fn reader_open_on(&self, nreg: &str) -> bool {
        self.reader
            .as_ref()
            .map(|r| r.hit.nreg == nreg)
            .unwrap_or(false)
    }

    /// Open a document in the reader: live card + text fetched in the
    /// background (paced by the shared client).
    pub fn open_reader(&mut self, hit: rada_db::SearchHit) {
        self.page = Page::Reader;
        self.nav = None;
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
    pub fn print_reader(&mut self, ctx: &egui::Context) {
        let Some(r) = self.reader.as_ref() else { return };
        let body = r.export_doc("html");
        let path = std::env::temp_dir().join(format!("rada-print-{}.html", sanitize(&r.hit.nreg)));
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
            Msg::Search { run, res } => {
                if run == self.search.run {
                    self.search.loading = false;
                    match res {
                        Ok((hits, total)) => {
                            self.search.total = total;
                            self.search.hits = hits;
                            self.search.selected.clear();
                            self.search.error = None;
                        }
                        Err(e) => {
                            self.search.hits.clear();
                            self.search.error = Some(e);
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
                match res {
                    Ok(card) => {
                        r.editions = card.eds.clone();
                        r.card = Some(card);
                        r.card_error = None;
                    }
                    Err(e) => r.card_error = Some(e),
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
                            self.arrivals.error = None;
                        }
                        Err(e) => {
                            self.arrivals.docs.clear();
                            self.arrivals.from_cache = false;
                            self.arrivals.saved_at = None;
                            self.arrivals.error = Some(e);
                        }
                    }
                }
            }
            Msg::SyncLine(line) => {
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
                            .push(format!("застосовано {applied}, невдалих {failed}"));
                        self.toast(format!("sync: +{applied} (невдалих {failed})"), ok());
                        self.refresh_index_state();
                        self.refresh_dicts();
                    }
                    Err(e) => {
                        self.home.sync_log.push(format!("помилка: {e}"));
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
                            .push(format!("init: імпортовано {n} документів"));
                        self.toast(format!("індекс побудовано: {n} док."), ok());
                        self.refresh_index_state();
                        self.refresh_dicts();
                    }
                    Err(e) => {
                        self.home.sync_log.push(format!("init: помилка {e}"));
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
        // Ctrl+F: focus the find field (reader) or the query field (search).
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::F)) {
            self.focus_req = match self.page {
                Page::Reader => Some(FocusReq::ReaderFind),
                Page::Search => Some(FocusReq::SearchQuery),
                _ => None,
            };
        }
        // Alt+Left/Right: prev/next document inside the reader.
        let alt_prev = ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowLeft));
        let alt_next = ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowRight));
        if self.page == Page::Reader && self.nav.is_some() {
            if alt_prev {
                self.nav_step(-1);
            }
            if alt_next {
                self.nav_step(1);
            }
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

/// «20261027-1530» stamp for backup file names (UTC, no external crates).
fn backup_stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    stamp_from_secs(secs)
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
    use super::stamp_from_secs;

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
}
