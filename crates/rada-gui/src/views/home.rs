//! Home page: index status + snapshot fetch + incremental sync.

use eframe::egui;

use crate::app::{err, gold, App};

const KB: f64 = 1_048_576.0;

pub fn show(app: &mut App, ctx: &egui::Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            let recents = app.settings.recents.clone();
            if !recents.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("Останні документи:").strong());
                    for (n, t) in recents.iter().take(6) {
                        let full: Vec<char> = t.chars().collect();
                        let short: String = full.iter().take(38).collect();
                        let title = if full.len() > 38 {
                            format!("{short}…")
                        } else {
                            short
                        };
                        let label = if title.trim().is_empty() {
                            n.clone()
                        } else {
                            format!("{n} — {title}")
                        };
                        if ui.button(egui::RichText::new(label).small()).clicked() {
                            app.open_reader_by_nreg(n, t, 0, "", "", "");
                        }
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

                                ui.label("кеш текстів");
                                match app.cache_stats {
                                    Some((rows, bytes)) if rows > 0 => ui.label(format!(
                                        "{} тіл · {:.1} MB",
                                        crate::views::reader::group_thousands(rows as usize),
                                        bytes as f64 / KB
                                    )),
                                    Some(_) => ui.label(
                                        egui::RichText::new("порожньо — тіла кешуються при читанні")
                                            .small()
                                            .weak(),
                                    ),
                                    None => ui.label(
                                        egui::RichText::new("зчитую …").small().weak(),
                                    ),
                                };
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
                     снапшот (~70 MB) з GitHub і розгорнути його локально. Або побудуйте \
                     індекс з офіційного дампу в розділі «Повна побудова індексу» нижче.",
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

            ui.add_space(10.0);
            backup_controls(app, ui);
            ui.add_space(6.0);
            init_controls(app, ui);

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
            ui.horizontal(|ui| {
                if ui
                    .button(egui::RichText::new("Про програму").small())
                    .on_hover_text("версія, індекс, джерела даних")
                    .clicked()
                {
                    app.show_about = true;
                }
                ui.label(
                    egui::RichText::new(format!(
                        "rada-client v{}",
                        env!("CARGO_PKG_VERSION")
                    ))
                    .small()
                    .weak(),
                );
            });
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

/// Local backup / restore: gzip snapshot of the current index to a chosen
/// file, and restore of the index from such a file (annotations preserved).
fn backup_controls(app: &mut App, ui: &mut egui::Ui) {
    let has_index = app.index_status.as_ref().map(|s| s.docs).unwrap_or(0) > 0;
    egui::CollapsingHeader::new("Резервна копія")
        .default_open(true)
        .show(ui, |ui| {
        ui.label(
            "Збережіть поточний індекс у стиснений файл (.sqlite.gz) або відновіть його \
             з раніше збереженої копії. Анотації зберігаються в обох випадках.",
        );
        ui.horizontal(|ui| {
            if app.home.backup_busy {
                ui.spinner();
                ui.label("резервне копіювання / відновлення ...");
            } else {
                let bk = ui.add_enabled(
                    has_index,
                    egui::Button::new(egui::RichText::new("Зберегти резервну копію").strong()),
                );
                if bk.clicked() {
                    app.run_backup();
                }
                let rs = ui.add(
                    egui::Button::new(
                        egui::RichText::new("Відновити з файлу ...").color(gold()),
                    ),
                );
                if rs.clicked() {
                    app.home.confirm_restore = true;
                }
                if !has_index {
                    ui.label(
                        egui::RichText::new("(немає індексу — копіювати нічого)").small().weak(),
                    );
                }
            }
        });
        },
    );

    if app.home.confirm_restore && !app.home.backup_busy {
        egui::Window::new("Відновити індекс з файлу?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                ui.label(
                    "Оберіть файл копії (.sqlite.gz). Поточний індекс буде замінено:\n",
                );
                ui.label(app.db_path.display().to_string());
                if has_index {
                    ui.label("Ваші анотації збережуться і повернуться після відновлення.");
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Скасувати").clicked() {
                        app.home.confirm_restore = false;
                    }
                    if ui
                        .button(
                            egui::RichText::new("Обрати файл ...")
                                .strong()
                                .color(gold()),
                        )
                        .clicked()
                    {
                        app.run_restore_local();
                    }
                });
            });
    }
}

/// Full-dump init: build/rebuild the index from the official `doc.txt`
/// (network download or a locally picked file). Upsert-only, annotations safe.
fn init_controls(app: &mut App, ui: &mut egui::Ui) {
    egui::CollapsingHeader::new("Повна побудова індексу (init)")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                "Будує індекс з офіційного дампу doc.txt (~51 MB): завантаження через \
                 мережу або вибір локального файлу.",
            );
            ui.label(
                "Альтернатива снапшоту, коли GitHub недоступний; анотацій не чіпає.",
            );
            ui.horizontal(|ui| {
                if app.home.init_running {
                    ui.spinner();
                    ui.label("побудова індексу ...");
                } else {
                    if ui
                        .button(
                            egui::RichText::new("Побудувати з офіційного дампу").strong(),
                        )
                        .clicked()
                    {
                        app.home.confirm_init = true;
                    }
                    if ui.button("З файлу дампу ...").clicked() {
                        app.run_init(true);
                    }
                }
            });
        });

    if app.home.confirm_init && !app.home.init_running {
        egui::Window::new("Побудувати індекс з дампу?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                ui.label("Завантажити офіційний doc.txt (~51 MB) і поповнити індекс:");
                ui.label(app.db_path.display().to_string());
                ui.label("Існуючі документи оновляться; анотації залишаться.");
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Скасувати").clicked() {
                        app.home.confirm_init = false;
                    }
                    if ui
                        .button(
                            egui::RichText::new("Завантажити й побудувати")
                                .strong()
                                .color(gold()),
                        )
                        .clicked()
                    {
                        app.run_init(false);
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
    /// Local backup/restore worker is busy.
    pub backup_busy: bool,
    /// Restore-from-file confirmation window is open.
    pub confirm_restore: bool,
    /// Full-dump init worker is running.
    pub init_running: bool,
    /// Full-dump init confirmation window is open.
    pub confirm_init: bool,
}
