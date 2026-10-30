//! Favorites page: every saved annotation across all documents.
//!
//! Rows can be grouped under one header per document (nreg) and filtered
//! client-side by a text needle (annotation text + document title) and a
//! kind chip. Filtering lives in the pure [`row_matches`] /
//! [`filter_and_group`] pair so both render modes share one source.
//!
//! Import is two-step: the picked file is validated first with
//! [`preflight_import`] (what the real importer would import vs skip and
//! why) and only written to the database after «Імпортувати» in the
//! «Перевірка імпорту» window.

use eframe::egui;

use crate::app::{err, gold, App};

const KIND_FILTERS: [&str; 4] = ["усі", "коментарі", "виділення", "закладки"];

/// One favorites list row: annotation + document title
/// (empty title when the doc is not indexed).
pub type FavRow = (rada_db::Annotation, String);

pub struct FavoritesState {
    /// (annotation, document title — empty when the doc is not indexed)
    pub items: Vec<FavRow>,
    pub loading: bool,
    pub error: Option<String>,
    /// Client-side text filter over annotation text and document title.
    pub filter: String,
    /// Kind chip: 0 = усі, then [`KIND_FILTERS`] order
    /// (1 коментарі, 2 виділення, 3 закладки).
    pub kind_sel: usize,
    /// Render rows nested under per-document headers instead of flat.
    pub grouped: bool,
    /// Row order: 0 = новіші перші (default), 1 = старіші перші,
    /// 2 = за назвою документа (A→Я).
    pub sort_sel: usize,
    /// nregs whose group rows are hidden in the grouped view (session-only,
    /// like the web arrivals group collapse).
    pub collapsed: Vec<String>,
    /// First visit triggers an auto-load.
    pub loaded: bool,
    /// Two-step import: file picked through «Імпорт .json», waiting for the
    /// user's verdict in the «Перевірка імпорту» window.
    pub confirm_import: Option<PendingImport>,
}

impl Default for FavoritesState {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            loading: false,
            error: None,
            filter: String::new(),
            kind_sel: 0,
            grouped: true,
            sort_sel: 0,
            collapsed: Vec::new(),
            loaded: false,
            confirm_import: None,
        }
    }
}

/// Pre-import validation verdict for one file (see [`preflight_import`]).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImportPlan {
    /// Entries the real importer would insert.
    pub ok: usize,
    /// Entries the real importer would skip (malformed / blank fields).
    pub bad: usize,
    /// Up to 5 human-readable skip reasons («запис 7: немає nreg»),
    /// 1-based record numbers.
    pub bad_preview: Vec<String>,
    /// Accepted entries per kind, singular page labels
    /// («коментар» / «виділення» / «закладка»), first-appearance order.
    pub kinds: Vec<(String, usize)>,
}

/// A file picked for import whose [`ImportPlan`] awaits confirmation.
#[derive(Debug, Clone)]
pub struct PendingImport {
    pub path: std::path::PathBuf,
    pub plan: ImportPlan,
}

fn kind_tag(kind: &str) -> (&'static str, egui::Color32) {
    match kind {
        "highlight" => ("виділення", crate::themes::accent()),
        "bookmark" => ("закладка", crate::themes::info()),
        _ => ("коментар", crate::themes::weak()),
    }
}

/// Per-chip row counts for the kind filter row: index 0 = усі, 1 = коментарі,
/// 2 = виділення, 3 = закладки (same buckets as [`matches_filter`]).
pub fn kind_counts(rows: &[FavRow]) -> [usize; 4] {
    let mut c = [0usize; 4];
    for (a, _) in rows {
        c[0] += 1;
        if matches_filter(&a.kind, 1) {
            c[1] += 1;
        } else if matches_filter(&a.kind, 2) {
            c[2] += 1;
        } else if matches_filter(&a.kind, 3) {
            c[3] += 1;
        }
    }
    c
}

fn matches_filter(kind: &str, filter: usize) -> bool {
    match filter {
        1 => kind == "comment",
        2 => kind == "highlight",
        3 => kind == "bookmark",
        _ => true,
    }
}

/// Does one row pass the text + kind filters? Case-insensitive substring
/// over the annotation text, the document title *and* the document nreg
/// (plain `to_lowercase`, as the rest of the app filters Ukrainian text with).
pub fn row_matches(a: &rada_db::Annotation, title: &str, needle: &str, kind: usize) -> bool {
    if !matches_filter(&a.kind, kind) {
        return false;
    }
    let q = needle.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }
    a.text.to_lowercase().contains(&q)
        || title.to_lowercase().contains(&q)
        || a.nreg.to_lowercase().contains(&q)
}

/// Toggle the group rows of one `nreg` in the favorites grouped view
/// (add when absent, remove when present; no duplicates).
pub fn toggle_collapse(list: &mut Vec<String>, nreg: &str) {
    match list.iter().position(|x| x == nreg) {
        Some(pos) => {
            list.remove(pos);
        }
        None => list.push(nreg.to_string()),
    }
}

