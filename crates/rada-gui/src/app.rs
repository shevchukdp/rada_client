//! GUI application state: pages, background worker plumbing, message pump.
//!
//! Threading: egui is immediate mode and single-threaded, so all blocking
//! work (SQLite reads, paced live API calls, snapshot downloads) runs on
//! short-lived worker threads that report back through an `mpsc` channel.
//! Stale results are filtered with per-request generation counters.

use std::path::PathBuf;
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
        res: Outcome<String>,
    },
    Annotations {
        nreg: String,
        res: Outcome<Vec<rada_db::Annotation>>,
    },
    AnnotationSaved(Outcome<()>),
    AnnotationDeleted(Outcome<bool>),
    Arrivals {
        run: u64,
        res: Outcome<Vec<UpdatedDoc>>,
    },
    SyncLine(String),
    SyncDone(Outcome<(usize, usize)>),
    FetchProgress(FetchProg),
    FetchDone(Outcome<PathBuf>),
    Resolved {
        run: u64,
        res: Option<String>,
    },
    Exported(Outcome<PathBuf>),
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
    pub arrivals: views::ArrivalsState,
    pub dicts: views::Dicts,
    pub index_status: Option<IndexStatus>,
    pub toasts: Vec<Toast>,
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
        apply_theme(&cc.egui_ctx);
        let (tx, rx) = mpsc::channel();
        let pace_secs = pace.unwrap_or(5);
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
            arrivals: views::ArrivalsState::default(),
            dicts: views::Dicts::default(),
            index_status: None,
            toasts: Vec::new(),
        };
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
        self.spawn(move |tx| {
            let res = (|| -> Outcome<_> {
                let conn = rada_db::open(&db).map_err(|e| e.to_string())?;
                rada_db::search::search(&conn, &q).map_err(|e| e.to_string())
            })();
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
        self.spawn(move |tx| {
            let res = match &edition {
                Some(ed) => client.show_edition_text(&nreg, ed).map_err(|e| e.to_string()),
                None => client.show_text(&nreg).map_err(|e| e.to_string()),
            };
            let _ = tx.send(Msg::Text { run, res });
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
            let _ = tx.send(Msg::AnnotationSaved(res));
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
        self.arrivals.run = run;
        self.arrivals.loading = true;
        self.spawn(move |tx| {
            let res = client.updated_docs().map_err(|e| e.to_string());
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
                        "{}:{}:",
                        j.org,
                        j.orgdat.map(|d| d.to_string()).unwrap_or_default()
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

    /// Export the currently open document (md/txt) through a save dialog.
    pub fn export_reader(&mut self, md: bool) {
        let Some(r) = self.reader.as_ref() else { return };
        let body = r.export_doc(md);
        let ext = if md { "md" } else { "txt" };
        let default_name = format!("zakon-{}.{}", sanitize(&r.hit.nreg), ext);
        let Some(path) = rfd::FileDialog::new().set_file_name(default_name).save_file()
        else {
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
        let Some(path) = rfd::FileDialog::new().set_file_name(default_name).save_file()
        else {
            return;
        };
        self.spawn(move |tx| {
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
            Msg::Search { run, res } => {
                if run == self.search.run {
                    self.search.loading = false;
                    match res {
                        Ok((hits, total)) => {
                            self.search.total = total;
                            self.search.hits = hits;
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
                    Ok(t) => {
                        r.text = Some(t);
                        r.text_error = None;
                    }
                    Err(e) => {
                        r.text = None;
                        r.text_error = Some(e);
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
            Msg::AnnotationSaved(res) => match res {
                Ok(()) => {
                    self.toast("анотацію збережено", ok());
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
            Msg::Arrivals { run, res } => {
                if run == self.arrivals.run {
                    self.arrivals.loading = false;
                    match res {
                        Ok(docs) => {
                            self.arrivals.docs = docs;
                            self.arrivals.error = None;
                        }
                        Err(e) => {
                            self.arrivals.docs.clear();
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
            Msg::Resolved { run, res } => {
                if run == self.search.resolve_run {
                    self.search.resolve_loading = false;
                    self.search.resolve_result = res;
                }
            }
            Msg::Exported(res) => match res {
                Ok(path) => self.toast(format!("збережено: {}", path.display()), ok()),
                Err(e) => self.toast(format!("помилка збереження: {e}"), err()),
            },
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_messages();

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
        }

        views::statusbar(self, ctx);
        views::toasts(self, ctx);
    }
}

pub(crate) fn ok() -> egui::Color32 {
    egui::Color32::from_rgb(0x9A, 0xCC, 0x80)
}

pub(crate) fn err() -> egui::Color32 {
    egui::Color32::from_rgb(0xE6, 0x6A, 0x6A)
}

pub(crate) fn gold() -> egui::Color32 {
    egui::Color32::from_rgb(0xFF, 0xB3, 0x00)
}

fn read_index_status(db: &PathBuf) -> IndexStatus {
    let meta = match std::fs::metadata(db) {
        Err(_) => {
            return IndexStatus {
                docs: 0,
                last_import: None,
                size_bytes: 0,
                dicts: Vec::new(),
            }
        }
        Ok(m) => m,
    };
    let mut st = IndexStatus {
        docs: 0,
        last_import: None,
        size_bytes: meta.len(),
        dicts: Vec::new(),
    };
    if let Ok(conn) = rada_db::open(db) {
        st.docs = rada_db::doc_count(&conn).unwrap_or(0);
        st.last_import = rada_db::last_import(&conn);
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

fn apply_theme(ctx: &egui::Context) {
    // Force the dark theme (egui follows the OS theme by default).
    ctx.set_theme(egui::Theme::Dark);
    ctx.style_mut(|s| {
        let bg = egui::Color32::from_rgb(0x1A, 0x22, 0x33);
        let bg2 = egui::Color32::from_rgb(0x14, 0x1B, 0x2A);
        s.visuals.panel_fill = bg;
        s.visuals.window_fill = bg;
        s.visuals.extreme_bg_color = bg2;
        s.visuals.faint_bg_color = egui::Color32::from_rgb(0x22, 0x2C, 0x42);
        s.visuals.hyperlink_color = gold();
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
