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
        if !org_num.is_empty() {
            ui.label(egui::RichText::new(format!("№{org_num}")).small());
        }
        if !org_date.is_empty() {
            ui.label(egui::RichText::new(format!("від {}", fmt_date(&org_date))).small());
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
    let _ = app;
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
    let (text, t_err, loading) = {
        let r = app.reader.as_ref().unwrap();
        (r.text.clone(), r.text_error.clone(), r.text_loading)
    };

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
        ui.add_space(6.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            for para in paragraphs(t) {
                ui.add_sized(
                    [ui.available_width(), 12.0],
                    egui::Label::new(para).selectable(true),
                );
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
                    });
                    ui.label(egui::RichText::new(&a.text));
                    if ui.button(egui::RichText::new("✕ видалити").small()).clicked() {
                        del = Some(a.id);
                    }
                });
        }
        if let Some(id) = del {
            app.delete_annotation(id);
        }
    });

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
