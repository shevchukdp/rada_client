//! Home page: index status + snapshot fetch + incremental sync.

use eframe::egui;

use crate::app::{err, gold, App};

const KB: f64 = 1_048_576.0;

pub fn show(app: &mut App, ctx: &egui::Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            if !app.settings.last_nreg.is_empty() {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Продовжити:").strong());
                    let title: String =
                        app.settings.last_title.chars().take(80).collect();
                    let label = if title.trim().is_empty() {
                        app.settings.last_nreg.clone()
                    } else {
                        format!("{} — {}", app.settings.last_nreg, title)
                    };
                    if ui.button(egui::RichText::new(label).small()).clicked() {
                        let nreg = app.settings.last_nreg.clone();
                        let title = app.settings.last_title.clone();
                        app.open_reader_by_nreg(&nreg, &title, 0, "", "", "");
                    }
                });
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);
            }
            ui.heading("Стан індексу");

            match &app.index_status {
                None => {
                    ui.spinner();
                    ui.label("зчитую ...");
                }
                Some(st) => {
                    egui::Grid::new("home_status")
                        .num_columns(2)
                        .spacing([24.0, 6.0])
                        .show(ui, |ui| {
                            ui.label("файл індексу");
                            ui.label(app.db_path.display().to_string());
                            ui.end_row();

                            if st.docs > 0 {
                                ui.label("документів");
                                ui.label(
                                    egui::RichText::new(format!("{}", st.docs))
                                        .strong()
                                        .color(gold()),
                                );
                                ui.end_row();

                                ui.label("розмір бази");
                                ui.label(format!("{:.1} MB", st.size_bytes as f64 / KB));
                                ui.end_row();

                                ui.label("останній імпорт/синк");
                                ui.label(st.last_import.clone().unwrap_or_else(|| "—".into()));
                                ui.end_row();

                                for (kind, count) in &st.dicts {
                                    ui.label(match kind.as_str() {
                                        "stan" => "словник stan (стани)",
                                        "typs" => "словник typs (типи)",
                                        "organs" => "словник organs (органи)",
                                        other => other,
                                    });
                                    ui.label(format!("{count} записів"));
                                    ui.end_row();
                                }
                                let dicts_empty = st.dicts.iter().all(|(_, c)| *c == 0);
                                if dicts_empty {
                                    ui.label(
                                        egui::RichText::new(
                                            "словники порожні — натисніть «Оновити наживо» нижче",
                                        )
                                        .small()
                                        .color(gold()),
                                    );
                                    ui.end_row();
                                }
                            } else {
                                ui.label(
                                    egui::RichText::new("індекс порожній або відсутній")
                                        .color(err()),
                                );
                                ui.end_row();
                            }
                        });
                }
            }

            ui.add_space(14.0);
            ui.separator();
            ui.heading("Оновлення даних");

            if app.index_status.as_ref().map(|s| s.docs).unwrap_or(0) == 0 {
                // Fresh install: propose the snapshot.
                ui.label(
                    "Немає локального індексу. Найшвидший шлях — завантажити щомісячний \
                     снапшот (~70 MB) з GitHub і розгорнути його локально.",
                );
                fetch_controls(app, ui);
            } else {
                let st = app.index_status.clone();
                if let Some(st) = st {
                    ui.horizontal(|ui| {
                        if st.docs > 0 {
                            ui.label(
                                egui::RichText::new(format!(
                                    "поточна база: {} док.",
                                    st.docs
                                )),
                            );
                        }
                    });
                }
                ui.collapsing("Оновлення індексу", |ui| {
                    ui.label("1. «Оновити наживо» — зміни за останні години (терміново).");
                    ui.label("2. «Отримати снапшот» — повний свіжий індекс за місяць + ваші анотації будуть збережені.");
                });
                ui.add_space(8.0);
                sync_controls(app, ui);
                ui.add_space(10.0);
                fetch_controls(app, ui);
            }

            sync_log(app, ui);

            ui.add_space(18.0);
            ui.separator();
            ui.label(
                egui::RichText::new(format!(
                    "Живі запити дотримуються паузи {} с (правило серверу: UA «OpenData», ≤60 за хв). \
                     Пошук по заголовках, анотації й стани — повністю офлайн.",
                    app.pace_secs
                ))
                .small(),
            );
        });
    });
}

fn sync_controls(app: &mut App, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        let watch = ui.add(
            egui::Button::new(
                egui::RichText::new(if app.home.sync_running {
                    "Оновлюю наживо ..."
                } else {
                    "⟳ Оновити наживо (sync)"
                })
                .strong(),
            ),
        );
        if watch.clicked() && !app.home.sync_running {
            app.run_sync();
        }
        ui.label("словники + фід змін за останні години (мінімум 5 c між запитами).");
    });
}

fn fetch_controls(app: &mut App, ui: &mut egui::Ui) {
    if !app.home.fetch_running {
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new("⬇ Отримати снапшот").strong().color(gold()),
                ),
            )
            .clicked()
        {
            app.home.confirm_fetch = true;
        }
    } else {
        ui.horizontal(|ui| {
            ui.spinner();
            match &app.home.fetch_prog {
                Some(crate::app::FetchProg::Resolve) => {
                    ui.label("шукаю останній снапшот на GitHub ...");
                }
                Some(crate::app::FetchProg::Download { done }) => {
                    ui.label(format!("завантаження: MB {}", *done as f64 / KB));
                }
                Some(crate::app::FetchProg::Restore) => {
                    ui.label("розгортаю базу (gzip + перевірка цілісності) ...");
                }
                None => {
                    ui.label("готу ...");
                }
            }
        });
    }

    if app.home.confirm_fetch && !app.home.fetch_running {
        let has_index = app.index_status.as_ref().map(|s| s.docs).unwrap_or(0) > 0;
        egui::Window::new("Отримати снапшот?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                ui.label(format!(
                    "Завантажити свіжий щомісячний індекс (~70 MB) і замінити поточний:\n{}",
                    app.db_path.display()
                ));
                if has_index {
                    ui.label("Ваші анотації збережуться і повернуться після відновлення.");
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Скасувати").clicked() {
                        app.home.confirm_fetch = false;
                    }
                    if ui
                        .button(
                            egui::RichText::new("Завантажити")
                                .strong()
                                .color(gold()),
                        )
                        .clicked()
                    {
                        app.home.confirm_fetch = false;
                        app.run_fetch_snapshot();
                    }
                });
            });
    }
}

fn sync_log(app: &mut App, ui: &mut egui::Ui) {
    if app.home.sync_log.is_empty() {
        return;
    }
    ui.add_space(10.0);
    ui.strong("Журнал дій");
    for line in &app.home.sync_log {
        ui.label(egui::RichText::new(format!("· {line}")).small());
    }
}

/// Home page state.
#[derive(Default)]
pub struct HomeState {
    pub sync_running: bool,
    pub sync_log: Vec<String>,
    pub fetch_running: bool,
    pub fetch_prog: Option<crate::app::FetchProg>,
    pub confirm_fetch: bool,
}
