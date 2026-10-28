//! GUI views: one file per page plus the shared bars drawn every frame.

pub mod arrivals;
pub mod favorites;
pub mod home;
pub mod reader;
pub mod search;

use eframe::egui;
use std::time::Duration;

use crate::app::{err, gold, App, Page};

/// Reference dictionaries as (id, label) lists.
#[derive(Default)]
pub struct Dicts {
    pub stan: Vec<(i64, String)>,
    pub typs: Vec<(i64, String)>,
    pub organs: Vec<(i64, String)>,
}

pub use arrivals::ArrivalsState;
pub use favorites::FavoritesState;
pub use home::HomeState;
pub use reader::ReaderState;
pub use search::SearchState;

/// Left-aligned page switcher + title; right-aligned live-client counters.
pub fn topbar(app: &mut App, ctx: &egui::Context) {
    let mut go: Option<Page> = None;
    let mut theme_pick: Option<String> = None;
    let mut pace_delta: Option<i64> = None;
    let mut help_toggle = false;
    egui::TopBottomPanel::top("topbar")
        .exact_height(46.0)
        .show(ctx, |ui| {
            let app_ref: &App = app;
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading(
                    egui::RichText::new(" § ")
                        .strong()
                        .color(gold())
                        .size(22.0),
                );
                ui.vertical(|ui| {
                    ui.set_min_width(180.0);
                    ui.label(
                        egui::RichText::new("Законодавство України")
                            .strong()
                            .size(16.0),
                    );
                    ui.label(
                        egui::RichText::new(format!(
                            "zakon.rada.gov.ua · локальний клієнт v{}",
                            env!("CARGO_PKG_VERSION")
                        ))
                        .small(),
                    );
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(10.0);
                    if ui
                        .button(egui::RichText::new("+").small())
                        .on_hover_text("більша пауза між live-запитами (до 60 с)")
                        .clicked()
                    {
                        pace_delta = Some(1);
                    }
                    if ui
                        .button(egui::RichText::new("−").small())
                        .on_hover_text("менша пауза між live-запитами (мінімум 1 с; сервер просить ≥5)")
                        .clicked()
                    {
                        pace_delta = Some(-1);
                    }
                    ui.label(
                        egui::RichText::new(format!("{} с/запит", app.pace_secs))
                            .small(),
                    );
                    ui.separator();
                    if ui
                        .button(egui::RichText::new("клавіші").small())
                        .on_hover_text("список гарячих клавіш")
                        .clicked()
                    {
                        help_toggle = true;
                    }
                    theme_picker(ui, app_ref.settings.theme_id(), &mut theme_pick);
                    page_button(ui, app_ref, "Оновлення", Page::Arrivals, &mut go);
                    page_button(ui, app_ref, "Читалка", Page::Reader, &mut go);
                    page_button(ui, app_ref, "Обране", Page::Favorites, &mut go);
                    page_button(ui, app_ref, "Пошук", Page::Search, &mut go);
                    page_button(ui, app_ref, "Головна", Page::Home, &mut go);
                });
            });
            ui.add_space(4.0);
        });

    if let Some(p) = go {
        if p != Page::Reader || app.reader.is_some() {
            app.page = p;
        }
    }
    if let Some(id) = theme_pick {
        app.set_theme(&id);
    }
    if let Some(d) = pace_delta {
        app.set_pace_secs((app.pace_secs as i64 + d).clamp(1, 60) as u64);
    }
    if help_toggle {
        app.show_help = !app.show_help;
    }
    if app.show_help {
        help_window(app, ctx);
    }
    if app.show_about {
        about_window(app, ctx);
    }
}

/// Every global keyboard shortcut the app actually implements, as
/// (keys, what it does) — the single source of truth rendered by both the
/// topbar «клавіші» window and the About dialog (web «клавіші» parity).
/// Keys stay ASCII (the bundled egui font has no arrow glyphs).
pub fn help_lines() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "Ctrl+F",
            "поле «Знайти в тексті» (читалка) або поле запиту (пошук)",
        ),
        ("Enter", "виконати пошук / відкрити документ за номером"),
        (
            "Enter / Shift+Enter",
            "наступний / попередній збіг (у полі «Знайти в тексті»)",
        ),
        ("Esc", "зняти підсвічування пошуку (у полі читалки)"),
        (
            "Alt+Left / Alt+Right",
            "попередній / наступний документ списку в читалці",
        ),
        (
            "Alt+PageUp / Alt+PageDown",
            "те саме, що й Alt+стрілки (зручніше правою рукою)",
        ),
        (
            "Ctrl+Plus / Ctrl+Minus",
            "більший / дрібніший текст читалки (крок 10%)",
        ),
        ("Ctrl+0", "скинути масштаб тексту читалки до 100%"),
    ]
}

