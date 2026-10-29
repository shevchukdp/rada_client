//! Home page: index status + snapshot fetch + incremental sync.

use eframe::egui;

use crate::app::{err, gold, App};

const KB: f64 = 1_048_576.0;

pub fn show(app: &mut App, ctx: &egui::Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            let recents = app.settings.recents.clone();
            if !recents.is_empty() {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("Останні документи:").strong());
                    for (n, t, at) in recents.iter().take(6) {
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
                        // stamp with the relative open time when known
                        let label = if *at > 0 {
                            format!("{label} · {}", crate::views::arrivals::fmt_since(now, *at))
                        } else {
                            label
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
                                    Some((rows, bytes)) if rows > 0 => {
                                        ui.horizontal(|ui| {
                                            ui.label(format!(
                                                "{} тіл · {:.1} MB",
                                                crate::views::reader::group_thousands(rows as usize),
                                                bytes as f64 / KB
                                            ));
                                            if ui
                                                .button(egui::RichText::new("очистити").small())
                                                .on_hover_text(
                                                    "видалити всі кешовані тіла документів \
                                                     (анотації та індекс не чіпаються)",
                                                )
                                                .clicked()
                                            {
                                                app.home.confirm_purge = true;
                                            }
                                        });
                                    }
                                    Some(_) => {
                                        ui.label(
                                            egui::RichText::new(
                                                "порожньо — тіла кешуються при читанні",
                                            )
                                            .small()
                                            .weak(),
                                        );
                                    }
                                    None => {
                                        ui.label(
                                            egui::RichText::new("зчитую …").small().weak(),
                                        );
                                    }
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
                    if st.docs > 0 && !st.fresh.is_empty() {
                        // Offline «fresh by adoption date» card: the same rows
                        // the CLI `recent` command renders, straight from the
                        // index (no network). Click opens the reader.
                        let fresh = st.fresh.clone();
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new("Свіжі документи (за датою):").strong(),
                        );
                        for (date, nreg, title) in fresh.iter() {
                            let label = fresh_row_label(date, nreg, title);
                            let (n, t, d) = (nreg.clone(), title.clone(), date.clone());
                            if ui
                                .button(egui::RichText::new(label).small())
                                .on_hover_text("відкрити в читалці")
                                .clicked()
                            {
                                app.open_reader_by_nreg(&n, &t, 0, &d, "", "");
                            }
                        }
                    }
                }
            }

            // ---- dictionary quick search ------------------------------------
            ui.add_space(10.0);
            let (dq, stan, typs, organs) = (
                app.home.dict_query.clone(),
                app.dicts.stan.clone(),
                app.dicts.typs.clone(),
                app.dicts.organs.clone(),
            );
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Пошук у словниках:").strong());
                ui.add_sized(
                    [260.0, 24.0],
                    egui::TextEdit::singleline(&mut app.home.dict_query)
                        .hint_text("міністерство, постанова, чинний…"),
                );
                if !app.home.dict_query.is_empty()
                    && ui
                        .small_button("×")
                        .on_hover_text("очистити пошук у словниках")
                        .clicked()
                {
                    app.home.dict_query.clear();
                }
            });
            let needle = dq.trim().to_lowercase();
            if !needle.is_empty() {
                let show = 8usize;
                let mut total = 0usize;
                let mut jump: Option<(DictField, i64, String)> = None;
                for (name, list) in [("stan", &stan), ("typs", &typs), ("organs", &organs)] {
                    let hits = dict_hits(list, &needle, usize::MAX);
                    if hits.is_empty() {
                        continue;
                    }
                    let Some(field) = dict_field(name) else {
                        continue;
                    };
                    total += hits.len();
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new(format!("{name}:")).small().strong());
                        for (id, label) in hits.iter().take(show) {
                            let short: String = label.chars().take(60).collect();
                            if ui
                                .button(
                                    egui::RichText::new(format!("«{short}» ({id})")).small(),
                                )
                                .on_hover_text("показати документи з цим значенням")
                                .clicked()
                            {
                                jump = Some((field, *id, label.clone()));
                            }
                        }
                        if hits.len() > show {
                            ui.label(
                                egui::RichText::new(format!("… ще {}", hits.len() - show))
                                    .small()
                                    .weak(),
                            );
                        }
                    });
                }
                if total == 0 {
                    ui.label(
                        egui::RichText::new("у словниках нічого не знайдено")
                            .small()
                            .weak(),
                    );
                }
                // clicking a value applies the matching filter and searches
                if let Some((f, id, label)) = jump {
                    match f {
                        DictField::Status => app.search.f_status = Some(id),
                        DictField::Typ => app.search.f_typ = Some(id),
                        DictField::Org => {
                            app.search.f_org = Some(id);
                            app.search.f_org_label = label;
                        }
                    }
                    app.search.set_offset(0);
                    app.page = crate::app::Page::Search;
                    app.run_search();
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

    if app.home.confirm_purge {
        egui::Window::new("Очистити кеш текстів?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                ui.label(
                    "Буде видалено всі кешовані тіла документів (вони завантажуватимуться \
                     повторно при читанні).\nІндекс, словники та анотації НЕ чіпаються.",
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Скасувати").clicked() {
                        app.home.confirm_purge = false;
                    }
                    if ui
                        .button(egui::RichText::new("Очистити").strong().color(err()))
                        .clicked()
                    {
                        app.home.confirm_purge = false;
                        app.purge_text_cache();
                    }
                });
            });
    }

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
    ui.horizontal(|ui| {
        ui.strong("Журнал дій");
        if ui
            .button(egui::RichText::new("очистити").small())
            .on_hover_text("сховати записи журналу")
            .clicked()
        {
            app.home.sync_log.clear();
        }
    });
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
    /// «Очистити кеш текстів» confirmation window is open.
    pub confirm_purge: bool,
    /// Quick substring query over the three dictionaries (session-only).
    pub dict_query: String,
}