/// Order rows by the favorites sort chip: 0 = newest first, 1 = oldest
/// first, 2 = by document title (case-insensitive, empty titles last,
/// newest first within one title). `created_at` is `YYYY-MM-DD HH:MM:SS`,
/// so a plain string compare orders it chronologically.
pub fn sort_rows(rows: &mut [FavRow], sort_sel: usize) {
    match sort_sel {
        1 => rows.sort_by(|a, b| a.0.created_at.cmp(&b.0.created_at)),
        2 => rows.sort_by(|a, b| {
            let ta = a.1.to_lowercase();
            let tb = b.1.to_lowercase();
            let ea = ta.trim().is_empty();
            let eb = tb.trim().is_empty();
            ea.cmp(&eb).then_with(|| ta.cmp(&tb)).then_with(|| {
                b.0.created_at.cmp(&a.0.created_at)
            })
        }),
        _ => rows.sort_by(|a, b| b.0.created_at.cmp(&a.0.created_at)),
    }
}

/// Filter rows with [`row_matches`], then group them by `nreg`: groups
/// appear in first-appearance order of their rows, and rows keep their
/// input order inside a group. `(nreg, title, rows)`; the group title is
/// the first non-empty title seen among the group's rows.
pub fn filter_and_group(
    rows: &[FavRow],
    needle: &str,
    kind: usize,
) -> Vec<(String, String, Vec<FavRow>)> {
    let mut groups: Vec<(String, String, Vec<FavRow>)> = Vec::new();
    for (a, title) in rows {
        if !row_matches(a, title, needle, kind) {
            continue;
        }
        match groups.iter_mut().find(|(n, _, _)| *n == a.nreg) {
            Some((_, t, rs)) => {
                if t.trim().is_empty() && !title.trim().is_empty() {
                    *t = title.clone();
                }
                rs.push((a.clone(), title.clone()));
            }
            None => groups.push((
                a.nreg.clone(),
                title.clone(),
                vec![(a.clone(), title.clone())],
            )),
        }
    }
    groups
}

/// Plain-text report of the CURRENTLY filtered/sorted Favorites — the exact
/// rows the page shows (same [`row_matches`] filter), so what you see is what
/// downloads. Mirrors the web «експорт .txt»: header «Обрані анотації —
/// {shown} з {total}», then one block per row «[kind] title||nreg ({nreg})»
/// + the annotation text. `rows` is already sorted by [`sort_rows`].
pub fn favorites_txt_report(rows: &[FavRow], total: usize, needle: &str, kind: usize) -> String {
    let kept: Vec<&FavRow> = rows
        .iter()
        .filter(|(a, t)| row_matches(a, t, needle, kind))
        .collect();
    let mut out = format!("Обрані анотації — {} з {total}\n", kept.len());
    for (a, title) in kept {
        let head = if title.trim().is_empty() {
            a.nreg.clone()
        } else {
            title.clone()
        };
        out.push_str(&format!("\n[{}] {head} ({})\n", a.kind, a.nreg));
        out.push_str(a.text.trim_end());
        out.push('\n');
    }
    out
}

/// The CURRENTLY filtered/sorted Favorites as CSV (UTF-8 with BOM so Excel
/// opens Cyrillic correctly) — the web `favoritesCsv` twin: columns
/// `nreg,title,kind,created_at,text`, the same [`row_matches`] filter, so
/// what you see is what downloads. The arrivals/year exports' own quoting
/// family via [`csv_esc`].
pub fn favorites_csv(rows: &[FavRow], needle: &str, kind: usize) -> String {
    let esc = crate::views::arrivals::csv_esc;
    let mut out = String::from("\u{FEFF}nreg,title,kind,created_at,text\n");
    for (a, title) in rows {
        if !row_matches(a, title, needle, kind) {
            continue;
        }
        out.push_str(&esc(&a.nreg));
        out.push(',');
        out.push_str(&esc(title));
        out.push(',');
        out.push_str(&esc(&a.kind));
        out.push(',');
        out.push_str(&esc(&a.created_at));
        out.push(',');
        out.push_str(&esc(&a.text));
        out.push('\n');
    }
    out
}

