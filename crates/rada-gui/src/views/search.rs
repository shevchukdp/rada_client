//! Search page: query + filters + results list + name resolver.

use eframe::egui;

use crate::app::{err, gold, App};

pub fn show(app: &mut App, ctx: &egui::Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
        // ---- filter row -------------------------------------------
        egui::Grid::new("search_filters")
            .num_columns(4)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Заголовок");
                let q = ui.add_sized(
                    [340.0, 26.0],
                    egui::TextEdit::singleline(&mut app.search.query)
                        .hint_text("2+ слів → FTS5, 1 слово → підстрока"),
                );
                if q.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    app.run_search();
                }
                ui.label("Номер:");
                let _num = ui.add_sized(
                    [110.0, 26.0],
                    egui::TextEdit::singleline(&mut app.search.f_number).hint_text("2041"),
                );
                ui.end_row();

                ui.label("Стан");
                status_combo(app, ui);
                ui.label("Тип");
                typs_combo(app, ui);
                ui.label("Дата з / по");
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [96.0, 24.0],
                        egui::TextEdit::singleline(&mut app.search.f_date_from)
                            .hint_text("20240101"),
                    );
                    ui.add_sized(
                        [96.0, 24.0],
                        egui::TextEdit::singleline(&mut app.search.f_date_to).hint_text("20241231"),
                    );
                });
                ui.end_row();
            });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let find = ui.add(
                egui::Button::new(egui::RichText::new("🔎 Знайти").strong().color(gold())),
            );
            if find.clicked() {
                app.run_search();
            }
            if app.search.loading {
                ui.spinner();
            }
            if app.search.searched && !app.search.loading {
                if app.search.total > 0 {
                    ui.label(
                        egui::RichText::new(format!("збігів: {}", app.search.total)).small(),
                    );
                } else if app.search.error.is_none() {
                    ui.label(egui::RichText::new("збігів немає (перевірте фільтри)").small());
                }
            }
            if let Some(e) = &app.search.error {
                ui.label(egui::RichText::new(e).small().color(err()));
            }
        });

        ui.add_space(8.0);
        ui.separator();

        // ---- export the current results page (offline) --------------
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Експорт списку (поточна сторінка):").small());
            let has = !app.search.hits.is_empty();
            if ui.add_enabled(has, egui::Button::new("Експорт .md")).clicked() {
                app.export_search_list(true);
            }
            if ui.add_enabled(has, egui::Button::new("Експорт .txt")).clicked() {
                app.export_search_list(false);
            }
        });
        ui.add_space(6.0);

        // ---- results ------------------------------------------------
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let hits = app.search.hits.clone();
                if hits.is_empty() && !app.search.loading {
                    if !app.search.searched {
                        ui.label("Введіть запит і натисніть «Знайти».");
                    } else if app.search.error.is_none() && app.search.total == 0 {
                        ui.label("Нічого не знайдено. Перевірте фільтри або спростіть запит.");
                    }
                }
                for (i, h) in hits.clone().into_iter().enumerate() {
                    let meta = format!(
                        "{}   №{} від {}   [{}]   ред. {}",
                        h.nreg,
                        h.org_num,
                        fmt_date(&h.org_date),
                        stan_label(&app.dicts.stan, h.status),
                        fmt_date(&h.poddat),
                    );
                    let avail = (ui.available_width() - 56.0).max(100.0);
                    let text =
                        egui::RichText::new(format!("{}\n{}", h.nazva, meta)).small();
                    ui.horizontal(|ui| {
                        if ui
                            .add_sized([avail, 46.0], egui::Button::new(text).wrap())
                            .clicked()
                        {
                            app.open_reader_from(hits.clone(), i);
                        }
                        if ui.button(egui::RichText::new("копія").small()).clicked() {
                            ui.ctx().copy_text(h.nreg.clone());
                        }
                    });
                }

                // paging
                if !hits.is_empty() {
                    let (total, offset, limit) =
                        (app.search.total, app.search.offset(), app.search.limit as i64);
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let shown = offset + hits.len() as i64;
                        ui.label(
                            egui::RichText::new(format!(
                                "показано {}–{} із {total}",
                                offset + 1,
                                shown
                            ))
                            .small(),
                        );
                        if offset > 0 && ui.button("← попередні").clicked() {
                            app.search.set_offset(((offset - limit).max(0)) as usize);
                            app.run_search();
                        }
                        if shown < total && ui.button("далі →").clicked() {
                            app.search.set_offset((offset + limit) as usize);
                            app.run_search();
                        }
                    });
                }
            });

        // ---- resolver -----------------------------------------------
        ui.add_space(10.0);
        ui.collapsing("Знайти документ за назвою (наживо, find/a)", |ui| {
            ui.horizontal(|ui| {
                let inp = ui.add_sized(
                    [ui.available_width() - 130.0, 26.0],
                    egui::TextEdit::singleline(&mut app.search.resolve_input)
                        .hint_text("фрагмент назви документа"),
                );
                let btn = ui.button("Знайти nreg");
                if btn.clicked()
                    || (inp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                {
                    app.run_resolve();
                }
                if app.search.resolve_loading {
                    ui.spinner();
                }
            });
            if let Some(r) = &app.search.resolve_result.clone() {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("nreg:").small());
                    ui.label(egui::RichText::new(r).strong().color(gold()));
                    if ui
                        .button(egui::RichText::new("відкрити у читалці").small())
                        .clicked()
                    {
                        let rr = r.clone();
                        app.open_reader_by_nreg(&rr, "(знайдено через find/a)", 0, "", "", "");
                    }
                });
            }
        });
    });
}