/// Which search filter a dictionary feeds (for click-to-filter jumps from
/// the home dictionary search).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictField {
    Status,
    Typ,
    Org,
}

/// Map a dictionary name to its search filter; unknown names → `None`.
pub fn dict_field(name: &str) -> Option<DictField> {
    match name {
        "stan" => Some(DictField::Status),
        "typs" => Some(DictField::Typ),
        "organs" => Some(DictField::Org),
        _ => None,
    }
}

/// Case-insensitive substring search over dictionary entries
/// (`(id, label)` pairs): returns matching entries in their original order,
/// at most `cap` of them. An empty/whitespace needle matches nothing —
/// the UI only searches when the user typed something.
pub fn dict_hits(dicts: &[(i64, String)], needle: &str, cap: usize) -> Vec<(i64, String)> {
    let q = needle.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    dicts
        .iter()
        .filter(|(_, label)| label.to_lowercase().contains(&q))
        .take(cap)
        .cloned()
        .collect()
}

/// Row label for the home «Свіжі документи» card:
/// «dd.mm.yyyy  nreg  title», with the date shown only when it is a full
/// 8-digit YYYYMMDD and the title truncated to 60 chars (+ ellipsis).
/// An empty/blank title falls back to the bare nreg.
pub fn fresh_row_label(date: &str, nreg: &str, title: &str) -> String {
    let d = if date.len() == 8 && date.bytes().all(|b| b.is_ascii_digit()) {
        format!("{}.{}.{}  ", &date[6..8], &date[4..6], &date[0..4])
    } else {
        String::new()
    };
    let full: Vec<char> = title.chars().collect();
    let short: String = full.iter().take(60).collect();
    let short = if full.len() > 60 {
        format!("{short}…")
    } else {
        short
    };
    if short.trim().is_empty() {
        format!("{d}{nreg}")
    } else {
        format!("{d}{nreg}  {short}")
    }
}

#[cfg(test)]
mod tests {
    use super::{dict_field, dict_hits, fresh_row_label, DictField};

    fn d(id: i64, label: &str) -> (i64, String) {
        (id, label.to_string())
    }

    #[test]
    fn dict_hits_matches_case_insensitively_and_trims() {
        let dicts = vec![
            d(1, "чинний"),
            d(2, "недіючий"),
            d(3, "Міністерство юстиції України"),
            d(4, "Міністерство освіти і науки"),
        ];
        // plain substring, case-insensitive both ways
        assert_eq!(dict_hits(&dicts, "ЧИННИЙ", 10), vec![d(1, "чинний")]);
        assert_eq!(
            dict_hits(&dicts, "міністерство", 10),
            vec![d(3, "Міністерство юстиції України"), d(4, "Міністерство освіти і науки")]
        );
        // surrounding spaces are trimmed before matching
        assert_eq!(dict_hits(&dicts, "  недію  ", 10), vec![d(2, "недіючий")]);
    }

    #[test]
    fn dict_field_maps_known_dictionaries_only() {
        assert_eq!(dict_field("stan"), Some(DictField::Status));
        assert_eq!(dict_field("typs"), Some(DictField::Typ));
        assert_eq!(dict_field("organs"), Some(DictField::Org));
        assert_eq!(dict_field("Stan"), None, "names are exact ids");
        assert_eq!(dict_field(""), None);
    }

    #[test]
    fn dict_hits_empty_needle_matches_nothing_and_cap_applies() {
        let dicts = vec![d(1, "a"), d(2, "ab"), d(3, "abc")];
        // blank query = no search at all (the UI hides results anyway)
        assert!(dict_hits(&dicts, "", 10).is_empty());
        assert!(dict_hits(&dicts, "   ", 10).is_empty());
        // cap keeps the first matches in dictionary order
        assert_eq!(dict_hits(&dicts, "a", 2), vec![d(1, "a"), d(2, "ab")]);
        assert_eq!(dict_hits(&dicts, "a", 0), Vec::new());
        // no match = empty
        assert!(dict_hits(&dicts, "zzz", 10).is_empty());
        assert!(dict_hits(&[], "a", 10).is_empty());
    }

    #[test]
    fn fresh_row_label_formats_full_date_and_truncates_title() {
        assert_eq!(
            fresh_row_label("20261023", "1234-2026", "Про тест"),
            "23.10.2026  1234-2026  Про тест"
        );
        let long = "а".repeat(70);
        let l = fresh_row_label("20260101", "N", &long);
        assert!(l.starts_with("01.01.2026  N  "));
        assert!(l.ends_with('…'));
        // 10 date + 2 spaces + nreg + 2 spaces + 60 chars + ellipsis
        assert_eq!(l.chars().count(), 10 + 2 + 1 + 2 + 60 + 1);
    }

    #[test]
    fn fresh_row_label_skips_partial_dates_and_blank_titles() {
        assert_eq!(fresh_row_label("2026", "N1", "Титл"), "N1  Титл");
        assert_eq!(fresh_row_label("2026102a", "N1", "Титл"), "N1  Титл");
        assert_eq!(fresh_row_label("", "N1", "  "), "N1");
        assert_eq!(fresh_row_label("20261023", "N1", ""), "23.10.2026  N1");
    }
}
