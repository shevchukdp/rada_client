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
                let want_q = app.take_focus(crate::app::FocusReq::SearchQuery);
                let q = ui.add_sized(
                    [340.0, 26.0],
                    egui::TextEdit::singleline(&mut app.search.query)
                        .hint_text("2+ слів → FTS5, 1 слово → підстрока"),
                );
                if want_q {
                    q.request_focus();
                }
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
                    if ui
                        .button("…")
                        .on_hover_text("відкрити календар")
                        .clicked()
                    {
                        open_calendar(app, PickKind::From);
                    }
                    ui.add_sized(
                        [96.0, 24.0],
                        egui::TextEdit::singleline(&mut app.search.f_date_to)
                            .hint_text("20241231"),
                    );
                    if ui
                        .button("…")
                        .on_hover_text("відкрити календар")
                        .clicked()
                    {
                        open_calendar(app, PickKind::To);
                    }
                    for (label, days, tip) in [
                        ("7 днів", 7u32, "останні 7 днів від найсвіжіших даних"),
                        ("місяць", 30, "останні 30 днів"),
                        ("рік", 365, "останні 365 днів"),
                    ] {
                        if ui
                            .add(egui::Button::new(egui::RichText::new(label).small()))
                            .on_hover_text(tip)
                            .clicked()
                        {
                            let (from, to) = preset_range(data_today(app), days);
                            app.search.f_date_from = from;
                            app.search.f_date_to = to;
                        }
                    }
                });
                ui.end_row();

                ui.label("Видавник");
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [150.0, 24.0],
                        egui::TextEdit::singleline(&mut app.search.f_org_q)
                            .hint_text("фільтр назв видавця…"),
                    );
                    org_combo(app, ui);
                    if app.search.f_org.is_some() {
                        if ui.button(egui::RichText::new("скинути").small()).clicked() {
                            app.search.f_org = None;
                            app.search.f_org_label.clear();
                        }
                    }
                });
                ui.end_row();
            });

        ui.add_space(8.0);
        // ---- recent queries: one-click repeat of yesterday's searches ------
        if !app.settings.recent_q.is_empty() {
            let mut pick: Option<String> = None;
            let mut clear_hist = false;
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new("недавні запити:")
                        .small()
                        .color(crate::themes::weak()),
                );
                for q in app.settings.recent_q.iter().take(6) {
                    if ui
                        .button(egui::RichText::new(q).small())
                        .on_hover_text("повторити пошук")
                        .clicked()
                    {
                        pick = Some(q.clone());
                    }
                }
                if ui
                    .button(egui::RichText::new("очистити").small())
                    .on_hover_text("прибрати історію запитів")
                    .clicked()
                {
                    clear_hist = true;
                }
            });
            if let Some(q) = pick {
                app.search.query = q;
                app.run_search();
            }
            if clear_hist {
                app.settings.recent_q.clear();
                crate::settings::save(&app.settings);
            }
            ui.add_space(4.0);
        }
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
            ui.separator();
            if ui
                .button(egui::RichText::new("Знайти в кеші").small())
                .on_hover_text(
                    "повнотекстовий пошук у кешованих тілах документів \
                     (офлайн, без мережі; кеш поповнюється при читанні)",
                )
                .clicked()
            {
                app.run_cache_search();
            }
            ui.separator();
            let q = app.search.query.trim().to_string();
            let url = portal_search_url(&q);
            if ui
                .button(egui::RichText::new("Розширений пошук на порталі").small())
                .on_hover_text("відкрити розширений пошук zakon.rada.gov.ua у браузері")
                .clicked()
            {
                ui.ctx().open_url(egui::OpenUrl::new_tab(&url));
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
            if let Some((d, t)) = app.search.live_export {
                ui.spinner();
                ui.label(
                    egui::RichText::new(format!("живий експорт текстів: {d} / {t}"))
                        .small()
                        .color(gold()),
                );
            } else {
                if ui.add_enabled(has, egui::Button::new("Тексти .md")).clicked() {
                    app.export_search_list_live(true);
                }
                if ui.add_enabled(has, egui::Button::new("Тексти .txt")).clicked() {
                    app.export_search_list_live(false);
                }
                if has {
                    ui.label(
                        egui::RichText::new("(тексти наживо, ~5 с/док; додано анотації)")
                            .weak()
                            .small(),
                    );
                }
            }
        });
        ui.add_space(4.0);

        // ---- selection → individual numbered files ------------------
        ui.horizontal(|ui| {
            let has = !app.search.hits.is_empty();
            let n = app.search.selected.len();
            ui.label(
                egui::RichText::new(format!("Вибрано для файлів: {n}"))
                    .small()
                    .weak(),
            );
            if ui
                .add_enabled(has, egui::Button::new(egui::RichText::new("вибрати все").small()))
                .clicked()
            {
                app.search.selected = app.search.hits.iter().map(|h| h.nreg.clone()).collect();
            }
            if ui
                .add_enabled(n > 0, egui::Button::new(egui::RichText::new("зняти").small()))
                .clicked()
            {
                app.search.selected.clear();
            }
            let ready = n > 0 && app.search.live_export.is_none();
            if ui
                .add_enabled(ready, egui::Button::new("Вибране у файли .md"))
                .on_hover_text("кожен документ — окремий нумерований файл у вибраній теці (тексти наживо/кеш)")
                .clicked()
            {
                app.export_selected_files(true);
            }
            if ui
                .add_enabled(ready, egui::Button::new("Вибране у файли .txt"))
                .clicked()
            {
                app.export_selected_files(false);
            }
        });
        ui.add_space(6.0);

        // ---- column sort --------------------------------------------
        if app.search.searched {
            use rada_db::search::Sort;
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Сортування:").small().weak());
                let cur = app.search.sort;
                let words = app.search.query.split_whitespace().count();
                let mut resort = false;
                let small_btn = |ui: &mut egui::Ui, lbl: &str, active: bool| {
                    ui.add(
                        egui::Button::new(egui::RichText::new(lbl).small()).selected(active),
                    )
                };
                // Назва (A→Я / Я→A)
                let nazva_active = matches!(cur, Sort::NazvaAsc | Sort::NazvaDesc);
                let lbl = if cur == Sort::NazvaDesc {
                    "Назва ▼"
                } else {
                    "Назва ▲"
                };
                if small_btn(ui, lbl, nazva_active).clicked() {
                    app.search.sort = if cur == Sort::NazvaAsc {
                        Sort::NazvaDesc
                    } else {
                        Sort::NazvaAsc
                    };
                    resort = true;
                }
                // Дата (найновіші / найстаріші); Relevance без FTS = найновіші.
                let date_active = matches!(cur, Sort::DateDesc | Sort::DateAsc)
                    || (cur == Sort::Relevance && words < 2);
                let lbl = if cur == Sort::DateAsc {
                    "Дата ▲"
                } else {
                    "Дата ▼"
                };
                if small_btn(ui, lbl, date_active).clicked() {
                    app.search.sort = if cur == Sort::DateAsc {
                        Sort::DateDesc
                    } else {
                        Sort::DateAsc
                    };
                    resort = true;
                }
                // Редакція (найновіша редакція першою)
                let podd_active = cur == Sort::PoddDesc;
                if small_btn(ui, "Редакція ▼", podd_active).clicked() {
                    app.search.sort = if podd_active {
                        Sort::DateDesc
                    } else {
                        Sort::PoddDesc
                    };
                    resort = true;
                }
                // Релевантність — має сенс лише для FTS (2+ слова).
                if words >= 2 {
                    let rel_active = cur == Sort::Relevance;
                    if small_btn(ui, "Релевантність", rel_active).clicked() {
                        app.search.sort = if rel_active {
                            Sort::DateDesc
                        } else {
                            Sort::Relevance
                        };
                        resort = true;
                    }
                }
                if resort && !app.search.loading {
                    // remember the chosen ordering for the next launch
                    app.settings.sort = app.search.sort.as_str().to_string();
                    crate::settings::save(&app.settings);
                    app.run_search();
                }
            });
            ui.add_space(4.0);
        }

        // ---- facet chips: dictionary-driven one-click filters ------------
        // Advisory counts from `rada_db::facets` for the term the results
        // belong to; clicking a chip sets that filter, clicking the active
        // chip again clears it. Never shown over an empty or failed result.
        if app.search.searched
            && app.search.total > 0
            && (app.search.facets.is_some() || app.search.facets_loading)
            && app.search.query.trim() == app.search.facets_for
        {
            let fc = app.search.facets.clone();
            let stan = app.dicts.stan.clone();
            let typs = app.dicts.typs.clone();
            let organs = app.dicts.organs.clone();
            let (f_status, f_typ, f_org) =
                (app.search.f_status, app.search.f_typ, app.search.f_org);
            let mut pick: Option<FacetPick> = None;
            ui.horizontal_wrapped(|ui| {
                if app.search.facets_loading {
                    ui.spinner();
                }
                if let Some(fc) = &fc {
                    if let Some((id, was)) =
                        facet_group(ui, "Статус:", &fc.status, &stan, f_status)
                    {
                        pick = Some(FacetPick::Status(if was { None } else { Some(id) }));
                    }
                    ui.separator();
                    if let Some((id, was)) = facet_group(ui, "Тип:", &fc.typ, &typs, f_typ) {
                        pick = Some(FacetPick::Typ(if was { None } else { Some(id) }));
                    }
                    ui.separator();
                    if let Some((id, was)) =
                        facet_group(ui, "Видавник:", &fc.org, &organs, f_org)
                    {
                        pick = Some(if was {
                            FacetPick::Org(None, String::new())
                        } else {
                            let label = facet_chips(&fc.org, &organs)
                                .into_iter()
                                .find(|(_, i)| *i == id)
                                .map(|(l, _)| l)
                                .unwrap_or_else(|| format!("id {id}"));
                            FacetPick::Org(Some(id), label)
                        });
                    }
                }
            });
            if let Some(p) = pick {
                match p {
                    FacetPick::Status(v) => app.search.f_status = v,
                    FacetPick::Typ(v) => app.search.f_typ = v,
                    FacetPick::Org(v, label) => {
                        app.search.f_org = v;
                        app.search.f_org_label = label;
                    }
                }
                app.search.set_offset(0);
                app.run_search();
            }
            ui.add_space(4.0);
        }

        // ---- active filters row ------------------------------------------
        // Every set filter gets a chip; clicking it clears that filter and
        // re-runs. «скинути все» clears all of them at once.
        if app.search.searched {
            let stan = app.dicts.stan.clone();
            let typs = app.dicts.typs.clone();
            let organs = app.dicts.organs.clone();
            let (f_status, f_typ, f_org) =
                (app.search.f_status, app.search.f_typ, app.search.f_org);
            let org_label = app.search.f_org_label.clone();
            let (df, dt, num) = (
                app.search.f_date_from.trim().to_string(),
                app.search.f_date_to.trim().to_string(),
                app.search.f_number.trim().to_string(),
            );
            let any = f_status.is_some()
                || f_typ.is_some()
                || f_org.is_some()
                || !df.is_empty()
                || !dt.is_empty()
                || !num.is_empty();
            if any {
                let mut clear = ClearWhat::None;
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("Активні фільтри:").small().weak());
                    if let Some(id) = f_status {
                        let lbl = filter_label(&stan, id);
                        if ui
                            .button(egui::RichText::new(format!("Статус: {lbl} ×")).small())
                            .on_hover_text("прибрати цей фільтр")
                            .clicked()
                        {
                            clear = ClearWhat::Status;
                        }
                    }
                    if let Some(id) = f_typ {
                        let lbl = filter_label(&typs, id);
                        if ui
                            .button(egui::RichText::new(format!("Тип: {lbl} ×")).small())
                            .on_hover_text("прибрати цей фільтр")
                            .clicked()
                        {
                            clear = ClearWhat::Typ;
                        }
                    }
                    if let Some(id) = f_org {
                        let lbl = if org_label.trim().is_empty() {
                            filter_label(&organs, id)
                        } else {
                            org_label.clone()
                        };
                        if ui
                            .button(egui::RichText::new(format!("Видавник: {lbl} ×")).small())
                            .on_hover_text("прибрати цей фільтр")
                            .clicked()
                        {
                            clear = ClearWhat::Org;
                        }
                    }
                    if !df.is_empty() {
                        let lbl = fmt_date(&df);
                        if ui
                            .button(egui::RichText::new(format!("Дата від: {lbl} ×")).small())
                            .on_hover_text("прибрати цей фільтр")
                            .clicked()
                        {
                            clear = ClearWhat::DateFrom;
                        }
                    }
                    if !dt.is_empty() {
                        let lbl = fmt_date(&dt);
                        if ui
                            .button(egui::RichText::new(format!("Дата до: {lbl} ×")).small())
                            .on_hover_text("прибрати цей фільтр")
                            .clicked()
                        {
                            clear = ClearWhat::DateTo;
                        }
                    }
                    if !num.is_empty() {
                        if ui
                            .button(egui::RichText::new(format!("№ {num} ×")).small())
                            .on_hover_text("прибрати цей фільтр")
                            .clicked()
                        {
                            clear = ClearWhat::Number;
                        }
                    }
                    if ui
                        .button(egui::RichText::new("скинути все").small())
                        .on_hover_text("прибрати всі фільтри")
                        .clicked()
                    {
                        clear = ClearWhat::All;
                    }
                });
                match clear {
                    ClearWhat::None => {}
                    ClearWhat::Status => app.search.f_status = None,
                    ClearWhat::Typ => app.search.f_typ = None,
                    ClearWhat::Org => {
                        app.search.f_org = None;
                        app.search.f_org_label.clear();
                    }
                    ClearWhat::DateFrom => app.search.f_date_from.clear(),
                    ClearWhat::DateTo => app.search.f_date_to.clear(),
                    ClearWhat::Number => app.search.f_number.clear(),
                    ClearWhat::All => {
                        app.search.f_status = None;
                        app.search.f_typ = None;
                        app.search.f_org = None;
                        app.search.f_org_label.clear();
                        app.search.f_date_from.clear();
                        app.search.f_date_to.clear();
                        app.search.f_number.clear();
                    }
                }
                if !matches!(clear, ClearWhat::None) {
                    app.search.set_offset(0);
                    app.run_search();
                }
                ui.add_space(4.0);
            }
        }

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
                    let badge = cache_badge(app.search.cached.iter().any(|n| n == &h.nreg));
                    let meta = format!(
                        "{}   №{} від {}   [{}]   ред. {}{}",
                        h.nreg,
                        h.org_num,
                        fmt_date(&h.org_date),
                        stan_label(&app.dicts.stan, h.status),
                        fmt_date(&h.poddat),
                        badge,
                    );
                    let avail = (ui.available_width() - 96.0).max(100.0);
                    let text =
                        egui::RichText::new(format!("{}\n{}", h.nazva, meta)).small();
                    ui.horizontal(|ui| {
                        let mut sel = app.search.selected.contains(&h.nreg);
                        if ui
                            .checkbox(&mut sel, "")
                            .on_hover_text("позначте для експорту в окремі файли")
                            .changed()
                        {
                            if sel {
                                app.search.selected.push(h.nreg.clone());
                            } else {
                                app.search.selected.retain(|n| n != &h.nreg);
                            }
                        }
                        let btn = ui
                            .add_sized([avail, 46.0], egui::Button::new(text).wrap())
                            .on_hover_text("клік — відкрити; правий клік — дії з документом");
                        if btn.clicked() {
                            app.open_reader_from(hits.clone(), i);
                        }
                        // Right-click: quick actions for this hit.
                        let was_sel = app.search.selected.contains(&h.nreg);
                        btn.context_menu(|ui| {
                            if ui.button("Відкрити в читалці").clicked() {
                                app.open_reader_from(hits.clone(), i);
                                ui.close_menu();
                            }
                            if ui.button("Відкрити на сайті").clicked() {
                                ui.ctx().open_url(egui::OpenUrl::new_tab(
                                    rada_core::RadaClient::site_url(&h.nreg),
                                ));
                                ui.close_menu();
                            }
                            if ui.button("Копіювати реєстраційний номер").clicked() {
                                ui.ctx().copy_text(h.nreg.clone());
                                ui.close_menu();
                            }
                            if ui.button("Копіювати назву").clicked() {
                                ui.ctx().copy_text(h.nazva.clone());
                                ui.close_menu();
                            }
                            let lbl = if was_sel { "Зняти позначку" } else { "Позначити для файлів" };
                            if ui.button(lbl).clicked() {
                                if was_sel {
                                    app.search.selected.retain(|n| n != &h.nreg);
                                } else {
                                    app.search.selected.push(h.nreg.clone());
                                }
                                ui.close_menu();
                            }
                        });
                        if ui.button(egui::RichText::new("копія").small()).clicked() {
                            ui.ctx().copy_text(h.nreg.clone());
                        }
                        if ui
                            .button(egui::RichText::new("сайт").small())
                            .on_hover_text("відкрити акт на порталі zakon.rada.gov.ua")
                            .clicked()
                        {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(
                                rada_core::RadaClient::site_url(&h.nreg),
                            ));
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

                // ---- cache full-text («Знайти в кеші») ----------------
                let cloading = app.search.cache_loading;
                let cdone = app.search.cache_done;
                if cloading || cdone {
                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "у кешованому тексті · «{}»",
                                app.search.cache_term
                            ))
                            .strong()
                            .size(15.0),
                        );
                        if cloading {
                            ui.spinner();
                            ui.label(
                                egui::RichText::new("сканую кешовані тіла …")
                                    .small()
                                    .color(crate::themes::weak()),
                            );
                        } else if let Some(e) = &app.search.cache_error {
                            ui.label(egui::RichText::new(format!("помилка: {e}")).small().color(err()));
                        } else {
                            ui.label(
                                egui::RichText::new(format!(
                                    "документів(а): {}",
                                    app.search.cache_hits.len()
                                ))
                                .small()
                                .color(crate::themes::weak()),
                            );
                        }
                    });
                    let rows = app.search.cache_hits.clone();
                    for (h, title) in rows {
                        let label = cache_row_label(&title, &h.nreg);
                        let avail = (ui.available_width() - 30.0).max(100.0);
                        let text = egui::RichText::new(format!(
                            "{}\n{}   збігів: {}",
                            label, h.nreg, h.hits
                        ))
                        .small();
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("•").color(gold()));
                            if ui
                                .add_sized([avail, 46.0], egui::Button::new(text).wrap())
                                .clicked()
                            {
                                // Prefer the full offline row (status/dates).
                                let hit = rada_db::open(&app.db_path)
                                    .ok()
                                    .and_then(|conn| {
                                        rada_db::search::by_nreg(&conn, &h.nreg).ok().flatten()
                                    });
                                match hit {
                                    Some(hit) => app.open_reader(hit),
                                    None => app
                                        .open_reader_by_nreg(&h.nreg, &label, 0, "", "", ""),
                                }
                            }
                        });
                        if !h.snippet.trim().is_empty() {
                            ui.indent(egui::Id::new(("cache_snip", h.nreg.clone())), |ui| {
                                ui.label(
                                    egui::RichText::new(&h.snippet).small().weak().italics(),
                                );
                            });
                        }
                    }
                    if cdone && !cloading && app.search.cache_error.is_none()
                        && app.search.cache_hits.is_empty()
                    {
                        ui.label(
                            egui::RichText::new(
                                "збігів немає (документи потрапляють у кеш, коли ви читаєте їх у читалці)",
                            )
                            .small()
                            .color(crate::themes::weak()),
                        );
                    }
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

    calendar_popup(app, ctx);
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

