//! Reader page: document header, edition switcher, live text, annotations.

use eframe::egui;
use rada_core::models::{LawCardJson, StatusId};

use crate::app::{err, gold, App};
use crate::views::search::fmt_date;

#[derive(Clone)]
pub struct ReaderState {
    pub hit: rada_db::SearchHit,
    pub card_run: u64,
    pub text_run: u64,
    pub card: Option<LawCardJson>,
    pub card_loading: bool,
    pub card_error: Option<String>,
    pub editions: Vec<rada_core::models::Edition>,
    pub cur_edition: Option<String>,
    pub text: Option<String>,
    /// True when `text` came from the offline cache (live fetch failed).
    pub text_cached: bool,
    pub text_loading: bool,
    pub text_error: Option<String>,
    pub annotations: Vec<rada_db::Annotation>,
    pub ann_error: Option<String>,
    /// Index into `ANN_KINDS` (0 = comment, 1 = bookmark, 2 = highlight).
    pub ann_kind_sel: usize,
    pub ann_text: String,
    /// Set when an annotation is clicked to scroll its text into view.
    pub ann_jump: Option<String>,
    /// Find-in-text bar query (case-insensitive).
    pub find: String,
    /// Index of the current find match.
    pub find_cur: usize,
    /// Next/prev match was requested — scroll on the next render.
    pub find_scroll: bool,
    /// Annotation being edited inline (`edit_annotation`): (id, draft).
    pub edit_id: Option<i64>,
    pub edit_text: String,
    /// «Пов'язані документи» parsed from the live page (lazy, on expand).
    pub related: Vec<rada_core::related::RelatedDoc>,
    pub related_run: u64,
    pub related_loading: bool,
    pub related_error: Option<String>,
    /// The related list was already fetched for this document.
    pub related_loaded: bool,
    /// Mouse-selection mode: paragraphs render as read-only `TextEdit`s
    /// so the user can select a span and save it as a highlight.
    pub select_mode: bool,
    /// Cleaned text last selected with the mouse in `select_mode`
    /// (`None` = nothing selected). Drives the «виділити» button.
    pub sel_pending: Option<String>,
    /// «Зміст» panel visibility.
    pub outline_open: bool,
    /// «Порівняти редакції» panel (web parity): visibility, the two picked
    /// editions (`None` = поточна), fetch state and the rendered lines.
    pub cmp_open: bool,
    pub cmp_a: Option<String>,
    pub cmp_b: Option<String>,
    pub cmp_loading: bool,
    pub cmp_lines: Vec<String>,
    pub cmp_error: Option<String>,
    pub cmp_run: u64,
    /// Scroll progress of the body (0..=100), refreshed every render.
    pub scroll_pct: u8,
    /// «Догори» was clicked — force the scroll area back to the top next render.
    pub scroll_to_top: bool,
}

pub const ANN_KINDS: [&str; 3] = ["Коментар", "Закладка", "Виділення"];

/// Tab-strip label for a document: the title trimmed and truncated to 36
/// chars (+ ellipsis); untitled cards fall back to the bare nreg.
pub fn tab_label(title: &str, nreg: &str) -> String {
    let t = title.trim();
    if t.is_empty() {
        return nreg.to_string();
    }
    let full: Vec<char> = t.chars().collect();
    if full.len() <= 36 {
        full.into_iter().collect()
    } else {
        let s: String = full.iter().take(36).collect();
        format!("{s}…")
    }
}

impl ReaderState {
    pub fn new(hit: rada_db::SearchHit) -> Self {
        Self {
            hit,
            card_run: 0,
            text_run: 0,
            card: None,
            card_loading: false,
            card_error: None,
            editions: Vec::new(),
            cur_edition: None,
            text: None,
            text_cached: false,
            text_loading: false,
            text_error: None,
            annotations: Vec::new(),
            ann_error: None,
            ann_kind_sel: 0,
            ann_text: String::new(),
            ann_jump: None,
            find: String::new(),
            find_cur: 0,
            find_scroll: false,
            edit_id: None,
            edit_text: String::new(),
            related: Vec::new(),
            related_run: 0,
            related_loading: false,
            related_error: None,
            related_loaded: false,
            select_mode: false,
            sel_pending: None,
            outline_open: false,
            cmp_open: false,
            cmp_a: None,
            cmp_b: None,
            cmp_loading: false,
            cmp_lines: Vec::new(),
            cmp_error: None,
            cmp_run: 0,
            scroll_pct: 0,
            scroll_to_top: false,
        }
    }

    /// Character span of a text selection (reserved for a future
    /// "highlight from selection" flow; the current UI saves typed snippets).
    pub fn selected_span(&self) -> Option<(i64, i64)> {
        None
    }

    /// Export the current document as Markdown, plain text, or HTML.
    /// HTML inlines saved highlights as gold `<mark>` spans.
    /// Print-ready PLAIN variant of the HTML export: no structural classes,
    /// no gold accents, no `<mark>` highlights — the «простий» reading mode
    /// put on paper. Keeps the header meta line and the annotation footer.
    pub fn print_doc_plain(&self) -> String {
        let title = self
            .card
            .as_ref()
            .map(|c| c.nazva.clone())
            .unwrap_or_else(|| self.hit.nazva.clone());
        let text = self.text.clone().unwrap_or_default();
        let esc = |s: &str| {
            s.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
        };
        let mut s = String::new();
        s.push_str("<!doctype html>\n<html lang=\"uk\">\n<meta charset=\"utf-8\">\n<title>");
        s.push_str(&esc(&title));
        s.push_str("</title>\n<style>\nbody{font-family:Georgia,'Times New Roman',serif;max-width:56em;margin:2em auto;padding:0 1.2em;line-height:1.5;color:#111}\nh1{font-size:1.25em}\nmain p{margin:.35em 0;font-size:1.02em}\nheader{border-bottom:1px solid #888;margin-bottom:1.2em;padding-bottom:.5em}\nfooter{border-top:1px solid #bbb;margin-top:2em;padding-top:.8em;font-size:.9em;color:#333}\n</style>\n");
        s.push_str(&format!(
            "<header><h1>{}</h1>\n<p><b>Рег. №:</b> <code>{}</code> · <b>Стан:</b> {} · <b>Номер:</b> {} · <b>Дата:</b> {} (ред. {})</p></header>\n",
            esc(&title),
            esc(&self.hit.nreg),
            StatusId(self.hit.status).label(),
            esc(&self.hit.org_num),
            fmt_date(&self.hit.org_date),
            fmt_date(&self.hit.poddat),
        ));
        s.push_str("<main>\n");
        for line in text.lines() {
            s.push_str(&format!("<p>{}</p>\n", esc(line)));
        }
        s.push_str("</main>\n");
        if !self.annotations.is_empty() {
            s.push_str("<footer><b>Примітки та виділення</b><ul>\n");
            for a in &self.annotations {
                let (tag, txt) = match a.kind.as_str() {
                    "highlight" => ("виділення", esc(a.text.trim())),
                    "bookmark" => ("закладка", String::new()),
                    _ => ("коментар", esc(a.text.trim())),
                };
                let date = esc(&a.created_at);
                if txt.is_empty() {
                    s.push_str(&format!("<li><b>{tag}</b> · {date}</li>\n"));
                } else {
                    s.push_str(&format!("<li><b>{tag}</b> · {date}<br>{txt}</li>\n"));
                }
            }
            s.push_str("</ul></footer>\n");
        }
        s.push_str("</html>\n");
        s
    }