fn status_combo(app: &mut App, ui: &mut egui::Ui) {
    let opts = app
        .dicts
        .stan
        .clone();
    let cur = app.search.f_status;
    let label = cur
        .map(|id| opt_label(&opts, id, "стан"))
        .unwrap_or_else(|| "усі".into());
    let sel = egui::ComboBox::from_id_salt("f_status")
        .width(200.0)
        .selected_text(label);
    sel.show_ui(ui, |ui| {
        if ui
            .selectable_label(cur.is_none(), "усі")
            .clicked()
        {
            app.search.f_status = None;
        }
        for (id, lab) in &opts {
            if ui
                .selectable_label(cur == Some(*id), lab)
                .clicked()
            {
                app.search.f_status = Some(*id);
            }
        }
    });
}

fn typs_combo(app: &mut App, ui: &mut egui::Ui) {
    let opts = app.dicts.typs.clone();
    let cur = app.search.f_typ;
    let label = cur
        .map(|id| opt_label(&opts, id, "тип"))
        .unwrap_or_else(|| "усі".into());
    let sel = egui::ComboBox::from_id_salt("f_typ")
        .width(160.0)
        .selected_text(label);
    sel.show_ui(ui, |ui| {
        if ui.selectable_label(cur.is_none(), "усі").clicked() {
            app.search.f_typ = None;
        }
        for (id, lab) in &opts {
            if ui.selectable_label(cur == Some(*id), lab).clicked() {
                app.search.f_typ = Some(*id);
            }
        }
    });
}

fn opt_label(opts: &[(i64, String)], id: i64, kind: &str) -> String {
    opts.iter()
        .find(|(tid, _)| *tid == id)
        .map(|(_, l)| l.clone())
        .unwrap_or_else(|| format!("{kind} {id}"))
}

fn stan_label(stan: &[(i64, String)], id: i64) -> String {
    if stan.is_empty() {
        return match id {
            0 => "невизначено".into(),
            1 => "втратив чинність".into(),
            2 => "набирає чинності".into(),
            3 => "дію зупинено".into(),
            4 => "дію відновлено".into(),
            5 => "чинний".into(),
            6 => "не набрав чинності".into(),
            7 => "не застосовується".into(),
            8 => "частково набрав".into(),
            9 => "втратив частково".into(),
            _ => format!("статус {id}"),
        };
    }
    opt_label(stan, id, "стан")
}