/// Validate an annotations JSON exactly the way the real importer
/// (`rada_db::annotations::import_json`) accepts rows, *before* anything is
/// written: the same top-level shape rules (array or `{"annotations": [...]}`),
/// the same required string fields of `AnnEntry`, the same blank-nreg skip,
/// and the same kind/text validation `annotations::insert` applies. Whatever
/// this counts as `ok` the importer inserts; `bad` rows it skips.
pub fn preflight_import(json: &str) -> ImportPlan {
    let broken_file = || ImportPlan {
        ok: 0,
        bad: 1,
        bad_preview: vec!["файл не є списком анотацій".to_string()],
        kinds: Vec::new(),
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        return broken_file();
    };
    let Some(arr) = v
        .as_array()
        .or_else(|| v.get("annotations").and_then(|x| x.as_array()))
    else {
        return broken_file();
    };

    let has_text = |s: Option<&str>| s.map_or(false, |t| !t.trim().is_empty());
    let mut plan = ImportPlan::default();
    for (i, item) in arr.iter().enumerate() {
        let nreg = item.get("nreg").and_then(|x| x.as_str());
        let kind = item.get("kind").and_then(|x| x.as_str());
        let text = item.get("text").and_then(|x| x.as_str());
        let problem = if !item.is_object() {
            Some("не об'єкт".to_string())
        } else if !has_text(nreg) {
            Some("немає nreg".to_string())
        } else if !has_text(kind) {
            Some("немає kind".to_string())
        } else if !matches!(kind.unwrap(), "comment" | "highlight" | "bookmark") {
            Some(format!("невідомий тип «{}»", kind.unwrap()))
        } else if kind.unwrap() != "bookmark" && !has_text(text) {
            // insert() demands a snippet/body for highlights and comments.
            Some("немає text".to_string())
        } else if serde_json::from_value::<rada_db::annotations::AnnEntry>(item.clone()).is_err()
        {
            // Required fields are strings already; only span_* can still
            // fail the importer's AnnEntry deserialization.
            Some("невірні span_start/span_end".to_string())
        } else {
            None
        };
        match problem {
            Some(r) => {
                plan.bad += 1;
                if plan.bad_preview.len() < 5 {
                    plan.bad_preview.push(format!("запис {}: {r}", i + 1));
                }
            }
            None => {
                plan.ok += 1;
                let label = kind_tag(kind.unwrap()).0.to_string();
                match plan.kinds.iter_mut().find(|(l, _)| *l == label) {
                    Some((_, n)) => *n += 1,
                    None => plan.kinds.push((label, 1)),
                }
            }
        }
    }
    plan
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    if !app.fav.loaded {
        app.load_favorites();
    }

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.heading("Обране");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("оновити").clicked() {
                    app.load_favorites();
                }
                if app.fav.loading {
                    ui.spinner();
                }
                if ui
                    .button(egui::RichText::new("⬇ Експорт .json").small())
                    .on_hover_text("зберегти всі анотації в портативний JSON")
                    .clicked()
                {
                    app.export_annotations();
                }
                if ui
                    .button(egui::RichText::new("експорт .txt").small())
                    .on_hover_text("зберегти показані анотації у текстовий файл")
                    .clicked()
                {
                    let mut rows = app.fav.items.clone();
                    sort_rows(&mut rows, app.fav.sort_sel);
                    let total = app.fav.items.len();
                    let (needle, kind) = (app.fav.filter.clone(), app.fav.kind_sel);
                    app.export_favorites_txt(rows, total, needle, kind);
                }
                if ui
                    .button(egui::RichText::new("експорт .csv").small())
                    .on_hover_text("зберегти показані анотації у файл .csv")
                    .clicked()
                {
                    let mut rows = app.fav.items.clone();
                    sort_rows(&mut rows, app.fav.sort_sel);
                    let (needle, kind) = (app.fav.filter.clone(), app.fav.kind_sel);
                    app.export_favorites_csv(rows, needle, kind);
                }
                if ui
                    .button(egui::RichText::new("⬆ Імпорт .json").small())
                    .on_hover_text("додати анотації з JSON-файлу (спочатку перевірка)")
                    .clicked()
                {
                    pick_and_preflight_import(app);
                }
            });
        });

        // ---- toolbar: text filter + kind chips + grouping toggle -----
        ui.horizontal_wrapped(|ui| {
            let want_f = app.take_focus(crate::app::FocusReq::FavoritesFilter);
            let fr = ui.add_sized(
                [300.0, 24.0],
                egui::TextEdit::singleline(&mut app.fav.filter)
                    .hint_text("фільтр тексту чи назви…"),
            );
            if want_f {
                fr.request_focus();
            }
            // Persist when the edit commits (focus lost) — not per keystroke.
            if fr.lost_focus() && app.settings.fav_filter != app.fav.filter {
                app.settings.fav_filter = app.fav.filter.clone();
                crate::settings::save(&app.settings);
            }
            let counts = kind_counts(&app.fav.items);
            for (i, lab) in KIND_FILTERS.iter().enumerate() {
                let active = app.fav.kind_sel == i;
                let label = format!("{lab} ({})", counts[i]);
                if ui
                    .add(egui::Button::new(egui::RichText::new(label).small()).selected(active))
                    .clicked()
                {
                    app.fav.kind_sel = i;
                    app.settings.fav_kind = i;
                    crate::settings::save(&app.settings);
                }
            }
            ui.separator();
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("групувати").small())
                        .selected(app.fav.grouped),
                )
                .on_hover_text("групувати анотації під заголовками документів")
                .clicked()
            {
                app.fav.grouped = !app.fav.grouped;
                app.settings.fav_grouped = Some(app.fav.grouped);
                crate::settings::save(&app.settings);
            }
            ui.separator();
            for (i, lab, tip) in [
                (0usize, "новіші", "спочатку найсвіжіші анотації"),
                (1, "старіші", "спочатку найдавніші анотації"),
                (2, "за назвою", "за алфавітом назви документа"),
            ] {
                let active = app.fav.sort_sel == i;
                if ui
                    .add(egui::Button::new(egui::RichText::new(lab).small()).selected(active))
                    .on_hover_text(tip)
                    .clicked()
                {
                    app.fav.sort_sel = i;
                    app.settings.fav_sort = i;
                    crate::settings::save(&app.settings);
                }
            }
            if app.fav.grouped && !app.fav.collapsed.is_empty() {
                ui.separator();
                if ui
                    .button(egui::RichText::new("розгорнути всі").small())
                    .on_hover_text("показати рядки всіх згорнутих груп")
                    .clicked()
                {
                    app.fav.collapsed.clear();
                }
            }
        });
        if let Some(e) = &app.fav.error {
            ui.label(egui::RichText::new(e).small().color(err()));
        }

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut items = app.fav.items.clone();
                sort_rows(&mut items, app.fav.sort_sel);
                let all_empty = items.is_empty();
                let needle = app.fav.filter.clone();
                let kind = app.fav.kind_sel;
                let mut shown = 0usize;
                if app.fav.grouped {
                    let groups = filter_and_group(&items, &needle, kind);
                    for (nreg, title, rows) in groups {
                        shown += rows.len();
                        let short: String = title.chars().take(90).collect();
                        let head = if short.trim().is_empty() {
                            nreg.clone()
                        } else {
                            short
                        };
                        let is_collapsed = app.fav.collapsed.iter().any(|n| n == &nreg);
                        ui.horizontal(|ui| {
                            let head_lbl = if is_collapsed {
                                format!("{head}  ({}) — згорнуто", rows.len())
                            } else {
                                format!("{head}  ({})", rows.len())
                            };
                            let head_btn = ui.add(
                                egui::Button::new(
                                    egui::RichText::new(head_lbl).strong().color(gold()),
                                )
                                .frame(false)
                                .small(),
                            );
                            if head_btn
                                .on_hover_text("клік — згорнути/розгорнути групу")
                                .clicked()
                            {
                                toggle_collapse(&mut app.fav.collapsed, &nreg);
                            }
                            if ui
                                .button(egui::RichText::new("відкрити").small())
                                .on_hover_text("відкрити документ у читалці")
                                .clicked()
                            {
                                let (n, t) = (nreg.clone(), title.clone());
                                app.open_reader_by_nreg(&n, &t, 0, "", "", "");
                            }
                            let toggle_lbl = if is_collapsed { "розгорнути" } else { "згорнути" };
                            if ui
                                .button(egui::RichText::new(toggle_lbl).small())
                                .on_hover_text("сховати/показати рядки цієї групи")
                                .clicked()
                            {
                                toggle_collapse(&mut app.fav.collapsed, &nreg);
                            }
                        });
                        if !is_collapsed {
                            ui.indent(egui::Id::new(("fav_group", nreg.clone())), |ui| {
                                for (a, title) in rows {
                                    annotation_row(app, ui, &a, &title);
                                    ui.add_space(4.0);
                                }
                            });
                        }
                        ui.add_space(6.0);
                    }
                } else {
                    let rows: Vec<FavRow> = items
                        .into_iter()
                        .filter(|(a, t)| row_matches(a, t, &needle, kind))
                        .collect();
                    shown = rows.len();
                    for (a, title) in rows {
                        annotation_row(app, ui, &a, &title);
                        ui.add_space(6.0);
                    }
                }
                if !app.fav.loading && app.fav.error.is_none() {
                    if all_empty {
                        ui.label(
                            "тут з’являються збережені коментарі, виділення та закладки \
                             з усіх документів (додаються у «Читалці»)",
                        );
                    } else if shown == 0 {
                        ui.label(
                            egui::RichText::new("нічого не знайдено (перевірте фільтр)")
                                .small()
                                .color(crate::themes::weak()),
                        );
                    }
                }
            });
    });

    import_confirm_window(app, ctx);
}