    pub fn export_doc(&self, fmt: &str) -> String {
        let title = self
            .card
            .as_ref()
            .map(|c| c.nazva.clone())
            .unwrap_or_else(|| self.hit.nazva.clone());
        let text = self.text.clone().unwrap_or_default();
        if fmt == "md" {
            let mut s = String::new();
            s.push_str(&format!("# {title}\n\n"));
            s.push_str(&format!(
                "- **Рег. №:** `{}`\n- **Стан:** {}\n- **Номер:** {}\n- **Дата:** {} (ред. {})\n\n",
                self.hit.nreg,
                StatusId(self.hit.status).label(),
                self.hit.org_num,
                fmt_date(&self.hit.org_date),
                fmt_date(&self.hit.poddat),
            ));
            if let Some(c) = &self.card {
                if let Some(mj) = &c.minjust {
                    if !mj.is_empty() {
                        s.push_str(&format!("- **Реєстр. Мін'юсту:** {mj}\n"));
                    }
                }
                if let Some(p) = &c.pidstava {
                    if !p.is_empty() {
                        s.push_str(&format!("- **Підстава поточної редакції:** {p}\n"));
                    }
                }
                s.push('\n');
            }
            s.push_str("## Текст\n\n```\n");
            s.push_str(text.trim_end());
            s.push_str("\n```\n");
            s.push_str(&ann_appendix(&self.annotations, true));
            s
        } else if fmt == "html" {
            let esc = |s: &str| {
                s.replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;")
            };
            // Structured body: one <p> per classified line, gold marks inlined.
            let mut body = String::new();
            for para in paragraphs(&text) {
                let cls = match para.kind {
                    PKind::DocTitle => "dt",
                    PKind::Subtitle => "ds",
                    PKind::Meta => "mt",
                    PKind::Section => "sec",
                    PKind::Article => "art",
                    PKind::Num => "num",
                    PKind::Plain => "",
                };
                let mut escp = esc(&para.text);
                for a in &self.annotations {
                    if a.kind != "highlight" || a.text.trim().is_empty() {
                        continue;
                    }
                    let needle = esc(&a.text);
                    let (open, close) = ("<mark class=\"hl\">", "</mark>");
                    let mut pos = 0usize;
                    while pos < escp.len() {
                        match escp[pos..].find(&needle) {
                            Some(k) => {
                                let at = pos + k;
                                let repl = format!("{open}{needle}{close}");
                                escp.replace_range(at..at + needle.len(), &repl);
                                pos = at + open.len() + needle.len() + close.len();
                            }
                            None => break,
                        }
                    }
                }
                if cls.is_empty() {
                    body.push_str(&format!("<p>{escp}</p>\n"));
                } else {
                    body.push_str(&format!("<p class=\"{cls}\">{escp}</p>\n"));
                }
            }
            let mut s = String::new();
            s.push_str("<!doctype html>\n<html lang=\"uk\">\n<meta charset=\"utf-8\">\n<title>");
            s.push_str(&esc(&title));
            s.push_str("</title>\n<style>\nbody{font-family:Georgia,'Times New Roman',serif;max-width:56em;margin:2em auto;padding:0 1.2em;line-height:1.5;color:#111}\nmain p{margin:.4em 0;font-size:1.02em}\nmain p.dt{text-align:center;color:#8a6d00;font-weight:700;font-size:1.3em;letter-spacing:.05em}\nmain p.ds{text-align:center;font-weight:700;font-size:1.15em;margin-bottom:1em}\nmain p.mt{color:#666;font-size:.85em;margin:.15em 0}\nmain p.sec{font-weight:700;color:#8a6d00;font-size:1.1em;margin-top:1.5em;text-transform:uppercase}\nmain p.art{font-weight:700;font-size:1.08em;margin-top:1.2em}\nmain p.num{padding-left:1.7em;margin:.3em 0}\nmark.hl{background:#c9a227;color:#1a1a1a;padding:0 .15em;border-radius:2px}\nheader{border-bottom:2px solid #c9a227;margin-bottom:1.2em;padding-bottom:.5em}\nfooter{border-top:1px solid #bbb;margin-top:2em;padding-top:.8em;font-size:.9em;color:#333}\n</style>\n");
            s.push_str(&format!(
                "<header><h1>{}</h1>\n<p><b>Рег. №:</b> <code>{}</code> · <b>Стан:</b> {} · <b>Номер:</b> {} · <b>Дата:</b> {} <span style=\"color:#777\">(ред. {})</span></p></header>\n",
                esc(&title),
                esc(&self.hit.nreg),
                StatusId(self.hit.status).label(),
                esc(&self.hit.org_num),
                fmt_date(&self.hit.org_date),
                fmt_date(&self.hit.poddat),
            ));
            s.push_str("<main>\n");
            s.push_str(&body);
            s.push_str("</main>\n");
            if !self.annotations.is_empty() {
                s.push_str("<footer><b>Примітки та виділення</b><ul>\n");
                for a in &self.annotations {
                    let (tag, txt) = match a.kind.as_str() {
                        "highlight" => ("виділення", esc(a.text.trim())),
                        "bookmark" => ("закладка", String::new()),
                        _ => ("коментар", esc(a.text.trim())),
                    };
                    let date = esc(&a.created_at);
                    if txt.is_empty() {
                        s.push_str(&format!("<li><b>{tag}</b> · {date}</li>\n"));
                    } else {
                        s.push_str(&format!("<li><b>{tag}</b> · {date}<br>{txt}</li>\n"));
                    }
                }
                s.push_str("</ul></footer>\n");
            }
            s.push_str("</html>\n");
            s
        } else {
            let mut s = String::new();
            s.push_str(&title);
            s.push('\n');
            s.push_str(&format!(
                "Рег. №: {}\nСтан: {}\nНомер: {}\nДата: {} (ред. {})\n\n",
                self.hit.nreg,
                StatusId(self.hit.status).label(),
                self.hit.org_num,
                fmt_date(&self.hit.org_date),
                fmt_date(&self.hit.poddat),
            ));
            s.push_str(text.trim_end());
            s.push_str(&ann_appendix(&self.annotations, false));
            s.push('\n');
            s
        }
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    egui::SidePanel::right("reader_ann")
        .resizable(true)
        .default_width(310.0)
        .show(ctx, |ui| {
            annotations_panel(app, ui);
        });

    // ---- document tab strip (Ctrl+Tab / Ctrl+Shift+Tab cycles) ----------
    if !app.tabs.is_empty() {
        let active = app.active_tab;
        let tabs = app.tabs.clone();
        let mut activate: Option<usize> = None;
        let mut close: Option<usize> = None;
        egui::TopBottomPanel::top("reader_tabs").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("Вкладки:").small().weak());
                for (i, t) in tabs.iter().enumerate() {
                    let sel = active == Some(i);
                    let label = tab_label(&t.title, &t.nreg);
                    let b = egui::Button::new(egui::RichText::new(label).small()).selected(sel);
                    if ui
                        .add(b)
                        .on_hover_text(format!("{}\nвідкрити вкладку", t.nreg))
                        .clicked()
                    {
                        activate = Some(i);
                    }
                    if ui
                        .button(egui::RichText::new("×").small())
                        .on_hover_text("закрити вкладку")
                        .clicked()
                    {
                        close = Some(i);
                    }
                    if i + 1 < tabs.len() {
                        ui.separator();
                    }
                }
            });
        });
        if let Some(i) = activate {
            app.tab_activate(i);
        } else if let Some(i) = close {
            app.tab_close(i);
        }
    }

    // «Догори» click from the previous frame: force the body back to the top.
    let jump = app.reader.as_ref().map(|r| r.scroll_to_top).unwrap_or(false);
    let last_pct = app.reader.as_ref().map(|r| r.scroll_pct).unwrap_or(0);
    egui::CentralPanel::default().show(ctx, |ui| {
        let mut sa = egui::ScrollArea::vertical().auto_shrink([false, false]);
        if jump {
            sa = sa.vertical_scroll_offset(0.0);
        }
        let out = sa.show(ui, |ui| {
            header(app, ui);
            ui.add_space(6.0);
            edition_row(app, ui);
            ui.add_space(8.0);
            ui.separator();
            text_block(app, ui);
            related_block(app, ui);
        });
        if let Some(r) = app.reader.as_mut() {
            r.scroll_pct = scroll_pct(out.state.offset.y, out.content_size.y, out.inner_rect.height());
            if jump {
                r.scroll_to_top = false;
            }
        }
    });

    // Floating «Догори» (web btn-top parity): deep in a long act, one click
    // back to the title. Bottom-left, clear of the toasts (bottom-right).
    if top_button_shown(last_pct) {
        egui::Area::new(egui::Id::new("reader_top_btn"))
            .anchor(egui::Align2::LEFT_BOTTOM, [16.0, -16.0])
            .show(ctx, |ui| {
                if ui
                    .button(egui::RichText::new(format!("Догори · {last_pct}%")).strong())
                    .on_hover_text("прокрутити до початку документа")
                    .clicked()
                {
                    if let Some(r) = app.reader.as_mut() {
                        r.scroll_to_top = true;
                    }
                }
            });
    }
}

/// Scroll progress of the reader body in percents: 0 when the document fits
/// without scrolling; clamped to 0..=100 otherwise.
pub fn scroll_pct(offset: f32, content_h: f32, view_h: f32) -> u8 {
    let span = content_h - view_h;
    if span <= 1.0 {
        return 0;
    }
    ((offset / span) * 100.0).clamp(0.0, 100.0) as u8
}

/// Should the floating «Догори» button be visible at this progress?
pub fn top_button_shown(pct: u8) -> bool {
    pct >= 5
}

// ---------------------------------------------------------------------
// related documents
// ---------------------------------------------------------------------

/// «Пов'язані документи»: links to other acts parsed from the live page
/// (one paced request, fetched once per opened document). Rows open the
/// referenced act in the reader.
fn related_block(app: &mut App, ui: &mut egui::Ui) {
    let (loading, error, items) = {
        let r = app.reader.as_ref().unwrap();
        (r.related_loading, r.related_error.clone(), r.related.clone())
    };
    if !loading && items.is_empty() && error.is_none() {
        // First render of this document: kick off the fetch.
        app.load_related();
        return;
    }

    ui.add_space(14.0);
    ui.separator();
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.heading(
            egui::RichText::new("Пов'язані документи")
                .strong()
                .size(17.0),
        );
        if !items.is_empty() {
            ui.label(egui::RichText::new(format!("{}", items.len())).weak().small());
        }
        if loading {
            ui.spinner();
            ui.label(
                egui::RichText::new("завантажую з порталу ...")
                    .small()
                    .color(crate::themes::weak()),
            );
        }
    });
    if let Some(e) = &error {
        ui.label(
            egui::RichText::new(format!("не вдалося отримати: {e}"))
                .small()
                .color(err()),
        );
        if ui
            .button(egui::RichText::new("оновити").small())
            .on_hover_text("повторити запит до порталу")
            .clicked()
        {
            app.retry_related();
        }
        return;
    }
    if items.is_empty() && !loading {
        ui.label(
            egui::RichText::new("посилань на інші акти не знайдено")
                .small()
                .color(crate::themes::weak()),
        );
        return;
    }
    for rel in &items {
        let label = if rel.title.trim().is_empty() {
            rel.nreg.clone()
        } else {
            rel.title.clone()
        };
        let resp = ui
            .horizontal(|ui| {
                ui.label(egui::RichText::new("•").color(gold()));
                ui.selectable_label(false, &label)
            })
            .response;
        if resp.clicked() {
            // Prefer the full offline row (status/dates) when indexed.
            let hit = rada_db::open(&app.db_path)
                .ok()
                .and_then(|conn| rada_db::search::by_nreg(&conn, &rel.nreg).ok().flatten());
            match hit {
                Some(h) => app.open_reader(h),
                None => app.open_reader_by_nreg(&rel.nreg, &rel.title, 0, "", "", ""),
            }
            return;
        }
    }
}

// ---------------------------------------------------------------------
// header
// ---------------------------------------------------------------------

fn header(app: &mut App, ui: &mut egui::Ui) {
    let (title, status, nreg, org_num, org_date, card, opensite) = {
        let r = app.reader.as_ref().unwrap();
        (
            r.card.as_ref().map(|c| c.nazva.clone()).unwrap_or_else(|| r.hit.nazva.clone()),
            r.card.as_ref().map(|c| c.status).unwrap_or(r.hit.status),
            r.hit.nreg.clone(),
            r.hit.org_num.clone(),
            r.hit.org_date.clone(),
            r.card.is_some(),
            rada_core::RadaClient::site_url(&r.hit.nreg),
        )
    };

    let nav_info: Option<(usize, usize)> =
        app.nav.as_ref().map(|(h, i)| (h.len(), i + 1));
    let mut nav_req: Option<i64> = None;

    ui.heading(
        egui::RichText::new(&title)
            .strong()
            .color(gold())
            .size(19.0 * app.settings.text_scale()),
    );
    ui.horizontal(|ui| {
        let (label, color) = chip(status);
        ui.label(
            egui::RichText::new(format!(" {label} "))
                .strong()
                .color(egui::Color32::BLACK)
                .background_color(color),
        );
        ui.label(egui::RichText::new(format!("рег. № {nreg}")).small());
        if ui.button(egui::RichText::new("копіювати").small()).clicked() {
            ui.ctx().copy_text(nreg.clone());
        }
        if !org_num.is_empty() {
            ui.label(egui::RichText::new(format!("№{org_num}")).small());
        }
        if !org_date.is_empty() {
            ui.label(egui::RichText::new(format!("від {}", fmt_date(&org_date))).small());
        }
        if app.reader.as_ref().map(|r| r.text_cached).unwrap_or(false) {
            ui.label(
                egui::RichText::new(" кешовано ")
                    .small()
                    .strong()
                    .color(egui::Color32::BLACK)
                    .background_color(egui::Color32::from_rgb(120, 170, 255)),
            )
            .on_hover_text("живе завантаження не вдалося — текст з офлайн-кешу");
        }
        // paging through the results list this document was opened from
        if let Some((len, pos)) = nav_info {
            if len > 1 {
                ui.separator();
                if ui
                    .button("‹")
                    .on_hover_text("попередній документ (Alt+PageUp)")
                    .clicked()
                {
                    nav_req = Some(-1);
                }
                ui.label(egui::RichText::new(format!("{pos} / {len}")).small());
                if ui
                    .button("›")
                    .on_hover_text("наступний документ (Alt+PageDown)")
                    .clicked()
                {
                    nav_req = Some(1);
                }
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
            if ui.button(egui::RichText::new("🌐 на сайті").small()).clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab(opensite));
            }
            if card
                && ui.button(egui::RichText::new("картка").small()).clicked()
            {
                // toggles handled by the collapsible below; placeholder no-op
            }
        });
    });
    if let Some(d) = nav_req {
        app.nav_step(d);
    }
}

