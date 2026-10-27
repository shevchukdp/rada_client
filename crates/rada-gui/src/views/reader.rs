//! Reader page: document header, edition switcher, live text, annotations.

use eframe::egui;
use rada_core::models::{LawCardJson, StatusId};

use crate::app::{err, gold, App};
use crate::views::search::fmt_date;

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
}

pub const ANN_KINDS: [&str; 3] = ["Коментар", "Закладка", "Виділення"];

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
        }
    }

    /// Character span of a text selection (reserved for a future
    /// "highlight from selection" flow; the current UI saves typed snippets).
    pub fn selected_span(&self) -> Option<(i64, i64)> {
        None
    }

    /// Export the current document as Markdown or plain text.
    pub fn export_doc(&self, md: bool) -> String {
        let title = self
            .card
            .as_ref()
            .map(|c| c.nazva.clone())
            .unwrap_or_else(|| self.hit.nazva.clone());
        let text = self.text.clone().unwrap_or_default();
        if md {
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

    egui::CentralPanel::default().show(ctx, |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                header(app, ui);
                ui.add_space(6.0);
                edition_row(app, ui);
                ui.add_space(8.0);
                ui.separator();
                text_block(app, ui);
            });
    });
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
        egui::RichText::new(&title).strong().color(gold()).size(19.0),
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
        // paging through the results list this document was opened from
        if let Some((len, pos)) = nav_info {
            if len > 1 {
                ui.separator();
                if ui.button("‹").clicked() {
                    nav_req = Some(-1);
                }
                ui.label(egui::RichText::new(format!("{pos} / {len}")).small());
                if ui.button("›").clicked() {
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
        5 => ("ЧИННИЙ", egui::Color32::from_rgb(0x9A, 0xCC, 0x80)),
        1 => ("ВТРАТИВ ЧИННІСТЬ", egui::Color32::from_rgb(0xE6, 0x6A, 0x6A)),
        2 => ("НАБИРАЄ ЧИННОСТІ", egui::Color32::from_rgb(0xFF, 0xB3, 0x00)),
        3 => ("ДІЮ ЗУПИНЕНО", egui::Color32::from_rgb(0xFF, 0xB3, 0x00)),
        4 => ("ДІЮ ВІДНОВЛЕНО", egui::Color32::from_rgb(0x7A, 0xC7, 0xE4)),
        8 => ("ЧАСТКОВО НАБРАВ", egui::Color32::from_rgb(0x7A, 0xC7, 0xE4)),
        6 => ("НЕ НАБРАВ ЧИННОСТІ", egui::Color32::from_rgb(0xAA, 0xAA, 0xAA)),
        _ => ("НЕВИЗНАЧЕНО", egui::Color32::from_rgb(0xAA, 0xAA, 0xAA)),
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

    // card details (from live card)
    if let Some(c) = app.reader.as_ref().unwrap().card.clone() {
        ui.collapsing("Картка документа (наживо)", |ui| {
            egui::Grid::new("card_meta").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
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
            app.export_reader(true);
        }
        if ui.button("⬇ Експорт .txt").clicked() {
            app.export_reader(false);
        }
        if loading {
            ui.spinner();
            ui.label(
                egui::RichText::new("текст документа — живий запит (пауза між запитами 5 с)...")
                    .small(),
            );
        }
    });

    if loading {
        return;
    }

    if let Some(t) = &text {
        let paras = paragraphs(t);

        // ---- find in text --------------------------------------------
        let qlab = app.reader.as_ref().unwrap().find.trim().to_string();
        let mut g_hits: Vec<(usize, (usize, usize))> = Vec::new();
        if !qlab.is_empty() {
            for (pi, p) in paras.iter().enumerate() {
                for (a, b) in find_spans(p, &qlab) {
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
            if ui.button("»").clicked() || enter {
                find_req = Some(1);
            }
            if ui.button("«").clicked() {
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
        ui.add_space(6.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            for (pi, para) in paras.iter().enumerate() {
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
                let ann = ann_spans(para, &snippets);
                ui.horizontal_wrapped(|ui| {
                    for (seg, part) in styled(para, &ann, &p_hits) {
                        match seg {
                            Seg::Plain => {
                                ui.label(part);
                            }
                            Seg::Ann => {
                                ui.label(
                                    egui::RichText::new(part)
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
                // "→ текст": scroll the first paragraph holding the annotation.
                if let Some(j) = &jump {
                    if !j.is_empty() && para.contains(j.as_str()) {
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
            }
            ui.add_space(40.0);
        });
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

fn paragraphs(t: &str) -> Vec<String> {
    t.split("\n\n")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.replace('\n', " "))
        .collect()
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
                "bookmark" => ("закладка", egui::Color32::from_rgb(0x7A, 0xC7, 0xE4)),
                _ => ("коментар", egui::Color32::GRAY),
            };
            egui::Frame::none()
                .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(0x33, 0x3F, 0x5A)))
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
                        if ui.button(egui::RichText::new("✕ видалити").small()).clicked() {
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