/// «Клавіші»: the keyboard shortcut sheet opened from the topbar.
fn help_window(app: &mut App, ctx: &egui::Context) {
    let mut open = app.show_help;
    let mut close_req = false;
    egui::Window::new("Гарячі клавіші")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::RIGHT_TOP, [-12.0, 46.0])
        .show(ctx, |ui| {
            egui::Grid::new("help_grid")
                .num_columns(2)
                .spacing([12.0, 3.0])
                .show(ui, |ui| {
                    for (keys, what) in help_lines() {
                        ui.label(egui::RichText::new(keys).monospace().small());
                        ui.label(egui::RichText::new(what).small());
                        ui.end_row();
                    }
                });
            ui.separator();
            if ui.button("Закрити").clicked() {
                close_req = true;
            }
        });
    if close_req {
        open = false;
    }
    app.show_help = open;
}

/// «Про програму»: version, index facts, live counters, data sources, links.
fn about_window(app: &mut App, ctx: &egui::Context) {
    let sent = app
        .client
        .requests_sent
        .load(std::sync::atomic::Ordering::Relaxed);
    let st = app.index_status.clone();
    let db = app.db_path.display().to_string();
    let pace = app.pace_secs;
    let mut open = app.show_about;
    let mut close_req = false;
    egui::Window::new("Про програму")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(" § ")
                        .strong()
                        .color(gold())
                        .size(30.0),
                );
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("Законодавство України")
                            .strong()
                            .size(17.0),
                    );
                    ui.label(
                        egui::RichText::new(format!(
                            "локальний офлайн-клієнт · v{}",
                            env!("CARGO_PKG_VERSION")
                        ))
                        .small()
                        .weak(),
                    );
                });
            });
            ui.separator();
            egui::Grid::new("about_grid")
                .num_columns(2)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Індекс").strong());
                    ui.label(db);
                    ui.end_row();
                    match &st {
                        Some(s) if s.docs > 0 => {
                            ui.label(egui::RichText::new("Документів").strong());
                            ui.label(format!(
                                "{}  ·  {:.0} MB",
                                s.docs,
                                s.size_bytes as f64 / 1_048_576.0
                            ));
                            ui.end_row();
                            ui.label(egui::RichText::new("Найновіші дані").strong());
                            ui.label(
                                s.max_date
                                    .as_deref()
                                    .map(|d| crate::views::search::fmt_date(d))
                                    .unwrap_or_else(|| "—".into()),
                            );
                            ui.end_row();
                            ui.label(egui::RichText::new("Останній імпорт").strong());
                            ui.label(s.last_import.clone().unwrap_or_else(|| "—".into()));
                            ui.end_row();
                            let dict: Vec<String> = s
                                .dicts
                                .iter()
                                .map(|(k, n)| format!("{k} {n}"))
                                .collect();
                            ui.label(egui::RichText::new("Словники").strong());
                            ui.label(dict.join("  ·  "));
                            ui.end_row();
                        }
                        _ => {
                            ui.label("—");
                            ui.label("індекс відсутній — «Головна» → «Отримати снапшот»");
                            ui.end_row();
                        }
                    }
                    ui.label(egui::RichText::new("Live-запити").strong());
                    ui.label(format!("{sent}  ·  пауза {pace} с/запит"));
                    ui.end_row();
                });
            ui.collapsing("Гарячі клавіші", |ui| {
                egui::Grid::new("about_keys")
                    .num_columns(2)
                    .spacing([12.0, 3.0])
                    .show(ui, |ui| {
                        for (keys, what) in help_lines() {
                            ui.label(egui::RichText::new(keys).monospace().small());
                            ui.label(egui::RichText::new(what).small());
                            ui.end_row();
                        }
                    });
            });
            ui.separator();
            ui.label(
                egui::RichText::new(
                    "Дані: zakon.rada.gov.ua / data.rada.gov.ua (відкриті дані, CC BY 4.0).\n\
                     Обмеження сервера: ~200 МБ/добу — клиент сам дотримується паузи.",
                )
                .small()
                .weak(),
            );
            ui.horizontal(|ui| {
                let link = |ui: &mut egui::Ui, label: &str, url: &str| {
                    if ui.button(egui::RichText::new(label).small()).clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                    }
                };
                link(ui, "GitHub проєкту", "https://github.com/shevchukdp/rada_client");
                link(ui, "zakon.rada.gov.ua", "https://zakon.rada.gov.ua");
                link(ui, "відкриті дані", "https://data.rada.gov.ua/open");
            });
            if ui.button("Закрити").clicked() {
                close_req = true;
            }
        });
    if close_req {
        open = false;
    }
    app.show_about = open;
}