fn chip(status: i64) -> (&'static str, egui::Color32) {
    match status {
        5 => ("ЧИННИЙ", crate::themes::ok()),
        1 => ("ВТРАТИВ ЧИННІСТЬ", crate::themes::err()),
        2 => ("НАБИРАЄ ЧИННОСТІ", crate::themes::accent()),
        3 => ("ДІЮ ЗУПИНЕНО", crate::themes::accent()),
        4 => ("ДІЮ ВІДНОВЛЕНО", crate::themes::info()),
        8 => ("ЧАСТКОВО НАБРАВ", crate::themes::info()),
        6 => ("НЕ НАБРАВ ЧИННОСТІ", crate::themes::weak()),
        _ => ("НЕВИЗНАЧЕНО", crate::themes::weak()),
    }
}

// ---------------------------------------------------------------------
// edition switcher
// ---------------------------------------------------------------------

fn edition_row(app: &mut App, ui: &mut egui::Ui) {
    let (cur, eds, card_loading, card_err) = {
        let r = app.reader.as_ref().unwrap();
        (
            r.cur_edition.clone(),
            r.editions.clone(),
            r.card_loading,
            r.card_error.clone(),
        )
    };
    let mut picked: Option<Option<String>> = None;
    let mut retry = false;

    ui.horizontal(|ui| {
        ui.label("Редакція:");
        let label = match &cur {
            Some(ed) => format!("ред. {}", fmt_date(ed)),
            None => "поточна ред.".to_string(),
        };
        egui::ComboBox::from_id_salt("editions")
            .width(220.0)
            .selected_text(label)
            .show_ui(ui, |ui| {
                if ui.selectable_label(cur.is_none(), "поточна ред.").clicked() {
                    picked = Some(None);
                }
                for e in &eds {
                    let d = e.datred.to_string();
                    let lab = format!("ред. {}", fmt_date(&d));
                    if ui
                        .selectable_label(cur.as_deref() == Some(d.as_str()), lab)
                        .clicked()
                    {
                        picked = Some(Some(d));
                    }
                }
            });
        if card_loading {
            ui.spinner();
            ui.label(
                egui::RichText::new("картка документа (наживо) ...").small(),
            );
        } else if let Some(ce) = &card_err {
            ui.label(egui::RichText::new(format!("картка: {ce}")).small().color(err()));
            if ui.button("ще раз").clicked() {
                retry = true;
            }
        }
    });

    if let Some(p) = picked {
        app.switch_edition(p);
    }
    if retry {
        app.retry_card();
    }

    edition_compare(app, ui, &eds);
}

/// «Порівняти редакції» panel: two edition pickers, a fetch-and-compare
/// button and the rendered report (word/char/time per edition plus the
/// signed difference). Mirrors the web reader's compare panel.
fn edition_compare(app: &mut App, ui: &mut egui::Ui, eds: &[rada_core::models::Edition]) {
    if eds.len() < 2 {
        return; // nothing to compare against
    }
    let (open, a, b, loading, lines, error) = {
        let r = app.reader.as_ref().unwrap();
        (
            r.cmp_open,
            r.cmp_a.clone(),
            r.cmp_b.clone(),
            r.cmp_loading,
            r.cmp_lines.clone(),
            r.cmp_error.clone(),
        )
    };

    let toggle = ui
        .button(egui::RichText::new("Порівняти редакції").small())
        .on_hover_text("порахувати слова, символи й час читання для двох редакцій")
        .clicked();
    if toggle {
        let r = app.reader.as_mut().unwrap();
        r.cmp_open = !r.cmp_open;
        if r.cmp_open {
            // sensible defaults: the shown edition against the previous one
            let shown = r.cur_edition.clone();
            r.cmp_a = shown.clone();
            r.cmp_b = match &shown {
                Some(cur) => eds
                    .iter()
                    .map(|e| e.datred.to_string())
                    .find(|d| d != cur)
                    .or_else(|| eds.first().map(|e| e.datred.to_string())),
                None => eds.first().map(|e| e.datred.to_string()),
            };
            r.cmp_lines.clear();
            r.cmp_error = None;
        }
        return;
    }
    if !open {
        return;
    }

    ui.indent(egui::Id::new("edition_compare"), |ui| {
        let mut new_a: Option<Option<String>> = None;
        let mut new_b: Option<Option<String>> = None;
        let mut run = false;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("A:").small());
            new_a = edition_pick(ui, "cmp_a", &a, eds);
            ui.label(egui::RichText::new("B:").small());
            new_b = edition_pick(ui, "cmp_b", &b, eds);
            if ui
                .button(egui::RichText::new("Порівняти").small())
                .on_hover_text("завантажить обидві редакції (живі запити)")
                .clicked()
            {
                run = true;
            }
            if loading {
                ui.spinner();
            }
        });
        for line in &lines {
            ui.label(egui::RichText::new(line).small());
        }
        if !lines.is_empty() {
            if ui
                .button(egui::RichText::new("експорт звіту").small())
                .on_hover_text("зберегти звіт порівняння у текстовий файл")
                .clicked()
            {
                let (nreg, a, b) = {
                    let r = app.reader.as_ref().unwrap();
                    (r.hit.nreg.clone(), r.cmp_a.clone(), r.cmp_b.clone())
                };
                app.export_compare(&lines, &nreg, a.as_deref(), b.as_deref());
            }
        }
        if let Some(e) = &error {
            ui.label(egui::RichText::new(format!("порівняння: {e}")).small().color(err()));
        }
        if let Some(p) = new_a {
            let r = app.reader.as_mut().unwrap();
            r.cmp_a = p;
            r.cmp_lines.clear();
        }
        if let Some(p) = new_b {
            let r = app.reader.as_mut().unwrap();
            r.cmp_b = p;
            r.cmp_lines.clear();
        }
        if run {
            app.compare_editions();
        }
    });
}

/// A dated-edition picker for the compare panel; `None` = поточна ред.
fn edition_pick(
    ui: &mut egui::Ui,
    id: &str,
    cur: &Option<String>,
    eds: &[rada_core::models::Edition],
) -> Option<Option<String>> {
    let label = match cur {
        Some(ed) => format!("ред. {}", fmt_date(ed)),
        None => "поточна ред.".to_string(),
    };
    let mut picked = None;
    egui::ComboBox::from_id_salt(id)
        .width(180.0)
        .selected_text(label)
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(cur.is_none(), "поточна ред.")
                .clicked()
            {
                picked = Some(None);
            }
            for e in eds {
                let d = e.datred.to_string();
                if ui
                    .selectable_label(cur.as_deref() == Some(d.as_str()), format!("ред. {}", fmt_date(&d)))
                    .clicked()
                {
                    picked = Some(Some(d));
                }
            }
        });
    picked
}

/// Apply a reader-zoom step to the persisted zoom value (0 = default 100%).
/// `delta` is ±10 (the toolbar buttons and Ctrl+Plus/Minus steps); stepping
/// starts from 100 when unset and clamps to 60..=250. `zoom_step(z, 0)`
/// normalises the unset value to 100.
pub fn zoom_step(z: u32, delta: i32) -> u32 {
    let cur = if z == 0 { 100 } else { z };
    (cur as i64 + delta as i64).clamp(60, 250) as u32
}

// ---------------------------------------------------------------------
// text body
// ---------------------------------------------------------------------