/// `YYYYMMDD` → `DD.MM.YYYY`; passthrough when not 8 digits.
pub fn fmt_date(d: &str) -> String {
    if d.len() == 8 && d.chars().all(|c| c.is_ascii_digit()) {
        format!("{}.{}.{}", &d[6..8], &d[4..6], &d[0..4])
    } else if d.is_empty() {
        "—".into()
    } else {
        d.to_string()
    }
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            run: 0,
            query: String::new(),
            f_status: None,
            f_typ: None,
            f_date_from: String::new(),
            f_date_to: String::new(),
            f_number: String::new(),
            offset_field: 0,
            limit: 50,
            hits: Vec::new(),
            total: 0,
            loading: false,
            searched: false,
            error: None,
            resolve_input: String::new(),
            resolve_run: 0,
            resolve_loading: false,
            resolve_result: None,
            invalid_date: false,
        }
    }
}

/// Search page state + query builder.
pub struct SearchState {
    pub run: u64,
    pub query: String,
    pub f_status: Option<i64>,
    pub f_typ: Option<i64>,
    pub f_date_from: String,
    pub f_date_to: String,
    pub f_number: String,
    pub offset_field: usize,
    pub limit: usize,
    pub hits: Vec<rada_db::SearchHit>,
    pub total: i64,
    pub loading: bool,
    pub searched: bool,
    pub error: Option<String>,
    pub resolve_input: String,
    pub resolve_run: u64,
    pub resolve_loading: bool,
    pub resolve_result: Option<String>,
    pub invalid_date: bool,
}

impl SearchState {
    /// Current offset (rows already shown before this page).
    pub fn offset(&self) -> i64 {
        self.offset_field as i64
    }
    pub fn set_offset(&mut self, v: usize) {
        self.offset_field = v;
    }

    /// Build the export text (md or txt) of the current results page.
    pub fn export_list(&self, md: bool) -> String {
        use rada_core::models::StatusId;
        let q = self.query.trim();
        let q_label = if q.is_empty() { "усі" } else { q };
        if md {
            let mut s = String::new();
            s.push_str(&format!(
                "# Пошук: «{q_label}» — {} док. (поточна сторінка з {total})\n\n",
                self.hits.len(),
                total = self.total
            ));
            for h in &self.hits {
                s.push_str(&format!("## {} — №{}\n\n", h.nazva, h.nreg));
                s.push_str(&format!(
                    "- **Стан:** {}\n- **Номер:** {} від {}\n- **Ред.:** {}\n\n",
                    StatusId(h.status).label(),
                    h.org_num,
                    fmt_date(&h.org_date),
                    fmt_date(&h.poddat),
                ));
            }
            s
        } else {
            let mut s = String::new();
            s.push_str(&format!(
                "Пошук: «{q_label}» — {} док. (поточна сторінка з {total})\n\n",
                self.hits.len(),
                total = self.total
            ));
            for h in &self.hits {
                s.push_str(&format!(
                    "{} — №{}\nСтан: {} · №{} від {} · ред. {}\n\n",
                    h.nazva,
                    h.nreg,
                    StatusId(h.status).label(),
                    h.org_num,
                    fmt_date(&h.org_date),
                    fmt_date(&h.poddat),
                ));
            }
            s
        }
    }

    /// Map the UI state to an index query; validates date inputs.
    pub fn build_query(&mut self) -> rada_db::search::SearchQuery {
        let mut invalid = false;
        let parse = |s: &String, invalid: &mut bool| -> Option<String> {
            let t = s.trim();
            if t.is_empty() {
                None
            } else if t.len() == 8 && t.chars().all(|c| c.is_ascii_digit()) {
                Some(t.to_string())
            } else {
                *invalid = true;
                None
            }
        };
        let q = rada_db::search::SearchQuery {
            nazva: {
                let t = self.query.trim();
                if t.is_empty() { None } else { Some(t.to_string()) }
            },
            org_id: None,
            typ: self.f_typ,
            status: self.f_status,
            date_from: parse(&self.f_date_from, &mut invalid),
            date_to: parse(&self.f_date_to, &mut invalid),
            doc_number: {
                let t = self.f_number.trim();
                if t.is_empty() { None } else { Some(t.to_string()) }
            },
            limit: self.limit,
            offset: self.offset_field,
        };
        self.invalid_date = invalid;
        q
    }
}