/// One annotation card: kind tag + document line + text (or cached-doc
/// preview for bare bookmarks) + open/delete actions.
fn annotation_row(app: &mut App, ui: &mut egui::Ui, a: &rada_db::Annotation, title: &str) {
    let (tag, color) = kind_tag(&a.kind);
    let short: String = title.chars().take(70).collect();
    let head = if short.trim().is_empty() {
        a.nreg.clone()
    } else {
        format!("{short}  ·  {}", a.nreg)
    };
    egui::Frame::none()
        .stroke(egui::Stroke::new(1.0_f32, crate::themes::border()))
        .inner_margin(egui::Margin::same(6.0))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width() - 6.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(tag).small().color(color));
                ui.label(egui::RichText::new(head).small());
                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        ui.label(egui::RichText::new(&a.created_at).weak().small());
                    },
                );
            });
            if !a.text.trim().is_empty() {
                ui.label(
                    egui::RichText::new(&a.text)
                        .small()
                        .color(crate::themes::weak()),
                );
            }
            // Context snippet from the cached document text
            // (only when the annotation itself is empty, e.g.
            // a bare bookmark — otherwise the text says it).
            if a.text.trim().is_empty() {
                if let Some(preview) = app.doc_preview(&a.nreg, 160) {
                    ui.label(
                        egui::RichText::new(preview)
                            .small()
                            .italics()
                            .color(crate::themes::weak()),
                    );
                }
            }
            ui.horizontal(|ui| {
                if ui
                    .button(egui::RichText::new("відкрити документ").small())
                    .clicked()
                {
                    let (n, t) = (a.nreg.clone(), title.to_string());
                    app.open_reader_by_nreg(&n, &t, 0, "", "", "");
                }
                if ui
                    .button(egui::RichText::new("цилата").small())
                    .on_hover_text("скопіювати повне посилання на акт")
                    .clicked()
                {
                    let t = crate::views::reader::citation_line(
                        if title.trim().is_empty() {
                            a.nreg.as_str()
                        } else {
                            title
                        },
                        &a.nreg,
                        None,
                    );
                    ui.ctx().copy_text(t);
                    app.toast("цилату скопійовано", crate::app::ok());
                }
                if ui
                    .button(egui::RichText::new("копія").small())
                    .on_hover_text("скопіювати номер документа")
                    .clicked()
                {
                    ui.ctx().copy_text(a.nreg.clone());
                }
                if ui
                    .button(egui::RichText::new("сайт").small())
                    .on_hover_text("відкрити акт на zakon.rada.gov.ua")
                    .clicked()
                {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(
                        rada_core::RadaClient::site_url(&a.nreg),
                    ));
                }
                if ui
                    .button(egui::RichText::new("видалити").small())
                    .clicked()
                {
                    app.delete_annotation_fresh(a.id);
                }
            });
        });
}