fn text_block(app: &mut App, ui: &mut egui::Ui) {
    let (text, t_err, loading, mut jump, anns) = {
        let r = app.reader.as_mut().unwrap();
        (
            r.text.clone(),
            r.text_error.clone(),
            r.text_loading,
            r.ann_jump.take(),
            r.annotations.clone(),
        )
    };
    // Highlight-annotation spans are painted gold inside the text.
    let snippets: Vec<String> = anns
        .iter()
        .filter(|a| a.kind == "highlight" && !a.text.trim().is_empty())
        .map(|a| a.text.clone())
        .collect();
    let zoom_scale = app.settings.text_scale();
    let raw_mode = app.settings.raw;

    // card details (from live card)
    if let Some(c) = app.reader.as_ref().unwrap().card.clone() {
        ui.collapsing("Картка документа (наживо)", |ui| {
            egui::Grid::new("card_meta").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                ui.label("тип");
                ui.label(type_labels(&c.types, &app.dicts.typs));
                ui.end_row();
                if c.org != 0 {
                    ui.label("видавник");
                    ui.label(crate::views::search::filter_label(&app.dicts.organs, c.org));
                    ui.end_row();
                }
                ui.label("поточна ред. від");
                ui.label(c.poddat.map(fmt_i64_date).unwrap_or_else(|| "—".into()));
                ui.end_row();
                ui.label("набрала чинності");
                ui.label(c.pridat.map(fmt_i64_date).unwrap_or_else(|| "—".into()));
                ui.end_row();
                ui.label("кількість редакцій");
                ui.label(format!("{}", c.eds.len()));
                ui.end_row();
                if let Some(mj) = &c.minjust {
                    if !mj.is_empty() {
                        ui.label("Реєстр. Мін'юсту");
                        ui.label(mj.clone());
                        ui.end_row();
                    }
                }
                if let Some(p) = &c.pidstava {
                    if !p.is_empty() {
                        ui.label("підстава ред.");
                        ui.label(p.clone());
                        ui.end_row();
                    }
                }
                if let Some(sz) = c.size {
                    ui.label("розмір тексту");
                    ui.label(format!("{sz} байт"));
                    ui.end_row();
                }
            });
        });
    }

    // export buttons
    ui.horizontal(|ui| {
        if ui.button("⬇ Експорт .md").clicked() {
            app.export_reader("md");
        }
        if ui.button("⬇ Експорт .txt").clicked() {
            app.export_reader("txt");
        }
        if ui.button("⬇ Експорт .html").clicked() {
            app.export_reader("html");
        }
        if let Some(t) = text.clone() {
            if ui
                .button("копіювати текст")
                .on_hover_text("скопіювати весь текст документа в буфер обміну")
                .clicked()
            {
                let n = t.chars().count();
                ui.ctx().copy_text(t);
                app.toast(format!("текст скопійовано ({n} симв.)"), crate::app::ok());
            }
        }
        if ui
            .button("цилата")
            .on_hover_text("скопіювати повне посилання на акт (назва, редакція, URL)")
            .clicked()
        {
            let (nazva, nreg, ed) = {
                let r = app.reader.as_ref().unwrap();
                (r.hit.nazva.clone(), r.hit.nreg.clone(), r.cur_edition.clone())
            };
            let cite = citation_line(&nazva, &nreg, ed.as_deref());
            ui.ctx().copy_text(cite);
            app.toast("цилату скопійовано", crate::app::ok());
        }
        if ui
            .button("Друк")
            .on_hover_text("відкрити друковану версію в браузері (друк — Ctrl+P)")
            .clicked()
        {
            app.print_reader(ui.ctx(), false);
        }
        if ui
            .button("Друк (простий)")
            .on_hover_text(
                "друкована версія без форматування: тільки текст, \
                 без класів, золотих акцентів і підсвічувань",
            )
            .clicked()
        {
            app.print_reader(ui.ctx(), true);
        }
        if let Some(t) = &text {
            let st = reading_stats(t);
            if !st.is_empty() {
                ui.label(egui::RichText::new(st).small().color(crate::themes::weak()))
                    .on_hover_text("оцінка часу читання: ≈400 слів за хвилину");
            }
        }
        ui.separator();
        // Reader font zoom: A−  100%  A+   (persisted in settings.ini).
        let z = app.settings.zoom;
        let zlabel = if z == 0 { "100%".to_string() } else { format!("{z}%") };
        if ui
            .button("A−")
            .on_hover_text("дрібніший текст")
            .clicked()
        {
            app.settings.zoom = zoom_step(z, -10);
            crate::settings::save(&app.settings);
        }
        if ui.button(egui::RichText::new(zlabel).small()).clicked() {
            app.settings.zoom = 0; // reset
            crate::settings::save(&app.settings);
        }
        if ui
            .button("A+")
            .on_hover_text("більший текст")
            .clicked()
        {
            app.settings.zoom = zoom_step(z, 10);
            crate::settings::save(&app.settings);
        }
        if ui
            .add(
                egui::Button::new(egui::RichText::new("Форматований").small())
                    .selected(!app.settings.raw),
            )
            .on_hover_text("вимкнути — простий текст без стилів заголовків/статей")
            .clicked()
        {
            app.settings.raw = false;
            crate::settings::save(&app.settings);
        }
        if ui
            .add(
                egui::Button::new(egui::RichText::new("Простий").small())
                    .selected(app.settings.raw),
            )
            .on_hover_text("усі абзаци одним стилем")
            .clicked()
        {
            app.settings.raw = true;
            crate::settings::save(&app.settings);
        }
        if ui
            .add(
                egui::Button::new(egui::RichText::new("Виділення мишею").small())
                    .selected(app.reader.as_ref().unwrap().select_mode),
            )
            .on_hover_text("режим вибраного мишею: абзаци можна виділяти та зберігати як виділення")
            .clicked()
        {
            let r = app.reader.as_mut().unwrap();
            r.select_mode = !r.select_mode;
            if !r.select_mode {
                r.sel_pending = None;
            }
        }
        if loading {
            ui.spinner();
            ui.label(
                egui::RichText::new("текст документа — живий запит (пауза між запитами 5 с)...")
                    .small(),
            );
        }
    });

    // Mouse-selection mode: hint + save button, enabled once a paragraph
    // TextEdit below has reported a non-empty selection.
    let mut save_sel = false;
    if app.reader.as_ref().map(|r| r.select_mode).unwrap_or(false) {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("мишею виділіть уривок і натисніть «виділити»")
                    .small()
                    .color(crate::themes::weak()),
            );
            let has_sel = app
                .reader
                .as_ref()
                .and_then(|r| r.sel_pending.clone())
                .map(|s| !s.is_empty())
                .unwrap_or(false);
            if ui
                .add_enabled(
                    has_sel,
                    egui::Button::new(egui::RichText::new("виділити").small()),
                )
                .clicked()
            {
                save_sel = true;
            }
        });
    }

    if loading {
        return;
    }

    if let Some(t) = &text {
        let paras = paragraphs(t);
        let select_mode = app.reader.as_ref().map(|r| r.select_mode).unwrap_or(false);
        // Non-empty text reported by a paragraph TextEdit this frame.
        let mut frame_sel: Option<String> = None;

        // ---- find in text --------------------------------------------
        let qlab = app.reader.as_ref().unwrap().find.trim().to_string();
        let mut g_hits: Vec<(usize, (usize, usize))> = Vec::new();
        if !qlab.is_empty() {
            for (pi, p) in paras.iter().enumerate() {
                for (a, b) in find_spans(&p.text, &qlab) {
                    g_hits.push((pi, (a, b)));
                }
            }
        }
        let count = g_hits.len();
        let mut find_req: Option<i64> = None;
        ui.horizontal(|ui| {
            let mut q = app.reader.as_ref().unwrap().find.clone();
            let before = q.clone();
            let want_f = app.take_focus(crate::app::FocusReq::ReaderFind);
            let resp = ui.add_sized(
                [240.0, 24.0],
                egui::TextEdit::singleline(&mut q).hint_text("знайти в тексті ..."),
            );
            if want_f {
                resp.request_focus();
            } else if resp.has_focus()
                && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
            {
                q.clear();
            }
            if q != before {
                let r = app.reader.as_mut().unwrap();
                r.find = q.clone();
                r.find_cur = 0;
            }
            let enter =
                resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            // Shift+Enter keeps the field focused and steps to the previous hit.
            let shift_prev = resp.has_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.shift);
            if ui.button("»").clicked() || (enter && !shift_prev) {
                find_req = Some(1);
            }
            if ui.button("«").clicked() || shift_prev {
                find_req = Some(-1);
            }
            if !q.trim().is_empty() {
                if count == 0 {
                    ui.label(egui::RichText::new("не знайдено").small().color(err()));
                } else {
                    let cur = app.reader.as_ref().unwrap().find_cur.min(count - 1);
                    ui.label(
                        egui::RichText::new(format!("збіг {} / {}", cur + 1, count)).small(),
                    );
                }
            }
        });
        if let Some(d) = find_req {
            if count > 0 {
                let cur = app.reader.as_ref().unwrap().find_cur as i64;
                let n = count as i64;
                let r = app.reader.as_mut().unwrap();
                r.find_cur = ((cur + d).rem_euclid(n)) as usize;
                r.find_scroll = true;
            }
        }
        let cur_idx = if count == 0 {
            0
        } else {
            app.reader.as_ref().unwrap().find_cur.min(count - 1)
        };
        let mut find_target = if count > 0 && app.reader.as_ref().unwrap().find_scroll {
            Some(g_hits[cur_idx].0)
        } else {
            None
        };
        app.reader.as_mut().unwrap().find_scroll = false;

        // ---- outline («Зміст») ----------------------------------------
        // Section/article headings become one-click jump targets.
        let toc = toc_entries(&paras);
        let mut outline_pick: Option<usize> = None;
        if toc.len() >= 2 {
            let open = app.reader.as_ref().map(|r| r.outline_open).unwrap_or(false);
            let resp = egui::CollapsingHeader::new(
                egui::RichText::new(format!("Зміст ({})", toc.len())).strong().size(14.0),
            )
            .id_salt("reader_outline")
            .open(Some(open))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(220.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for (pi, title) in &toc {
                            let short = toc_label(title);
                            let indent = if toc_is_section(title) { 0.0 } else { 14.0 };
                            ui.indent(egui::Id::new(("toc", *pi)), |ui| {
                                ui.add_space(indent);
                                if ui
                                    .selectable_label(false, egui::RichText::new(short).small())
                                    .clicked()
                                {
                                    outline_pick = Some(*pi);
                                }
                            });
                        }
                    });
            });
            // Controlled open state (`open(Some(..))`): flip it ourselves on a
            // header click (egui's `openness` is an animation float).
            let now_open = if resp.header_response.clicked() {
                !open
            } else {
                open
            };
            if let Some(r) = app.reader.as_mut() {
                r.outline_open = now_open;
            }
        }
        let mut outline_target = outline_pick;
        ui.add_space(6.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            for (pi, para) in paras.iter().enumerate() {
                // breathing room: extra before structural headings.
                let gap = if raw_mode {
                    2.0
                } else {
                    match para.kind {
                        PKind::DocTitle | PKind::Section | PKind::Article => 14.0,
                        PKind::Subtitle | PKind::Meta => 4.0,
                        _ => 2.0,
                    }
                };
                ui.add_space(gap);
                let y_before = ui.cursor().min.y;
                let x_left = ui.min_rect().min.x;
                let x_right = ui.max_rect().max.x;
                let mut p_hits: Vec<(usize, usize, Seg)> = Vec::new();
                if !qlab.is_empty() {
                    for (gi, (hpi, (a, b))) in g_hits.iter().enumerate() {
                        if *hpi == pi {
                            p_hits.push((
                                *a,
                                *b,
                                if gi == cur_idx {
                                    Seg::Cur
                                } else {
                                    Seg::Hit
                                },
                            ));
                        }
                    }
                }
                let ann = ann_spans(&para.text, &snippets);
                let kind = if raw_mode { PKind::Plain } else { para.kind };
                let centered = matches!(kind, PKind::DocTitle | PKind::Subtitle);
                let scale = zoom_scale;
                let mut body = |ui: &mut egui::Ui| {
                    if select_mode {
                        // Selectable mode: a read-only multiline TextEdit is the
                        // only built-in way in egui 0.29 to select text with the
                        // mouse (labels are not selectable). The buffer is a
                        // fresh clone every frame, so typing never touches the
                        // document; custom per-kind colors are lost here
                        // (accepted trade-off). Whole laws are ~200 KB of text,
                        // so one TextEdit per paragraph stays responsive inside
                        // the outer ScrollArea. Double-click word/paragraph
                        // selection works natively — nothing extra needed.
                        let mut buf = para.text.clone();
                        let resp = ui.add(
                            egui::TextEdit::multiline(&mut buf)
                                .id_salt(("sel", pi))
                                .font(sel_font_id(ui, kind, scale))
                                .text_color(ui.visuals().text_color())
                                .frame(false)
                                .margin(egui::Margin::same(0.0))
                                .interactive(true),
                        );
                        // egui 0.29 has no `Response::selected_text()` (added
                        // in 0.30); read the widget's stored cursor state and
                        // slice the paragraph by *character* index (Cyrillic
                        // safe). The focused edit wins over stale unfocused
                        // selections left in other paragraphs.
                        if let Some(cr) = egui::TextEdit::load_state(ui.ctx(), resp.id)
                            .and_then(|s| s.cursor.char_range())
                        {
                            let [a, b] = cr.sorted();
                            if a.index < b.index {
                                let text: String = para
                                    .text
                                    .chars()
                                    .skip(a.index)
                                    .take(b.index - a.index)
                                    .collect();
                                if !text.trim().is_empty()
                                    && (frame_sel.is_none() || resp.has_focus())
                                {
                                    frame_sel = Some(text);
                                }
                            }
                        }
                        return;
                    }
                    ui.horizontal_wrapped(|ui| {
                        for (seg, part) in styled(&para.text, &ann, &p_hits) {
                            match seg {
                                Seg::Plain => {
                                    ui.label(base_rt(kind, part, scale));
                                }
                                Seg::Ann => {
                                    ui.label(
                                        base_rt(kind, part, scale)
                                            .background_color(gold())
                                            .color(egui::Color32::BLACK)
                                            .strong(),
                                    );
                                }
                                Seg::Hit => {
                                    ui.label(
                                        egui::RichText::new(part)
                                            .background_color(egui::Color32::from_rgb(
                                                0x1E, 0x4D, 0x59,
                                            ))
                                            .color(egui::Color32::from_rgb(0xD8, 0xF2, 0xFF)),
                                    );
                                }
                                Seg::Cur => {
                                    ui.label(
                                        egui::RichText::new(part)
                                            .background_color(egui::Color32::from_rgb(
                                                0x4D, 0xC1, 0xF0,
                                            ))
                                            .color(egui::Color32::BLACK)
                                            .strong(),
                                    );
                                }
                            }
                        }
                    });
                };
                if centered {
                    ui.vertical_centered(body);
                } else if kind == PKind::Num {
                    ui.indent(egui::Id::new(("num", pi)), |ui| body(ui));
                } else {
                    body(ui);
                }
                // "→ текст": scroll the first paragraph holding the annotation.
                if let Some(j) = &jump {
                    if !j.is_empty() && para.text.contains(j.as_str()) {
                        let rect = egui::Rect::from_min_max(
                            egui::pos2(x_left, y_before),
                            egui::pos2(x_right, ui.cursor().min.y),
                        );
                        ui.scroll_to_rect(rect, Some(egui::Align::Center));
                        jump = None;
                    }
                }
                // scroll the paragraph holding the current find match.
                if find_target == Some(pi) {
                    let rect = egui::Rect::from_min_max(
                        egui::pos2(x_left, y_before),
                        egui::pos2(x_right, ui.cursor().min.y),
                    );
                    ui.scroll_to_rect(rect, Some(egui::Align::Center));
                    find_target = None;
                }
                // scroll the heading picked in the outline («Зміст»).
                if outline_target == Some(pi) {
                    let rect = egui::Rect::from_min_max(
                        egui::pos2(x_left, y_before),
                        egui::pos2(x_right, ui.cursor().min.y),
                    );
                    ui.scroll_to_rect(rect, Some(egui::Align::Center));
                    outline_target = None;
                }
            }
            ui.add_space(40.0);
        });

        // Publish this frame's mouse selection, then act on the «виділити»
        // button (whose enabled state came from the previous frame's value).
        let to_save = if save_sel {
            app.reader.as_ref().and_then(|r| r.sel_pending.clone())
        } else {
            None
        };
        {
            let r = app.reader.as_mut().unwrap();
            r.sel_pending = frame_sel
                .as_deref()
                .and_then(|s| clean_selection(s, SEL_MAX_CHARS));
        }
        if let Some(t) = to_save {
            // `save_annotation` reads `selected_span()` (spans are unset in
            // this flow), so it stores the snippet with `None` spans — same
            // as the panel's highlight save. The Msg::AnnotationSaved
            // handler reloads the annotation list on success.
            app.save_annotation("highlight", &t);
            app.reader.as_mut().unwrap().sel_pending = None;
        }
    } else if let Some(e) = &t_err {
        ui.label(egui::RichText::new(format!("текст: {e}")).color(err()));
        if ui.button("ще раз").clicked() {
            app.retry_text();
        }
    }
}

