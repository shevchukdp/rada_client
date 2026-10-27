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
                    ui.label(
                        egui::RichText::new(format!("{} с/запит", app.pace_secs))
                            .small(),
                    );
                    ui.separator();
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