/// Pick a `.json` through the file dialog, preflight it with
/// [`preflight_import`] and stash the verdict in
/// [`FavoritesState::confirm_import`]; nothing is written until the user
/// confirms in the «Перевірка імпорту» window.
fn pick_and_preflight_import(app: &mut App) {
    let mut d = rfd::FileDialog::new().add_filter("json", &["json"]);
    let dir = app.settings.export_dir.clone();
    if !dir.is_empty() && std::path::Path::new(&dir).is_dir() {
        d = d.set_directory(&dir);
    }
    let Some(path) = d.pick_file() else {
        return;
    };
    if let Some(par) = path.parent() {
        app.settings.export_dir = par.display().to_string();
        crate::settings::save(&app.settings);
    }
    match std::fs::read_to_string(&path) {
        Ok(raw) => {
            app.fav.error = None;
            app.fav.confirm_import = Some(PendingImport {
                path,
                plan: preflight_import(&raw),
            });
        }
        Err(e) => app.fav.error = Some(format!("імпорт: не вдалося прочитати файл: {e}")),
    }
}

/// «Перевірка імпорту»: what the importer will do with the picked file,
/// before a single row is written. «Імпортувати» reuses the existing app
/// entry point `App::import_annotations` (file re-pick in the same folder
/// is its own dialog's price for keeping the apply path in `app.rs`).
fn import_confirm_window(app: &mut App, ctx: &egui::Context) {
    let Some(pending) = app.fav.confirm_import.clone() else {
        return;
    };
    let mut confirm = false;
    let mut cancel = false;
    egui::Window::new("Перевірка імпорту")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(format!("файл: {}", pending.path.display()));
            ui.label(format!("імпортується: {}", pending.plan.ok));
            ui.label(format!("пропущено битих: {}", pending.plan.bad));
            if !pending.plan.kinds.is_empty() {
                let kinds: Vec<String> = pending
                    .plan
                    .kinds
                    .iter()
                    .map(|(label, n)| format!("{label} {n}"))
                    .collect();
                ui.label(egui::RichText::new(kinds.join("  ·  ")).small());
            }
            for line in &pending.plan.bad_preview {
                ui.label(egui::RichText::new(line).small().color(crate::themes::weak()));
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Скасувати").clicked() {
                    cancel = true;
                }
                if ui
                    .button(egui::RichText::new("Імпортувати").strong().color(gold()))
                    .on_hover_text("у діалозі виберіть той самий файл ще раз")
                    .clicked()
                {
                    confirm = true;
                }
            });
        });

    if cancel {
        app.fav.confirm_import = None;
    } else if confirm {
        app.fav.confirm_import = None;
        app.import_annotations();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ann(id: i64, nreg: &str, kind: &str, text: &str) -> rada_db::Annotation {
        rada_db::Annotation {
            id,
            nreg: nreg.to_string(),
            kind: kind.to_string(),
            text: text.to_string(),
            span_start: None,
            span_end: None,
            created_at: "2026-10-28 10:00:00".to_string(),
        }
    }

    /// Newest-first, two docs interleaved + one title-less doc.
    fn rows() -> Vec<FavRow> {
        vec![
            (
                ann(5, "2961-20", "comment", "треба перечитати"),
                "Про освіту".to_string(),
            ),
            (
                ann(4, "1556-18", "bookmark", ""),
                "Про запобігання корупції".to_string(),
            ),
            (
                ann(3, "2961-20", "highlight", "стаття 7"),
                "Про освіту".to_string(),
            ),
            (
                ann(2, "777-21", "comment", "перевірити дату"),
                String::new(),
            ),
            (
                ann(1, "1556-18", "comment", "підсумок"),
                "Про запобігання корупції".to_string(),
            ),
        ]
    }

    #[test]
    fn favorites_txt_report_is_wysiwyg() {
        let all = rows();
        // Full export: header shows 5 з 5, every row present in given order.
        let full = favorites_txt_report(&all, all.len(), "", 0);
        let mut it = full.lines();
        assert_eq!(it.next().unwrap(), "Обрані анотації — 5 з 5");
        assert_eq!(it.next().unwrap(), "");
        assert_eq!(it.next().unwrap(), "[comment] Про освіту (2961-20)");
        assert_eq!(it.next().unwrap(), "треба перечитати");
        assert!(full.contains("[bookmark] Про запобігання корупції (1556-18)"), "bookmark row shows title");
        assert!(full.contains("[comment] 777-21 (777-21)"), "title-less doc uses nreg as head");

        // Filtered by kind=comment (index 1) + needle «підсумок»: only one row,
        // header reflects the reduced shown count against the total.
        let only = favorites_txt_report(&all, all.len(), "підсумок", 1);
        assert!(only.starts_with("Обрані анотації — 1 з 5\n"));
        assert!(only.contains("підсумок"));
        assert!(!only.contains("треба перечитати"));
        assert_eq!(only.matches("\n[comment]").count(), 1);

        // Zero matches: header only, no trailing row blocks.
        let none = favorites_txt_report(&all, all.len(), "нема-такого-слова", 0);
        assert_eq!(none, "Обрані анотації — 0 з 5\n");
    }

    #[test]
    fn favorites_csv_is_wysiwyg_with_bom_and_escaping() {
        let all = rows();
        let csv = favorites_csv(&all, "", 0);
        assert!(csv.starts_with('\u{FEFF}'), "BOM first (Excel Cyrillic)");
        assert!(csv.starts_with("\u{FEFF}nreg,title,kind,created_at,text\n"));
        // full list: input order kept, every column present
        assert!(csv.contains("2961-20,Про освіту,comment,2026-10-28 10:00:00,треба перечитати\n"));
        assert!(csv.contains("1556-18,Про запобігання корупції,bookmark,2026-10-28 10:00:00,\n"));
        assert_eq!(csv.matches('\n').count(), 1 + all.len());
        // same row_matches filter as the .txt export: kind=comment + needle
        let only = favorites_csv(&all, "підсумок", 1);
        assert!(only.contains("1556-18,Про запобігання корупції,comment,2026-10-28 10:00:00,підсумок\n"));
        assert!(!only.contains("треба перечитати"));
        assert_eq!(only.matches('\n').count(), 2, "header + one row");
        // zero matches: header only, never an empty file
        assert_eq!(
            favorites_csv(&all, "нема-такого-слова", 0),
            "\u{FEFF}nreg,title,kind,created_at,text\n"
        );
        // quoting family: comma + quotes force doubled quotes
        let tricky = vec![(
            ann(9, "1-1", "comment", "А, \"цитата\""),
            "Назва, з комою".to_string(),
        )];
        let csv = favorites_csv(&tricky, "", 0);
        assert!(csv.contains("1-1,\"Назва, з комою\",comment,2026-10-28 10:00:00,\"А, \"\"цитата\"\"\"\n"));
    }

    #[test]
    fn empty_needle_passes_all_and_groups_keep_first_appearance_order() {
        let g = filter_and_group(&rows(), "", 0);
        assert_eq!(g.len(), 3, "one header per nreg");
        // Group order = first appearance (5 → 2961-20, 4 → 1556-18, 2 → 777-21).
        assert_eq!(g[0].0, "2961-20");
        assert_eq!(g[1].0, "1556-18");
        assert_eq!(g[2].0, "777-21");
        // Rows keep their input order inside a group.
        assert_eq!(g[0].2.len(), 2);
        assert_eq!(g[0].2[0].0.id, 5);
        assert_eq!(g[0].2[1].0.id, 3);
        assert_eq!(g[1].2[0].0.id, 4);
        assert_eq!(g[1].2[1].0.id, 1);
        assert_eq!(g[0].1, "Про освіту");
        assert!(g[2].1.is_empty(), "title-less doc keeps the empty title");
    }

    #[test]
    fn needle_matches_title_only_rows() {
        // «корупці» occurs only in document titles, never in annotation text.
        let g = filter_and_group(&rows(), "корупці", 0);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].0, "1556-18");
        assert_eq!(g[0].1, "Про запобігання корупції");
        // Both rows of the matching document pass (title match lifts them).
        assert_eq!(g[0].2.len(), 2);
    }

    #[test]
    fn needle_is_case_insensitive_over_annotation_text() {
        let g = filter_and_group(&rows(), "ПЕРЕЧИТАТИ", 0);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].0, "2961-20");
        assert_eq!(g[0].2.len(), 1);
        assert_eq!(g[0].2[0].0.id, 5);
    }

    #[test]
    fn kind_filter_picks_only_one_kind() {
        // 3 = закладки: the single bookmark row survives.
        let g = filter_and_group(&rows(), "", 3);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].0, "1556-18");
        assert_eq!(g[0].2.len(), 1);
        assert_eq!(g[0].2[0].0.kind, "bookmark");
        // 1 = коментарі: all three comments, across all their docs.
        let g = filter_and_group(&rows(), "", 1);
        let kinds: Vec<&str> = g
            .iter()
            .flat_map(|(_, _, rs)| rs.iter().map(|(a, _)| a.kind.as_str()))
            .collect();
        assert_eq!(kinds, vec!["comment"; 3]);
    }

    #[test]
    fn no_match_gives_no_groups() {
        assert!(filter_and_group(&rows(), "нічого подібного", 0).is_empty());
        assert!(filter_and_group(&[], "", 0).is_empty());
    }

    #[test]
    fn filter_also_matches_document_nreg() {
        // The nreg is now a third search surface next to text and title.
        let g = filter_and_group(&rows(), "1556", 0);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].0, "1556-18");
        assert_eq!(g[0].2.len(), 2, "both annotations of that doc");
        // Case-insensitive and trimmed, like the other surfaces.
        assert!(row_matches(&ann(1, "88-9", "comment", "т"), "X", " 88-9 ", 0));
        // Kind filter still wins over a matching nreg.
        assert!(!row_matches(&ann(1, "88-9", "comment", "т"), "X", "88-9", 3));
    }

    // ---- preflight_import: mirrors rada_db::annotations::import_json ------

    #[test]
    fn preflight_counts_mixed_array_and_previews_point_at_bad_records() {
        let json = r#"[
            {"nreg": "2961-20", "kind": "comment", "text": "добре"},
            {"nreg": "1556-18", "kind": "bookmark", "text": ""},
            {"nreg": "   ", "kind": "comment", "text": "текст є, реєстраційного ні"},
            {"nreg": "777-21", "kind": "highlight"},
            {"nreg": "1556-18", "kind": "comment", "text": "ще одна"}
        ]"#;
        let p = preflight_import(json);
        assert_eq!(p.ok, 3, "two comments + one bare bookmark importable");
        assert_eq!(p.bad, 2, "blank nreg + missing text");
        assert_eq!(
            p.kinds,
            vec![("коментар".to_string(), 2), ("закладка".to_string(), 1)]
        );
        assert_eq!(p.bad_preview.len(), 2, "1-based indices of the bad rows");
        assert!(
            p.bad_preview[0].starts_with("запис 3:") && p.bad_preview[0].contains("немає nreg"),
            "{:?}",
            p.bad_preview[0]
        );
        assert!(
            p.bad_preview[1].starts_with("запис 4:") && p.bad_preview[1].contains("немає text"),
            "{:?}",
            p.bad_preview[1]
        );
    }

    #[test]
    fn preflight_rejects_missing_nreg() {
        let p = preflight_import(r#"[{"kind": "comment", "text": "без nreg"}]"#);
        assert_eq!((p.ok, p.bad), (0, 1));
        assert_eq!(p.bad_preview, vec!["запис 1: немає nreg".to_string()]);
    }

    #[test]
    fn preflight_rejects_whitespace_nreg() {
        // import_json skips `nreg.trim().is_empty()` — preflight must too.
        let p = preflight_import(r#"[{"nreg": " \t ", "kind": "comment", "text": "текст є"}]"#);
        assert_eq!((p.ok, p.bad), (0, 1));
        assert_eq!(p.bad_preview, vec!["запис 1: немає nreg".to_string()]);
    }

    #[test]
    fn preflight_rejects_unknown_kind() {
        // Deserializes fine, then insert() rejects the kind → the importer
        // skips it, so preflight counts it as bad too.
        let p = preflight_import(r#"[{"nreg": "1-1", "kind": "note", "text": "хиба"}]"#);
        assert_eq!((p.ok, p.bad), (0, 1));
        assert_eq!(
            p.bad_preview,
            vec!["запис 1: невідомий тип «note»".to_string()]
        );
    }

    #[test]
    fn preflight_broken_file_is_a_report_not_a_panic() {
        // Unparseable, truncated, or a well-formed non-list: the real
        // importer bails without inserting anything.
        for garbage in ["", "не json", "[1, 2, 3", "{\"foo\": 1}", "42"] {
            let p = preflight_import(garbage);
            assert_eq!((p.ok, p.bad), (0, 1), "garbage: {garbage:?}");
            assert_eq!(
                p.bad_preview,
                vec!["файл не є списком анотацій".to_string()],
                "garbage: {garbage:?}"
            );
            assert!(p.kinds.is_empty());
        }
    }

    #[test]
    fn preflight_mirrors_importer_leniency_and_strictness() {
        // Wrapped {"annotations": [...]}, unknown extra field and a bare
        // bookmark are all accepted by the importer; a string span_start
        // fails its from_value::<AnnEntry>, so preflight skips it as well.
        let json = r#"{"annotations": [
            {"nreg": "1-1", "kind": "bookmark", "text": "", "extra": true},
            {"nreg": "2-2", "kind": "highlight", "text": "x", "span_start": "12"},
            {"nreg": "3-3", "kind": "highlight", "text": "y", "span_start": 12, "span_end": 17}
        ]}"#;
        let p = preflight_import(json);
        assert_eq!(p.ok, 2, "bare bookmark + good highlight are importable");
        assert_eq!(p.bad, 1, "string span_start is rejected by the importer");
        assert_eq!(
            p.kinds,
            vec![("закладка".to_string(), 1), ("виділення".to_string(), 1)]
        );
        assert!(p.bad_preview[0].starts_with("запис 2:"), "{:?}", p.bad_preview[0]);
    }

    #[test]
    fn preflight_caps_bad_preview_at_five_lines() {
        let items: Vec<String> = (1..=8)
            .map(|i| format!(r#"{{"nreg": "", "kind": "comment", "text": "ігнор {i}"}}"#))
            .collect();
        let p = preflight_import(&format!("[{}]", items.join(",")));
        assert_eq!((p.ok, p.bad), (0, 8));
        assert_eq!(p.bad_preview.len(), 5);
        assert!(p.bad_preview[0].starts_with("запис 1:"));
        assert!(p.bad_preview[4].starts_with("запис 5:"));
    }

    fn ann_at(id: i64, nreg: &str, created: &str) -> rada_db::Annotation {
        let mut a = ann(id, nreg, "comment", "т");
        a.created_at = created.to_string();
        a
    }

    #[test]
    fn kind_counts_buckets_all_comment_highlight_bookmark() {
        let rows: Vec<FavRow> = vec![
            (ann(1, "a-1", "comment", "к"), "А".to_string()),
            (ann(2, "a-1", "comment", "к"), "А".to_string()),
            (ann(3, "b-2", "highlight", "в"), "Б".to_string()),
            (ann(4, "c-3", "bookmark", ""), "В".to_string()),
            (ann(5, "c-3", "weird", "щ"), "В".to_string()),
        ];
        let c = kind_counts(&rows);
        // «усі» counts every row; unknown kinds land only in «усі».
        assert_eq!(c, [5, 2, 1, 1]);
        assert_eq!(kind_counts(&[]), [0, 0, 0, 0]);
    }

    #[test]
    fn toggle_collapse_adds_then_removes_without_duplicates() {
        let mut v: Vec<String> = Vec::new();
        toggle_collapse(&mut v, "2041-15");
        assert_eq!(v, vec!["2041-15".to_string()], "first click collapses");
        toggle_collapse(&mut v, "2041-15");
        assert!(v.is_empty(), "second click expands again");
        // distinct nregs accumulate, each toggles independently
        toggle_collapse(&mut v, "2041-15");
        toggle_collapse(&mut v, "16-15");
        assert_eq!(v, vec!["2041-15".to_string(), "16-15".to_string()]);
        toggle_collapse(&mut v, "2041-15");
        assert_eq!(v, vec!["16-15".to_string()]);
    }

    #[test]
    fn sort_rows_orders_by_date_and_title() {
        let rows = || -> Vec<FavRow> {
            vec![
                (ann_at(1, "a-1", "2026-01-01 10:00:00"), "Ялівка".to_string()),
                (ann_at(2, "b-2", "2026-05-05 09:00:00"), "абрика".to_string()),
                (ann_at(3, "c-3", "2026-03-03 08:00:00"), String::new()),
            ]
        };
        // 0 = newest first.
        let mut r = rows();
        sort_rows(&mut r, 0);
        assert_eq!(
            r.iter().map(|x| x.0.id).collect::<Vec<_>>(),
            vec![2, 3, 1],
            "created_at descending"
        );
        // 1 = oldest first.
        let mut r = rows();
        sort_rows(&mut r, 1);
        assert_eq!(r.iter().map(|x| x.0.id).collect::<Vec<_>>(), vec![1, 3, 2]);
        // 2 = by title case-insensitively, empty titles last.
        let mut r = rows();
        sort_rows(&mut r, 2);
        assert_eq!(
            r.iter().map(|x| x.0.id).collect::<Vec<_>>(),
            vec![2, 1, 3],
            "«абрика» < «Ялівка» (я→ua collation by lowercase code points), порожня назва — остання"
        );
    }

    #[test]
    fn sort_rows_title_mode_breaks_ties_by_newest() {
        let mut r: Vec<FavRow> = vec![
            (ann_at(1, "a-1", "2026-01-01 10:00:00"), "Кодекс".to_string()),
            (ann_at(2, "a-1", "2026-06-06 10:00:00"), "кодекс".to_string()),
        ];
        sort_rows(&mut r, 2);
        assert_eq!(r.iter().map(|x| x.0.id).collect::<Vec<_>>(), vec![2, 1]);
    }
}