/// Styling of a body-text segment.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Seg {
    Plain,
    /// Highlight-annotation span (gold).
    Ann,
    /// Find-bar match (dim cyan).
    Hit,
    /// Current find-bar match (bright cyan).
    Cur,
}

/// Every non-overlapping highlight-annotation span inside `p`.
fn ann_spans(p: &str, snippets: &[String]) -> Vec<(usize, usize)> {
    let mut spans: Vec<(usize, usize)> = Vec::new();
    for s in snippets {
        let mut start = 0usize;
        while let Some(i) = p[start..].find(s.as_str()) {
            let (a, b) = (start + i, start + i + s.len());
            if spans.iter().all(|(x, y)| b <= *x || a >= *y) {
                spans.push((a, b));
            }
            start = b;
        }
    }
    spans.sort();
    spans
}

/// Byte ranges where `q` occurs in `p` (case-insensitive, Cyrillic-safe).
fn find_spans(p: &str, q: &str) -> Vec<(usize, usize)> {
    let ql: Vec<char> = q.to_lowercase().chars().collect();
    if ql.is_empty() {
        return Vec::new();
    }
    // Map every lowercased char back to its byte offset in the source, so
    // slicing stays on valid char boundaries.
    let mut map: Vec<(char, usize)> = Vec::new();
    for (bi, ch) in p.char_indices() {
        for lc in ch.to_lowercase() {
            map.push((lc, bi));
        }
    }
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + ql.len() <= map.len() {
        if map[i..i + ql.len()]
            .iter()
            .zip(ql.iter())
            .all(|((c, _), qc)| c == qc)
        {
            let a = map[i].1;
            let b = if i + ql.len() < map.len() {
                map[i + ql.len()].1
            } else {
                p.len()
            };
            if b > a {
                out.push((a, b));
                i += ql.len();
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Split `p` into styled segments; find `hits` win over annotation `ann`
/// spans where they overlap.
fn styled<'a>(
    p: &'a str,
    ann: &[(usize, usize)],
    hits: &[(usize, usize, Seg)],
) -> Vec<(Seg, &'a str)> {
    let mut pts: Vec<usize> = vec![0, p.len()];
    for (a, b) in ann {
        pts.push(*a);
        pts.push(*b);
    }
    for (a, b, _) in hits {
        pts.push(*a);
        pts.push(*b);
    }
    pts.sort();
    pts.dedup();
    let mut out: Vec<(Seg, &'a str)> = Vec::new();
    for w in pts.windows(2) {
        let (x, y) = (w[0], w[1]);
        if x >= y {
            continue;
        }
        let mid = x + (y - x) / 2;
        let mut seg = Seg::Plain;
        for (a, b, s) in hits {
            if mid >= *a && mid < *b {
                seg = *s;
                break;
            }
        }
        if seg == Seg::Plain {
            for (a, b) in ann {
                if mid >= *a && mid < *b {
                    seg = Seg::Ann;
                    break;
                }
            }
        }
        out.push((seg, &p[x..y]));
    }
    out
}

/// Append a notes section to an exported document (empty if none).
fn ann_appendix(anns: &[rada_db::Annotation], md: bool) -> String {
    if anns.is_empty() {
        return String::new();
    }
    let mut s = String::new();
    if md {
        s.push_str(&format!(
            "\n## Примітки та виділення ({})\n\n",
            anns.len()
        ));
        for a in anns {
            let body = if a.text.trim().is_empty() {
                "—".to_string()
            } else {
                a.text.replace('\n', " ")
            };
            s.push_str(&format!(
                "- **[{}]** `{}` — {}\n",
                ann_kind_uk(&a.kind),
                a.created_at,
                body
            ));
        }
    } else {
        s.push_str(&format!(
            "\n\nПРИМІТКИ ТА ВИДІЛЕННЯ ({})\n\n",
            anns.len()
        ));
        for a in anns {
            s.push_str(&format!(
                "[{}] {}\n{}\n\n",
                ann_kind_uk(&a.kind),
                a.created_at,
                a.text
            ));
        }
    }
    s
}

fn ann_kind_uk(k: &str) -> &'static str {
    match k {
        "highlight" => "виділення",
        "bookmark" => "закладка",
        _ => "коментар",
    }
}

/// Structural role of a paragraph inside a legal act.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PKind {
    /// «ЗАКОН УКРАЇНИ», «УКАЗ ПРЕЗИДЕНТА УКРАЇНИ», … (leading ALL-CAPS line).
    DocTitle,
    /// The name line right below the document title («Про …»).
    Subtitle,
    /// Publication data / amendments block: «(Відомості …)», `{Із змінами …}`.
    Meta,
    /// «Розділ …» / «Глава …».
    Section,
    /// «Стаття N …».
    Article,
    /// Numbered item: «1. …», «2) …».
    Num,
    /// Ordinary body text.
    Plain,
}

/// One rendered paragraph (line-granular, classified).
pub struct Para {
    pub text: String,
    pub kind: PKind,
}

/// All letters uppercase (and at least three of them): header lines.
fn is_all_caps(s: &str) -> bool {
    let mut alpha = 0usize;
    for c in s.chars() {
        if c.is_alphabetic() {
            alpha += 1;
            if !c.is_uppercase() {
                return false;
            }
        }
    }
    alpha >= 3
}

/// Case-insensitive keyword prefix («Стаття» in any case).
fn kw(line: &str, word: &str) -> bool {
    let l = line.trim_start().to_uppercase();
    l.starts_with(&word.to_uppercase())
}

/// «1. …» / «2) …» — a numbered item.
fn numbered(line: &str) -> bool {
    let t = line.trim_start();
    let mut it = t.chars();
    match it.next() {
        Some(c) if c.is_ascii_digit() => matches!(it.next(), Some('.') | Some(')')),
        _ => false,
    }
}

/// Split the raw act text into classified paragraphs (Rada texts use one
/// line per logical paragraph; `{…}` wraps the amendments history).
pub fn paragraphs(t: &str) -> Vec<Para> {
    let mut out: Vec<Para> = Vec::new();
    let mut in_brace = false;
    for raw in t.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let kind = if in_brace {
            if line.contains('}') {
                in_brace = false;
            }
            PKind::Meta
        } else if line.starts_with('{') {
            in_brace = !line.contains('}');
            PKind::Meta
        } else if out.is_empty() && is_all_caps(line) {
            PKind::DocTitle
        } else if line.starts_with('(') && line.ends_with(')') {
            PKind::Meta
        } else if line.starts_with('№') && line.contains("від") {
            PKind::Meta
        } else if kw(line, "стаття") {
            PKind::Article
        } else if kw(line, "розділ") || kw(line, "глава") {
            PKind::Section
        } else if numbered(line) {
            PKind::Num
        } else {
            PKind::Plain
        };
        out.push(Para {
            text: line.to_string(),
            kind,
        });
    }
    // «Про …» right under the ALL-CAPS header is the act's name.
    if out.len() >= 2 && out[0].kind == PKind::DocTitle && out[1].kind == PKind::Plain {
        out[1].kind = PKind::Subtitle;
    }
    out
}

/// Max characters kept from a mouse selection before saving.
const SEL_MAX_CHARS: usize = 500;

/// Outline entries: `(paragraph index, heading text)` for structural
/// headings («Розділ/Глава/Стаття …») in document order.
pub fn toc_entries(paras: &[Para]) -> Vec<(usize, String)> {
    paras
        .iter()
        .enumerate()
        .filter_map(|(i, p)| match p.kind {
            PKind::Section | PKind::Article => Some((i, p.text.clone())),
            _ => None,
        })
        .collect()
}

/// Resolve a card's raw `|`-separated type ids into dictionary labels
/// joined with « · » (non-numeric tokens pass through as-is; empty → «—»).
pub fn type_labels(types: &str, dict: &[(i64, String)]) -> String {
    let parts: Vec<String> = types
        .split('|')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| match s.parse::<i64>() {
            Ok(id) => crate::views::search::filter_label(dict, id),
            Err(_) => s.to_string(),
        })
        .collect();
    if parts.is_empty() {
        "—".into()
    } else {
        parts.join(" · ")
    }
}