/// Publisher picker: free-text filter + combo of matching organs (cap 25).
fn org_combo(app: &mut App, ui: &mut egui::Ui) {
    let opts: Vec<(i64, String)> = app.dicts.organs.clone();
    let q = app.search.f_org_q.to_lowercase();
    let matches: Vec<(i64, String)> = if q.is_empty() {
        opts.iter().take(25).cloned().collect()
    } else {
        opts.iter()
            .filter(|(_, l)| l.to_lowercase().contains(&q))
            .take(25)
            .cloned()
            .collect()
    };
    let cur = app.search.f_org;
    let label = cur
        .map(|id| opt_label(&app.dicts.organs, id, "орган"))
        .unwrap_or_else(|| {
            if app.search.f_org_label.is_empty() {
                "усі".into()
            } else {
                app.search.f_org_label.clone()
            }
        });
    let sel = egui::ComboBox::from_id_salt("f_org").width(220.0).selected_text(label);
    sel.show_ui(ui, |ui| {
        if ui.selectable_label(cur.is_none(), "усі").clicked() {
            app.search.f_org = None;
            app.search.f_org_label.clear();
        }
        if matches.is_empty() {
            ui.label(egui::RichText::new("не знайдено збігів").small());
        }
        for (id, lab) in &matches {
            if ui.selectable_label(cur == Some(*id), lab).clicked() {
                app.search.f_org = Some(*id);
                app.search.f_org_label = lab.clone();
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

/// Which filter column a facet chip click edits (`None` = clear it).
enum FacetPick {
    Status(Option<i64>),
    Typ(Option<i64>),
    /// `(picked id, label stored into f_org_label)`; `(None, _)` clears.
    Org(Option<i64>, String),
}

/// Top-6 facet entries as `(label, id)` with dictionary-resolved labels:
/// known id → dict label, unset value 0 → «немає», unknown id → «id N».
/// `counts` arrives sorted by count descending (as `rada_db::facets` emits).
pub fn facet_chips(counts: &[(i64, usize)], dict: &[(i64, String)]) -> Vec<(String, i64)> {
    counts
        .iter()
        .take(6)
        .map(|(id, _)| {
            let label = dict
                .iter()
                .find(|(did, _)| *did == *id)
                .map(|(_, l)| l.clone())
                .unwrap_or_else(|| {
                    if *id == 0 {
                        "немає".into()
                    } else {
                        format!("id {id}")
                    }
                });
            (label, *id)
        })
        .collect()
}

/// Draw one facet group: weak title + up to six `{label} ({count})` chips.
/// Returns the clicked chip as `(id, was_already_active)` — the caller
/// toggles: active chip clears the filter, any other chip sets it.
fn facet_group(
    ui: &mut egui::Ui,
    title: &str,
    counts: &[(i64, usize)],
    dict: &[(i64, String)],
    active: Option<i64>,
) -> Option<(i64, bool)> {
    ui.label(
        egui::RichText::new(title)
            .small()
            .color(crate::themes::weak()),
    );
    let mut hit = None;
    for (label, id) in facet_chips(counts, dict) {
        let count = counts
            .iter()
            .find(|(i, _)| *i == id)
            .map(|(_, c)| *c)
            .unwrap_or(0);
        let sel = active == Some(id);
        let btn = egui::Button::new(egui::RichText::new(format!("{label} ({count})")).small())
            .selected(sel);
        if ui.add(btn).clicked() {
            hit = Some((id, sel));
        }
    }
    hit
}

/// Which active-filter chip was clicked (`None` = untouched).
enum ClearWhat {
    None,
    Status,
    Typ,
    Org,
    DateFrom,
    DateTo,
    Number,
    All,
}

/// Generic dictionary label for the active-filter chips: dict hit → label,
/// id 0 → «немає», unknown id → «id N».
/// The trailing offline-cache badge on a hit's meta line: « · кеш» when the
/// act's body text is already in the offline text cache (readable without
/// network), empty string otherwise. Single source for the badge wording.
pub fn cache_badge(cached: bool) -> &'static str {
    if cached {
        " · кеш"
    } else {
        ""
    }
}

/// Ctrl+Enter on the search page: index of the hit to open (the top hit),
/// or `None` when there is nothing to open (no results yet / failed search).
pub fn top_hit_index(hits: &[rada_db::SearchHit], searched: bool) -> Option<usize> {
    if searched && !hits.is_empty() {
        Some(0)
    } else {
        None
    }
}

pub fn filter_label(dict: &[(i64, String)], id: i64) -> String {
    dict.iter()
        .find(|(did, _)| *did == id)
        .map(|(_, l)| l.clone())
        .unwrap_or_else(|| {
            if id == 0 {
                "немає".into()
            } else {
                format!("id {id}")
            }
        })
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

/// Minimal percent-encoder for a URL query value (UTF-8 bytes; only
/// unreserved characters survive, spaces become `+`).
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Portal deep-link for advanced search. Empty query opens the advanced
/// search page; a non-empty query opens the name-search results with the
/// terms passed through (`text=a+b`).
pub fn portal_search_url(query: &str) -> String {
    let q = query.trim();
    if q.is_empty() {
        "https://zakon.rada.gov.ua/laws/adv".to_string()
    } else {
        format!(
            "https://zakon.rada.gov.ua/laws/find/main_result?text={}",
            url_encode(q)
        )
    }
}

/// List label of a cache hit: the offline index title when known, else the
/// bare nreg (uncached rows have no title).
pub fn cache_row_label(title: &str, nreg: &str) -> String {
    if title.trim().is_empty() {
        nreg.to_string()
    } else {
        title.to_string()
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
            sort: rada_db::search::Sort::Relevance,
            selected: Vec::new(),
            pick: None,
            pick_year: 0,
            pick_month: 0,
            f_number: String::new(),
            f_org: None,
            f_org_label: String::new(),
            f_org_q: String::new(),
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
            live_export: None,
            cache_run: 0,
            cache_loading: false,
            cache_done: false,
            cache_term: String::new(),
            cache_hits: Vec::new(),
            cache_error: None,
            facets_run: 0,
            facets_loading: false,
            facets: None,
            facets_for: String::new(),
            cached: Vec::new(),
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
    /// Column ordering for the results list (Relevance = FTS rank,
    /// degrading to newest-first outside FTS).
    pub sort: rada_db::search::Sort,
    /// nregs checked for individual-file export (cleared on each search).
    pub selected: Vec<String>,
    /// Open calendar popup (None = closed); month shown while open.
    pub pick: Option<PickKind>,
    pub pick_year: u32,
    pub pick_month: u32,
    pub f_number: String,
    /// Publisher (organs dict id) filter; also holds the picked label.
    pub f_org: Option<i64>,
    pub f_org_label: String,
    pub f_org_q: String,
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
    /// In-flight live bulk export (done / total).
    pub live_export: Option<(u64, u64)>,
    /// Offline full-text search over cached bodies («Знайти в кеші»).
    pub cache_run: u64,
    pub cache_loading: bool,
    pub cache_done: bool,
    /// Term the displayed cache results belong to.
    pub cache_term: String,
    /// `(hit, index title)` pairs, ranked.
    pub cache_hits: Vec<(rada_db::textcache::CacheHit, String)>,
    pub cache_error: Option<String>,
    /// Generation of the in-flight facet-counts worker (advisory filter chips).
    pub facets_run: u64,
    pub facets_loading: bool,
    /// Per-column counts for the current query (`None` until they arrive or
    /// after a failure — facets never block or disturb the hit list).
    pub facets: Option<rada_db::FacetCounts>,
    /// The query term the displayed facets belong to.
    pub facets_for: String,
    /// Nregs of the current hits whose body is in the offline text cache
    /// (drives the «кеш» badge; recomputed with every search).
    pub cached: Vec<String>,
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
            org_id: self.f_org,
            typ: self.f_typ,
            status: self.f_status,
            date_from: parse(&self.f_date_from, &mut invalid),
            date_to: parse(&self.f_date_to, &mut invalid),
            sort: self.sort,
            doc_number: {
                let t = self.f_number.trim();
                if t.is_empty() { None } else { Some(t.to_string()) }
            },
            limit: self.limit,
            offset: self.offset_field,
        };
        self.invalid_date = invalid;
        // A reversed date range would silently match nothing — swap it.
        if let (Some(a), Some(b)) = (&q.date_from, &q.date_to) {
            if a > b {
                let (f, t) = (q.date_to.clone(), q.date_from.clone());
                return rada_db::search::SearchQuery {
                    date_from: f,
                    date_to: t,
                    ..q
                };
            }
        }
        q
    }
}

// ---------------------------------------------------------------------
// date picker: calendar popup anchored on the freshest fetched data
// ---------------------------------------------------------------------

/// Which filter date field the calendar edits.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PickKind {
    From,
    To,
}

const MONTHS_UK: [&str; 12] = [
    "Січень",
    "Лютий",
    "Березень",
    "Квітень",
    "Травень",
    "Червень",
    "Липень",
    "Серпень",
    "Вересень",
    "Жовтень",
    "Листопад",
    "Грудень",
];
const WEEKDAYS_UK: [&str; 7] = ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Нд"];

fn is_leap(y: u32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: u32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// Days since 1970-01-01 (Howard Hinnant's civil calendar algorithm).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { (m - 3) as i64 } else { (m + 9) as i64 };
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z0: i64) -> (u32, u32, u32) {
    let z = z0 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as u32, m as u32, d as u32)
}

/// Monday = 0 … Sunday = 6.
fn weekday_mon0(y: u32, m: u32, d: u32) -> u32 {
    ((days_from_civil(y as i64, m, d) + 3).rem_euclid(7)) as u32
}

fn today_ymd() -> (u32, u32, u32) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    civil_from_days((secs / 86_400) as i64)
}

/// Strict `YYYYMMDD` parse (also used to keep the typed fields honest).
pub fn parse_ymd(s: &str) -> Option<(u32, u32, u32)> {
    let t: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    if t.len() != 8 {
        return None;
    }
    let y: u32 = t[..4].parse().ok()?;
    let m: u32 = t[4..6].parse().ok()?;
    let d: u32 = t[6..8].parse().ok()?;
    if !(1900..=2100).contains(&y) || !(1..=12).contains(&m) {
        return None;
    }
    if d == 0 || d > days_in_month(y, m) {
        return None;
    }
    Some((y, m, d))
}

/// Freshest data actually fetched from rada: the newest card date in the
/// index, falling back to the last successful import/sync, then real today.
pub fn data_today(app: &App) -> (u32, u32, u32) {    if let Some(st) = app.index_status.as_ref() {
        if let Some(md) = st.max_date.as_deref().and_then(parse_ymd) {
            return md;
        }
        if let Some(li) = st.last_import.as_deref() {
            // "YYYY-MM-DD HH:MM:SS" -> YYYYMMDD
            let t: String = li.chars().take(10).filter(|c| c.is_ascii_digit()).collect();
            if let Some(d) = parse_ymd(&t) {
                return d;
            }
        }
    }
    today_ymd()
}

/// Date range for the quick preset chip «останні N днів»: `from` is `days`
/// calendar days before `today` (YYYYMMDD), `to` stays empty (no upper
/// bound). Clamped: dates before 1900 yield an empty `from` (no filter).
pub fn preset_range(today: (u32, u32, u32), days: u32) -> (String, String) {
    let z = days_from_civil(today.0 as i64, today.1, today.2) - days as i64;
    let (y, m, d) = civil_from_days(z);
    if y < 1900 {
        return (String::new(), String::new());
    }
    (format!("{y:04}{m:02}{d:02}"), String::new())
}

/// Open the calendar for `kind`, scrolled to its value or the data month.
pub fn open_calendar(app: &mut App, kind: PickKind) {
    let cur = match kind {
        PickKind::From => app.search.f_date_from.clone(),
        PickKind::To => app.search.f_date_to.clone(),
    };
    let (y, m) = parse_ymd(&cur)
        .map(|(y, m, _)| (y, m))
        .unwrap_or_else(|| {
            let (y, m, _) = data_today(app);
            (y, m)
        });
    app.search.pick = Some(kind);
    app.search.pick_year = y;
    app.search.pick_month = m;
}

fn calendar_popup(app: &mut App, ctx: &egui::Context) {
    let Some(kind) = app.search.pick else {
        return;
    };
    let (ay, am, ad) = data_today(app);
    let selected = parse_ymd(match kind {
        PickKind::From => &app.search.f_date_from,
        PickKind::To => &app.search.f_date_to,
    });
    let mut close = false;
    let title = match kind {
        PickKind::From => "Дата з",
        PickKind::To => "Дата по",
    };
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            let y = app.search.pick_year;
            let m = app.search.pick_month;
            ui.horizontal(|ui| {
                if ui.button("‹").clicked() {
                    if m == 1 {
                        app.search.pick_year = y.saturating_sub(1);
                        app.search.pick_month = 12;
                    } else {
                        app.search.pick_month = m - 1;
                    }
                }
                ui.label(
                    egui::RichText::new(format!("{} {}", MONTHS_UK[(m - 1) as usize], y)).strong(),
                );
                if ui.button("›").clicked() {
                    if m == 12 {
                        app.search.pick_year = y + 1;
                        app.search.pick_month = 1;
                    } else {
                        app.search.pick_month = m + 1;
                    }
                }
            });
            ui.horizontal(|ui| {
                for w in WEEKDAYS_UK {
                    ui.add_sized(
                        [31.0, 18.0],
                        egui::Label::new(egui::RichText::new(w).small().weak()),
                    );
                }
            });
            let first_w = weekday_mon0(y, m, 1) as usize;
            let dim = days_in_month(y, m);
            egui::Grid::new("cal_days")
                .num_columns(7)
                .spacing([3.0, 3.0])
                .show(ui, |ui| {
                    let mut idx = 0usize;
                    for _ in 0..first_w {
                        ui.add_sized([31.0, 24.0], egui::Label::new(""));
                        idx += 1;
                        if idx % 7 == 0 {
                            ui.end_row();
                        }
                    }
                    for d in 1..=dim {
                        let date = (y, m, d);
                        // The index cannot hold data newer than the freshest
                        // successful fetch: future dates would filter to empty.
                        let blocked = date > (ay, am, ad) || date < (1990, 1, 1);
                        let sel = selected == Some(date);
                        let btn = egui::Button::new(egui::RichText::new(format!("{d}")).small())
                            .selected(sel);
                        let r = ui.add_enabled(!blocked, btn);
                        if blocked && date > (ay, am, ad) {
                            r.clone().on_hover_text("пізніше за останні отримані з ради дані");
                        }
                        if r.clicked() {
                            let v = format!("{y:04}{m:02}{d:02}");
                            match kind {
                                PickKind::From => app.search.f_date_from = v,
                                PickKind::To => app.search.f_date_to = v,
                            };
                            close = true;
                        }
                        idx += 1;
                        if idx % 7 == 0 {
                            ui.end_row();
                        }
                    }
                });
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(format!(
                    "найновіші отримані з ради дані: {ay:04}-{am:02}-{ad:02}"
                ))
                .small()
                .weak(),
            );
            ui.horizontal(|ui| {
                if ui.button("скинути").clicked() {
                    match kind {
                        PickKind::From => app.search.f_date_from.clear(),
                        PickKind::To => app.search.f_date_to.clear(),
                    };
                    close = true;
                }
                if ui.button("місяць даних").clicked() {
                    app.search.pick_year = ay;
                    app.search.pick_month = am;
                }
                if ui.button("закрити").clicked() {
                    close = true;
                }
            });
        });
    if close {
        app.search.pick = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_range_shifts_by_days_and_clamps() {
        // Same-day bound when days = 0, empty upper bound always.
        assert_eq!(
            preset_range((2026, 10, 28), 0),
            ("20261028".to_string(), String::new())
        );
        // Crosses the month boundary.
        assert_eq!(
            preset_range((2026, 3, 5), 7),
            ("20260226".to_string(), String::new())
        );
        // Crosses the year boundary.
        assert_eq!(
            preset_range((2026, 1, 3), 10),
            ("20251224".to_string(), String::new())
        );
        // Leap day is reachable backwards from March 1.
        assert_eq!(
            preset_range((2028, 3, 1), 1),
            ("20280229".to_string(), String::new())
        );
        // A year window lands on the previous year's same date.
        assert_eq!(
            preset_range((2027, 6, 15), 365),
            ("20260615".to_string(), String::new())
        );
        // Clamped below 1900: no filter at all.
        assert_eq!(
            preset_range((1900, 1, 2), 400),
            (String::new(), String::new())
        );
    }

    #[test]
    fn facet_chips_resolve_labels_cap_and_order() {
        let dict = vec![
            (1, "чинний".to_string()),
            (2, "недіючий".to_string()),
            (3, "а".to_string()),
            (4, "б".to_string()),
            (5, "в".to_string()),
        ];
        // Already count-desc, as rada_db::facets emits.
        let counts = vec![(1, 9), (0, 8), (2, 7), (99, 6), (3, 5), (4, 4), (5, 3)];
        let chips = facet_chips(&counts, &dict);
        // Cap at 6, input order preserved.
        assert_eq!(chips.len(), 6, "only the top 6 entries become chips");
        assert!(!chips.iter().any(|(_, id)| *id == 5), "7th entry dropped");
        // Labels: known id → dict, 0 → «немає», unknown → «id N».
        assert_eq!(chips[0], ("чинний".to_string(), 1));
        assert_eq!(chips[1], ("немає".to_string(), 0));
        assert_eq!(chips[2], ("недіючий".to_string(), 2));
        assert_eq!(chips[3], ("id 99".to_string(), 99));
        assert_eq!(chips[4], ("а".to_string(), 3));
        assert_eq!(chips[5], ("б".to_string(), 4));
        // Empty input → no chips.
        assert!(facet_chips(&[], &dict).is_empty());
    }

    #[test]
    fn cache_badge_has_a_single_wording() {
        assert_eq!(cache_badge(true), " · кеш");
        assert_eq!(cache_badge(false), "");
    }

    #[test]
    fn top_hit_index_only_when_results_present() {
        let hit = rada_db::SearchHit {
            nreg: "1-23".into(),
            nazva: "Про тест".into(),
            status: 1,
            org_date: "20200101".into(),
            org_num: "1".into(),
            poddat: String::new(),
        };
        assert_eq!(top_hit_index(&[], true), None, "no results");
        assert_eq!(top_hit_index(&[hit.clone()], false), None, "search not run/failed");
        assert_eq!(top_hit_index(&[hit], true), Some(0), "top hit is index 0");
    }

    #[test]
    fn filter_label_resolves_dict_zero_and_unknown() {
        let d = vec![(5i64, "чинний".to_string()), (21, "постанова".to_string())];
        assert_eq!(filter_label(&d, 21), "постанова");
        assert_eq!(filter_label(&d, 0), "немає");
        assert_eq!(filter_label(&d, 999), "id 999");
        assert_eq!(filter_label(&[], 7), "id 7", "empty dict falls back");
    }

    #[test]
    fn cache_row_label_prefers_title() {        assert_eq!(cache_row_label("Про освіту", "1556-18"), "Про освіту");
        assert_eq!(cache_row_label("   ", "1556-18"), "1556-18", "blank title falls back");
        assert_eq!(cache_row_label("", "1-1"), "1-1");
    }

    #[test]
    fn calendar_math() {
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2023, 2), 28);
        assert_eq!(days_in_month(2000, 2), 29);
        assert_eq!(days_in_month(2100, 2), 28);
        // 2026-10-27 was a Tuesday (Monday = 0).
        assert_eq!(weekday_mon0(2026, 10, 27), 1);
        // 2024-01-01 was a Monday.
        assert_eq!(weekday_mon0(2024, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(days_from_civil(2026, 10, 27)), (2026, 10, 27));
        assert_eq!(civil_from_days(days_from_civil(2000, 2, 29)), (2000, 2, 29));
    }

    #[test]
    fn ymd_parse_is_strict() {
        assert_eq!(parse_ymd("20240101"), Some((2024, 1, 1)));
        assert_eq!(parse_ymd("20240230"), None); // Feb 30
        assert_eq!(parse_ymd("20230229"), None); // non-leap
        assert_eq!(parse_ymd("18991231"), None); // out of range
        assert_eq!(parse_ymd("2024-01-01"), Some((2024, 1, 1))); // separators tolerated
        assert_eq!(parse_ymd("240101"), None); // too short
    }

    #[test]
    fn portal_url_empty_opens_adv() {
        assert_eq!(
            portal_search_url("   "),
            "https://zakon.rada.gov.ua/laws/adv"
        );
    }

    #[test]
    fn portal_url_encodes_query() {
        // ASCII: spaces become +, reserved chars percent-encoded.
        assert_eq!(
            portal_search_url("tax  code&x"),
            "https://zakon.rada.gov.ua/laws/find/main_result?text=tax++code%26x"
        );
        // Cyrillic "код" → UTF-8 percent-encoding (К=0xD0 0x9A …).
        assert_eq!(
            url_encode("код"),
            "%D0%BA%D0%BE%D0%B4"
        );
        assert!(portal_search_url("код").ends_with("text=%D0%BA%D0%BE%D0%B4"));
    }
}