fn page_button(
    ui: &mut egui::Ui,
    app: &App,
    label: &str,
    page: Page,
    go: &mut Option<Page>,
) {
    let active = app.page == page;
    let disabled = page == Page::Reader && app.reader.is_none();
    let txt = egui::RichText::new(label).strong().size(15.0);
    let btn = egui::Button::new(if active { txt.color(gold()) } else { txt });
    if ui.add_enabled(!disabled, btn).clicked() && !active {
        *go = Some(page);
    }
}

/// Bottom line: index path, documents, db size, live-request counter.
pub fn statusbar(app: &mut App, ctx: &egui::Context) {
    egui::TopBottomPanel::bottom("statusbar")
        .exact_height(22.0)
        .show(ctx, |ui| {
            let sent = app
                .client
                .requests_sent
                .load(std::sync::atomic::Ordering::Relaxed);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("індекс: {}", app.db_path.display())).small(),
                );
                ui.separator();
                match &app.index_status {
                    Some(st) if st.docs > 0 && st.size_bytes > 0 => {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} док.  ·  {:.0} MB",
                                st.docs,
                                st.size_bytes as f64 / 1_048_576.0
                            ))
                            .small(),
                        );
                    }
                    _ => {
                        ui.label(
                            egui::RichText::new("індекс відсутній — «Головна» → «Отримати снапшот»")
                                .small()
                                .color(err()),
                        );
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(format!("live-запитів: {sent}")).small());
                    ui.separator();
                    if ui
                        .button(egui::RichText::new("Про програму").small())
                        .on_hover_text("версія, індекс, джерела даних")
                        .clicked()
                    {
                        app.show_about = true;
                    }
                });
            });
        });
}

/// Theme dropdown («Тема»): returns the id picked this frame.
fn theme_picker(ui: &mut egui::Ui, cur: &str, pick: &mut Option<String>) {
    let label = crate::themes::by_id(cur)
        .map(|t| t.label)
        .unwrap_or("Класична");
    egui::ComboBox::from_id_salt("theme_picker")
        .width(170.0)
        .selected_text(format!("Тема: {label}"))
        .show_ui(ui, |ui| {
            for t in crate::themes::THEMES {
                if ui
                    .selectable_label(cur == t.id, egui::RichText::new(t.label).small())
                    .clicked()
                {
                    *pick = Some(t.id.to_string());
                }
            }
        });
}

/// Transient notifications, bottom-right (auto-expire in `App::update`).
pub fn toasts(app: &mut App, ctx: &egui::Context) {
    if app.toasts.is_empty() {
        return;
    }
    let snapshot: Vec<String> = app.toasts.iter().map(|t| t.text.clone()).collect();
    let colors: Vec<egui::Color32> = app.toasts.iter().map(|t| t.color).collect();
    egui::Area::new(egui::Id::new("toasts"))
        .anchor(egui::Align2::RIGHT_BOTTOM, [-14.0, -34.0])
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                for (t, c) in snapshot.into_iter().rev().zip(colors.iter().rev()) {
                    ui.label(egui::RichText::new(t).color(*c).size(14.0));
                }
            });
        });
    app.toasts.retain(|t| t.born.elapsed() < Duration::from_secs(4));
    if !app.toasts.is_empty() {
        ctx.request_repaint_after(Duration::from_millis(500));
    }
}

#[cfg(test)]
mod tests {
    use super::help_lines;

    #[test]
    fn help_lines_cover_the_real_shortcuts() {
        let lines = help_lines();
        assert!(!lines.is_empty());
        let keys: Vec<&str> = lines.iter().map(|(k, _)| *k).collect();
        // every global handler in App::update has a row here
        for expected in [
            "Ctrl+F",
            "Enter",
            "Enter / Shift+Enter",
            "Esc",
            "Alt+Left / Alt+Right",
            "Alt+PageUp / Alt+PageDown",
            "Ctrl+Plus / Ctrl+Minus",
            "Ctrl+0",
        ] {
            assert!(keys.contains(&expected), "missing row for {expected:?}");
        }
        // no empty descriptions, no non-ASCII in the key column
        // (the bundled egui font has no arrow/emoji glyphs)
        for (k, what) in &lines {
            assert!(!k.trim().is_empty() && !what.trim().is_empty());
            assert!(
                k.is_ascii(),
                "key column must stay ASCII, got {k:?}"
            );
        }
    }
}