/// Combo label of an edition selector (`None` = поточна ред.).
pub fn ed_label(ed: &Option<String>) -> String {
    match ed {
        Some(d) => format!("ред. {}", fmt_date(d)),
        None => "поточна ред.".to_string(),
    }
}

/// Citation line of a document (web «цилата» parity):
/// ««{title}» // Офіційний вебпортал парламенту України. Редакція від
/// {dd.mm.yyyy}, URL: https://zakon.rada.gov.ua/laws/show/{nreg}».
/// The edition part is omitted for `None`/blank editions (the current one).
pub fn citation_line(title: &str, nreg: &str, edition: Option<&str>) -> String {
    let mut s = format!("«{title}» // Офіційний вебпортал парламенту України.");
    if let Some(ed) = edition {
        if !ed.trim().is_empty() {
            s.push_str(&format!(" Редакція від {},", fmt_date(ed)));
        }
    }
    s.push_str(&format!(" URL: https://zakon.rada.gov.ua/laws/show/{nreg}"));
    s
}

/// Body of the compare-panel report saved via «експорт звіту»: a header
/// (nreg + the two edition labels in the combo's own wording) and the
/// rendered lines verbatim. One source for the file content.
pub fn cmp_export_body(
    lines: &[String],
    nreg: &str,
    a: Option<&str>,
    b: Option<&str>,
) -> String {
    let lab = |ed: Option<&str>| match ed {
        Some(d) => format!("ред. {}", fmt_date(d)),
        None => "поточна ред.".to_string(),
    };
    let mut out = format!("Порівняння редакцій {nreg}\nA: {}\nB: {}\n\n", lab(a), lab(b));
    for l in lines {
        out.push_str(l);
        out.push('\n');
    }
    out
}

/// Cycle the reader edition with Alt+Up/Down: the combo list is
/// [поточна (`None`), eds[0], eds[1], …] and `delta` steps through it with
/// wrap-around (`+1` = down, `-1` = up). `None` means nothing to do
/// (single-entry list, or an edition unknown to the list).
pub fn cycle_edition(eds: &[String], cur: Option<&str>, delta: i32) -> Option<Option<String>> {
    let len = eds.len() + 1; // +1 for «поточна ред.»
    if len <= 1 {
        return None;
    }
    let pos: usize = match cur {
        None => 0,
        Some(c) => eds.iter().position(|d| d == c).map(|i| i + 1)?,
    };
    let next = (pos as i64 + delta as i64).rem_euclid(len as i64) as usize;
    if next == pos {
        return None;
    }
    Some(if next == 0 {
        None
    } else {
        Some(eds[next - 1].clone())
    })
}

/// Toggle the reader text mode (Ctrl+M / the Форматований-Простий buttons):
/// returns `(new_raw, toast_label)` — `true` = plain single-style text.
pub fn toggle_raw_mode(current_raw: bool) -> (bool, &'static str) {
    if current_raw {
        (false, "форматований текст")
    } else {
        (true, "простий текст")
    }
}

/// Word/char/time stats of one edition for the compare panel
/// (««ред. 12.03.2020»: 401 слів · 2 406 симв. · ~2 хв»), same wording as
/// the web UI's `cmpSummaryLine`.
pub fn cmp_word_line(label: &str, text: &str) -> String {
    let words = text.split_whitespace().count();
    let chars = text.chars().count();
    let minutes = words.div_ceil(400).max(1);
    format!(
        "«{label}»: {} слів · {} симв. · ~{} хв",
        group_thousands(words),
        group_thousands(chars),
        minutes
    )
}

/// Signed word-count difference of two editions («різниця: +120 слів»),
/// `b` minus `a`; mirrors the web `cmpDiffLine`.
pub fn cmp_diff_line(words_a: usize, words_b: usize) -> String {
    let d = words_b as i64 - words_a as i64;
    format!("різниця: {:+} слів", d)
}

/// The full compare report: one line per edition plus the difference line.
pub fn cmp_report(la: &str, ta: &str, lb: &str, tb: &str) -> Vec<String> {
    let wa = ta.split_whitespace().count();
    let wb = tb.split_whitespace().count();
    vec![
        cmp_word_line(la, ta),
        cmp_word_line(lb, tb),
        cmp_diff_line(wa, wb),
    ]
}

/// Reading stats of the act body as a compact toolbar label
/// («401 слів · 2 406 симв. · ~2 хв», ≈400 words/min, min 1 хв;
/// thousands grouped with spaces). Empty string for empty text.
pub fn reading_stats(text: &str) -> String {
    let words = text.split_whitespace().count();
    if words == 0 {
        return String::new();
    }
    let chars = text.chars().count();
    let minutes = words.div_ceil(400).max(1);
    format!(
        "{} слів · {} симв. · ~{} хв",
        group_thousands(words),
        group_thousands(chars),
        minutes
    )
}

/// Decimal grouping with U+0020 every three digits (1234567 → «1 234 567»).
pub fn group_thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// One-line outline label: max 90 *characters* (Cyrillic-safe) with an
/// ellipsis when clipped.
pub fn toc_label(title: &str) -> String {
    let n = title.chars().count();
    if n <= 90 {
        return title.to_string();
    }
    let mut s: String = title.chars().take(90).collect();
    s.push('…');
    s
}

/// True for top-level headings («Розділ …» / «Глава …», any case).
pub fn toc_is_section(title: &str) -> bool {
    let l = title.trim_start().to_uppercase();
    l.starts_with("РОЗДІЛ") || l.starts_with("ГЛАВА")
}

/// Normalize a raw mouse selection for saving: trim, collapse whitespace
/// runs to single spaces, cap at `max_chars` *characters* (Cyrillic-safe).
/// `None` when the selection is empty or whitespace-only.
pub fn clean_selection(raw: &str, max_chars: usize) -> Option<String> {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    Some(collapsed.chars().take(max_chars).collect())
}

/// Font used by the mouse-selection `TextEdit`s: mirrors `base_rt`'s
/// per-kind sizes (TextEdit takes a single `font_id` for the whole widget).
fn sel_font_id(ui: &egui::Ui, kind: PKind, scale: f32) -> egui::FontId {
    if scale == 1.0 && matches!(kind, PKind::Plain | PKind::Num) {
        return egui::TextStyle::Body.resolve(ui.style());
    }
    let px = match kind {
        PKind::DocTitle => 20.0,
        PKind::Subtitle => 17.0,
        PKind::Meta => 11.0,
        PKind::Section => 16.0,
        PKind::Article => 15.0,
        PKind::Num | PKind::Plain => 13.0,
    };
    egui::FontId::proportional(px * scale)
}

/// Base styling of one paragraph kind (find/annotation spans may override).
/// `scale` comes from the user's reader zoom setting (1.0 = default).
fn base_rt(kind: PKind, part: &str, scale: f32) -> egui::RichText {
    let rt = egui::RichText::new(part);
    match kind {
        PKind::DocTitle => rt.strong().size(20.0 * scale).color(gold()),
        PKind::Subtitle => rt.strong().size(17.0 * scale).color(crate::themes::info()),
        PKind::Meta => {
            if scale == 1.0 {
                rt.small().weak()
            } else {
                rt.size(11.0 * scale).weak()
            }
        }
        PKind::Section => rt.strong().size(16.0 * scale).color(gold()),
        PKind::Article => rt.strong().size(15.0 * scale),
        PKind::Num | PKind::Plain => {
            if scale == 1.0 {
                rt
            } else {
                rt.size(13.0 * scale)
            }
        }
    }
}

fn fmt_i64_date(d: i64) -> String {
    fmt_date(&d.to_string())
}

// ---------------------------------------------------------------------
// annotations sidebar
// ---------------------------------------------------------------------

fn annotations_panel(app: &mut App, ui: &mut egui::Ui) {
    let (nreg, list, ann_err) = {
        let r = app.reader.as_ref().unwrap();
        (
            r.hit.nreg.clone(),
            r.annotations.clone(),
            r.ann_error.clone(),
        )
    };

    ui.heading(
        egui::RichText::new("Анотації")
            .strong()
            .size(17.0),
    );
    ui.label(egui::RichText::new(format!("{nreg}")).small());
    ui.add_space(4.0);

    ui.add_space(6.0);
    ui.strong(format!("Збережено: {}", list.len()));
    egui::ScrollArea::vertical().show(ui, |ui| {
        if let Some(e) = &ann_err {
            ui.label(egui::RichText::new(e).small().color(err()));
        }
        let mut del: Option<i64> = None;
        let mut jump_to: Option<String> = None;
        let mut open_edit: Option<(i64, String)> = None;
        for a in &list {
            let (kind, color) = match a.kind.as_str() {
                "highlight" => ("виділення", gold()),
                "bookmark" => ("закладка", crate::themes::info()),
                _ => ("коментар", crate::themes::weak()),
            };
            egui::Frame::none()
                .stroke(egui::Stroke::new(1.0_f32, crate::themes::border()))
                .inner_margin(egui::Margin::same(6.0))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width() - 6.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(kind).small().color(color));
                        ui.label(egui::RichText::new(&a.created_at).weak().small());
                        if !a.text.trim().is_empty() {
                            if ui.button(egui::RichText::new("» текст").small()).clicked() {
                                jump_to = Some(a.text.clone());
                            }
                        }
                    });
                    ui.label(egui::RichText::new(&a.text));
                    ui.horizontal(|ui| {
                        if ui.button(egui::RichText::new("змінити").small()).clicked() {
                            open_edit = Some((a.id, a.text.clone()));
                        }
                        if ui.button(egui::RichText::new("видалити").small()).clicked() {
                            del = Some(a.id);
                        }
                    });
                });
        }
        if let Some(j) = jump_to {
            app.reader.as_mut().unwrap().ann_jump = Some(j);
        }
        if let Some(id) = del {
            app.delete_annotation(id);
        }
        if let Some((id, t)) = open_edit {
            let r = app.reader.as_mut().unwrap();
            r.edit_id = Some(id);
            r.edit_text = t;
        }
    });

    // ---- inline edit of one annotation --------------------------------
    if let Some(id) = app.reader.as_ref().unwrap().edit_id {
        ui.separator();
        ui.strong(format!("Зміна тексту анотації #{id}"));
        ui.add_sized(
            [ui.available_width(), 54.0],
            egui::TextEdit::multiline(&mut app.reader.as_mut().unwrap().edit_text)
                .hint_text("новий текст анотації"),
        );
        ui.horizontal(|ui| {
            if ui.button(egui::RichText::new("💾 Зберегти").strong()).clicked() {
                let t = app.reader.as_ref().unwrap().edit_text.clone();
                app.edit_annotation(id, t);
                let r = app.reader.as_mut().unwrap();
                r.edit_id = None;
                r.edit_text.clear();
            }
            if ui.button("скасувати").clicked() {
                let r = app.reader.as_mut().unwrap();
                r.edit_id = None;
                r.edit_text.clear();
            }
        });
    }

    ui.add_space(10.0);
    ui.separator();
    ui.strong("Додати");

    // kind selector
    let mut save = false;
    ui.horizontal(|ui| {
        ui.label("тип:");
        for (i, kind) in ANN_KINDS.iter().enumerate() {
            if ui
                .selectable_label(app.reader.as_ref().unwrap().ann_kind_sel == i, *kind)
                .clicked()
            {
                app.reader.as_mut().unwrap().ann_kind_sel = i;
            }
        }
    });
    ui.add_sized(
        [ui.available_width(), 54.0],
        egui::TextEdit::multiline(&mut app.reader.as_mut().unwrap().ann_text)
            .hint_text("текст коментаря / виділення; закладка — текст не потрібен"),
    );
    if ui
        .button(egui::RichText::new("💾 Зберегти анотацію").strong())
        .clicked()
    {
        save = true;
    }
    if save {
        let sel = app.reader.as_ref().unwrap().ann_kind_sel;
        let kind_en = ["comment", "bookmark", "highlight"][sel];
        let text = app.reader.as_ref().unwrap().ann_text.clone();
        app.save_annotation(kind_en, &text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_labels_resolves_joins_and_passes_through() {
        let d = vec![(1i64, "Закон".to_string()), (21, "постанова".to_string())];
        assert_eq!(type_labels("1", &d), "Закон");
        assert_eq!(type_labels("1|21", &d), "Закон · постанова");
        assert_eq!(type_labels(" 21 | ", &d), "постанова");
        assert_eq!(type_labels("", &d), "—");
        assert_eq!(type_labels("| |", &d), "—", "only separators is empty");
        assert_eq!(type_labels("999", &d), "id 999", "unknown id");
        assert_eq!(type_labels("закон", &d), "закон", "non-numeric passes through");
    }

    #[test]
    fn reading_stats_format() {
        assert_eq!(reading_stats(""), "");
        assert_eq!(reading_stats("   \n\t "), "", "whitespace is not words");
        let text = "слово ".repeat(401); // 401 words → 2 minutes
        assert_eq!(reading_stats(&text), "401 слів · 2 406 симв. · ~2 хв");
        // Grouping: thousands separated by spaces.
        assert_eq!(group_thousands(0), "0");
        assert_eq!(group_thousands(999), "999");
        assert_eq!(group_thousands(1000), "1 000");
        assert_eq!(group_thousands(1234567), "1 234 567");
        // 1..400 words → ~1 хв.
        assert!(reading_stats(&"слов ".repeat(120)).ends_with("~1 хв"));
    }

    #[test]
    fn zoom_step_clamps_and_normalises_default() {
        // Unset (0) means 100%: one step lands on 90/110.
        assert_eq!(zoom_step(0, -10), 90);
        assert_eq!(zoom_step(0, 10), 110);
        // A set value steps from itself.
        assert_eq!(zoom_step(130, -10), 120);
        assert_eq!(zoom_step(130, 10), 140);
        // Clamped at both ends.
        assert_eq!(zoom_step(60, -10), 60);
        assert_eq!(zoom_step(65, -10), 60);
        assert_eq!(zoom_step(250, 10), 250);
        assert_eq!(zoom_step(245, 10), 250);
        // A zero delta normalises the unset value to 100.
        assert_eq!(zoom_step(0, 0), 100);
        assert_eq!(zoom_step(180, 0), 180);
    }

    #[test]
    fn cmp_export_body_has_header_and_lines() {
        let lines = vec!["A · поточна ред.: 1000 слів".to_string(), "різниця: +120 слів".to_string()];
        let body = cmp_export_body(&lines, "2755-17", None, Some("20230505"));
        let mut it = body.lines();
        assert_eq!(it.next().unwrap(), "Порівняння редакцій 2755-17");
        assert_eq!(it.next().unwrap(), "A: поточна ред.");
        assert_eq!(it.next().unwrap(), "B: ред. 05.05.2023");
        assert_eq!(it.next().unwrap(), "");
        assert_eq!(it.next().unwrap(), "A · поточна ред.: 1000 слів");
        assert_eq!(it.next().unwrap(), "різниця: +120 слів");
        assert_eq!(it.next(), None, "body ends with the last line + newline");
        // both dated
        let body2 = cmp_export_body(&[], "1-23", Some("20200101"), Some("20210202"));
        assert!(body2.contains("A: ред. 01.01.2020"));
        assert!(body2.contains("B: ред. 02.02.2021"));
    }

    #[test]
    fn cycle_edition_walks_combo_with_wrap() {
        let eds = || vec!["20200101".to_string(), "20220505".to_string()];
        // down from поточна → newest listed, → older, → wrap to поточна
        assert_eq!(cycle_edition(&eds(), None, 1), Some(Some("20200101".into())));
        assert_eq!(cycle_edition(&eds(), Some("20200101"), 1), Some(Some("20220505".into())));
        assert_eq!(cycle_edition(&eds(), Some("20220505"), 1), Some(None));
        // up mirrors down
        assert_eq!(cycle_edition(&eds(), None, -1), Some(Some("20220505".into())));
        assert_eq!(cycle_edition(&eds(), Some("20220505"), -1), Some(Some("20200101".into())));
        assert_eq!(cycle_edition(&eds(), Some("20200101"), -1), Some(None));
        // single-entry list (only поточна): nothing to do
        assert_eq!(cycle_edition(&[], None, 1), None);
        assert_eq!(cycle_edition(&[], None, -1), None);
        // edition not in the list (stale selection): no crash, no move
        assert_eq!(cycle_edition(&eds(), Some("19990101"), 1), None);
    }

    #[test]
    fn toggle_raw_mode_flips_with_labels() {
        assert_eq!(toggle_raw_mode(false), (true, "простий текст"));
        assert_eq!(toggle_raw_mode(true), (false, "форматований текст"));
    }

    #[test]
    fn scroll_pct_clamps_and_detects_unscrollable() {
        // Nothing to scroll → always 0.
        assert_eq!(scroll_pct(0.0, 500.0, 800.0), 0);
        assert_eq!(scroll_pct(30.0, 800.0, 800.0), 0, "span <= 1 px is not scrollable");
        // Quarter / halfway / end of a 10 000 px span.
        assert_eq!(scroll_pct(2_500.0, 10_800.0, 800.0), 25);
        assert_eq!(scroll_pct(5_000.0, 10_800.0, 800.0), 50);
        assert_eq!(scroll_pct(10_000.0, 10_800.0, 800.0), 100);
        // Overshoot (bounce/kinetic) clamps to 100.
        assert_eq!(scroll_pct(10_800.0, 10_800.0, 800.0), 100);
        // Never negative, never over 100.
        assert_eq!(scroll_pct(-40.0, 10_800.0, 800.0), 0);
        assert_eq!(scroll_pct(99_000.0, 10_800.0, 800.0), 100);
    }

    #[test]
    fn top_button_shown_from_five_percent() {
        assert!(!top_button_shown(0));
        assert!(!top_button_shown(4));
        assert!(top_button_shown(5));
        assert!(top_button_shown(100));
    }

    #[test]
    fn citation_line_matches_the_web_format() {
        // Current edition: no edition clause.
        assert_eq!(
            citation_line("Про освіту", "2041-15", None),
            "«Про освіту» // Офіційний вебпортал парламенту України. \
             URL: https://zakon.rada.gov.ua/laws/show/2041-15"
        );
        // Dated edition: dd.mm.yyyy clause before the URL.
        assert_eq!(
            citation_line("Податковий кодекс", "2755-17", Some("20241115")),
            "«Податковий кодекс» // Офіційний вебпортал парламенту України. \
             Редакція від 15.11.2024, URL: https://zakon.rada.gov.ua/laws/show/2755-17"
        );
        // Blank/whitespace edition behaves like None.
        assert!(citation_line("X", "1-1", Some("   ")).contains("України. URL:"));
        // Non-8-digit raw labels pass through (fmt_date contract).
        assert!(citation_line("X", "1-1", Some("чернетка")).contains("Редакція від чернетка,"));
    }

    #[test]
    fn edition_compare_lines_word_char_and_signed_diff() {
        // Per-edition line mirrors the web cmpSummaryLine wording.
        assert_eq!(
            cmp_word_line("поточна ред.", "a b c"),
            "«поточна ред.»: 3 слів · 5 симв. · ~1 хв"
        );
        assert_eq!(
            cmp_word_line("ред. 01.01.2020", &"слов ".repeat(401)),
            "«ред. 01.01.2020»: 401 слів · 2 005 симв. · ~2 хв"
        );
        assert_eq!(cmp_word_line("x", "   \n "), "«x»: 0 слів · 5 симв. · ~1 хв");
        // Difference is B minus A, signed.
        assert_eq!(cmp_diff_line(10, 10), "різниця: +0 слів");
        assert_eq!(cmp_diff_line(10, 17), "різниця: +7 слів");
        assert_eq!(cmp_diff_line(17, 10), "різниця: -7 слів");
    }

    #[test]
    fn cmp_report_lists_both_editions_then_the_difference() {
        let r = cmp_report("A", "один два три", "B", "один два");
        assert_eq!(r.len(), 3);
        assert!(r[0].starts_with("«A»"), "{:?}", r);
        assert!(r[1].starts_with("«B»"), "{:?}", r);
        assert_eq!(r[2], "різниця: -1 слів");
        // empty inputs still render three lines
        assert_eq!(cmp_report("A", "", "B", "").len(), 3);
    }

    #[test]
    fn toc_helpers() {
        let text = "ЗАКОН УКРАЇНИ\nПро тест\n\
                    РОЗДІЛ 1. ЗАГАЛЬНІ ПОЛОЖЕННЯ\n\
                    Стаття 1. Визначення\n\
                    1. текст\n\
                    Глава 2. Інше\n\
                    Стаття 2. Щось ще дуже довге ".to_string()
            + &"символів ".repeat(30);
        let paras = paragraphs(&text);
        let toc = toc_entries(&paras);
        // 2 sections + 2 articles = 4 entries, in document order.
        assert_eq!(toc.len(), 4, "{toc:?}");
        assert!(toc[0].1.starts_with("РОЗДІЛ"));
        assert!(toc[1].1.starts_with("Стаття 1"));
        assert!(toc_is_section(&toc[0].1) && !toc_is_section(&toc[1].1));
        // Indices point at the classified paragraphs.
        assert_eq!(paras[toc[1].0].kind, PKind::Article);
        // Label: short titles pass through; long ones clip at 90 chars + ellipsis.
        assert_eq!(toc_label("Стаття 1. Коротка"), "Стаття 1. Коротка");
        let long: String = "а".repeat(120);
        let lab = toc_label(&long);
        assert_eq!(lab.chars().count(), 91, "90 chars + ellipsis");
        assert!(lab.ends_with('…'));
    }

    #[test]
    fn find_spans_case_insensitive_cyrillic() {
        let p = "Кодекс про НАШУ та кодекс законів";
        let v = find_spans(p, "КОДЕКС");
        assert_eq!(v.len(), 2, "two case-insensitive hits");
        assert_eq!(&p[v[0].0..v[0].1], "Кодекс");
        assert_eq!(&p[v[1].0..v[1].1], "кодекс");
        // slicing stays on valid char boundaries
        assert!(v.iter().all(|(a, b)| p.is_char_boundary(*a) && p.is_char_boundary(*b)));
    }

    #[test]
    fn find_spans_empty_query_yields_nothing() {
        assert!(find_spans("будь-що", "   ").is_empty());
    }

    #[test]
    fn html_export_escapes_and_marks() {
        let mut r = ReaderState::new(rada_db::SearchHit {
            nreg: "1-1".into(),
            nazva: "Тест".into(),
            status: 5,
            org_date: "20240101".into(),
            org_num: "1".into(),
            poddat: String::new(),
        });
        r.text = Some("а < б & в; виділення тут; ще виділення".into());
        r.annotations = vec![rada_db::Annotation {
            id: 1,
            nreg: "1-1".into(),
            kind: "highlight".into(),
            text: "виділення".into(),
            span_start: None,
            span_end: None,
            created_at: "2026-01-01 00:00:00".into(),
        }];
        let s = r.export_doc("html");
        assert!(s.contains("а &lt; б &amp; в"), "html escaping");
        assert_eq!(
            s.matches("<mark class=\"hl\">").count(),
            2,
            "both mentions wrapped"
        );
        assert!(s.ends_with("</html>\n"), "complete document");
    }

    #[test]
    fn plain_print_has_no_classes_marks_or_gold() {
        let mut r = ReaderState::new(rada_db::SearchHit {
            nreg: "2-2".into(),
            nazva: "Тест".into(),
            status: 5,
            org_date: "20240101".into(),
            org_num: "2".into(),
            poddat: String::new(),
        });
        r.text = Some("Стаття 1. Текст\n<safe> & <b>виділення</b>".into());
        r.annotations = vec![rada_db::Annotation {
            id: 1,
            nreg: "2-2".into(),
            kind: "highlight".into(),
            text: "виділення".into(),
            span_start: None,
            span_end: None,
            created_at: "2026-01-01 00:00:00".into(),
        }];
        let s = r.print_doc_plain();
        assert!(!s.contains("<mark"), "no highlights in the plain variant");
        assert!(!s.contains("class="), "no structural classes at all");
        assert!(!s.contains("#8a6d00") && !s.contains("#c9a227"), "no gold accents");
        assert!(s.contains("&lt;safe&gt; &amp; &lt;b&gt;"), "escaping kept");
        assert!(s.contains("<p>Стаття 1. Текст</p>"), "one <p> per line");
        assert!(s.contains("Рег. №:"), "header meta kept");
        assert!(s.contains("<li><b>виділення</b>"), "annotation footer kept");
        assert!(s.ends_with("</html>\n"), "complete document");
    }

    #[test]
    fn plain_print_without_annotations_omits_footer() {
        let mut r = ReaderState::new(rada_db::SearchHit {
            nreg: "3-3".into(),
            nazva: "Порожньо".into(),
            status: 1,
            org_date: String::new(),
            org_num: String::new(),
            poddat: String::new(),
        });
        r.text = Some("тільки текст".into());
        let s = r.print_doc_plain();
        assert!(!s.contains("<footer>"), "no footer without annotations");
        assert!(s.contains("<p>тільки текст</p>"));
    }

    #[test]
    fn tab_label_truncates_and_falls_back_to_nreg() {
        assert_eq!(tab_label(" Про освіту ", "1-1"), "Про освіту");
        assert_eq!(tab_label("   ", "16-15"), "16-15", "blank title falls back");
        assert_eq!(tab_label("", "16-15"), "16-15");
        let long = "а".repeat(50);
        let l = tab_label(&long, "n");
        assert_eq!(l.chars().count(), 37, "36 chars + ellipsis");
        assert!(l.ends_with('…'));
        // exactly 36 chars stay untruncated
        let edge = "б".repeat(36);
        assert_eq!(tab_label(&edge, "n"), edge);
    }

    #[test]
    fn paragraphs_classify_structure() {
        let t = "ЗАКОН УКРАЇНИ\n\
                 Про вищу освіту\n\
                 (Відомості Верховної Ради (ВВР), 2014, № 37-38, ст.2004)\n\
                 {Із змінами, внесеними згідно із Законами\n\
                 № 76-VIII від 28.12.2014, ВВР, 2015, № 6, ст.40\n\
                 }\n\
                 РОЗДІЛ 1\n\
                 Загальні засади\n\
                 Стаття 1. Визначення понять\n\
                 1. У цьому Законі діють такі терміни:\n\
                 2) другий пункт\n\
                 звичайний текст";
        let p = paragraphs(t);
        let kinds: Vec<PKind> = p.iter().map(|x| x.kind).collect();
        assert_eq!(
            kinds,
            vec![
                PKind::DocTitle,
                PKind::Subtitle,
                PKind::Meta,
                PKind::Meta,
                PKind::Meta,
                PKind::Meta,
                PKind::Section,
                PKind::Plain,
                PKind::Article,
                PKind::Num,
                PKind::Num,
                PKind::Plain,
            ]
        );
        assert_eq!(p[0].text, "ЗАКОН УКРАЇНИ");
        assert_eq!(p[9].text, "1. У цьому Законі діють такі терміни:");
    }

    #[test]
    fn styled_hits_win_over_annotations_and_cover_everything() {
        // each Cyrillic letter is 2 bytes here
        let p = "абвгдежз";
        let segs = styled(p, &[(2, 8)], &[(4, 10, Seg::Hit)]);
        // the segments must tile the paragraph exactly…
        let joined: String = segs.iter().map(|(_, t)| (*t).to_string()).collect();
        assert_eq!(joined, p);
        // …with hits beating ann where they overlap
        let expect = vec![
            (Seg::Plain, "а"),
            (Seg::Ann, "б"),
            (Seg::Hit, "вг"),
            (Seg::Hit, "д"),
            (Seg::Plain, "ежз"),
        ];
        assert_eq!(segs, expect);
    }

    #[test]
    fn clean_selection_none_on_empty_or_whitespace() {
        assert_eq!(clean_selection("", 500), None);
        assert_eq!(clean_selection("   \n\t", 500), None);
        assert_eq!(clean_selection("\u{a0} \n", 500), None, "nbsp + blanks");
    }

    #[test]
    fn clean_selection_collapses_whitespace_runs() {
        assert_eq!(
            clean_selection("  Стаття   1.\n\tвизначає  поняття \n", 500).as_deref(),
            Some("Стаття 1. визначає поняття")
        );
    }

    #[test]
    fn clean_selection_truncates_by_chars_not_bytes() {
        // 600 Cyrillic chars (1200 bytes) must cap at exactly 500 *chars*.
        let raw = "а".repeat(600);
        let out = clean_selection(&raw, 500).unwrap();
        assert_eq!(out.chars().count(), 500);
        assert_eq!(out, "а".repeat(500));
        // short text passes through untouched
        assert_eq!(clean_selection("абв", 500).as_deref(), Some("абв"));
    }

    #[test]
    fn ann_appendix_shapes() {
        assert!(ann_appendix(&[], true).is_empty());
        let mk = |kind: &str, text: &str| rada_db::Annotation {
            id: 1,
            nreg: "1-1".into(),
            kind: kind.into(),
            text: text.into(),
            span_start: None,
            span_end: None,
            created_at: "2026-10-27 12:00".into(),
        };
        let md = ann_appendix(&[mk("highlight", "щось"), mk("comment", "")], true);
        assert!(md.contains("Примітки та виділення (2)"));
        assert!(md.contains("виділення"));
        assert!(md.contains("коментар"));
        assert!(md.contains("—")); // empty comment rendered as dash
    }
}
