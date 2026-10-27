//! Favorites page: every saved annotation across all documents.

use eframe::egui;

use crate::app::{err, App};

const KIND_FILTERS: [&str; 4] = ["усі", "коментарі", "виділення", "закладки"];

#[derive(Default)]
pub struct FavoritesState {
    /// (annotation, document title — empty when the doc is not indexed)
    pub items: Vec<(rada_db::Annotation, String)>,
    pub loading: bool,
    pub error: Option<String>,
    pub filter: usize,
    /// First visit triggers an auto-load.
    pub loaded: bool,
}

fn kind_tag(kind: &str) -> (&'static str, egui::Color32) {
    match kind {
        "highlight" => ("виділення", crate::themes::accent()),
        "bookmark" => ("закладка", crate::themes::info()),
        _ => ("коментар", crate::themes::weak()),
    }
}

fn matches_filter(kind: &str, filter: usize) -> bool {
    match filter {
        1 => kind == "comment",
        2 => kind == "highlight",
        3 => kind == "bookmark",
        _ => true,
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    if !app.fav.loaded {
        app.load_favorites();
    }

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.heading("Обране");
            ui.separator();
            for (i, lab) in KIND_FILTERS.iter().enumerate() {
                if ui.selectable_label(app.fav.filter == i, *lab).clicked() {
                    app.fav.filter = i;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("оновити").clicked() {
                    app.load_favorites();
                }
                if app.fav.loading {
                    ui.spinner();
                }
            });
        });
        if let Some(e) = &app.fav.error {
            ui.label(egui::RichText::new(e).small().color(err()));
        }

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if !app.fav.loading && app.fav.error.is_none() && app.fav.items.is_empty() {
                    ui.label(
                        "тут з’являються збережені коментарі, виділення та закладки \
                         з усіх документів (додаються у «Читалці»)",
                    );
                }
                let items = app.fav.items.clone();
                for (a, title) in items {
                    if !matches_filter(&a.kind, app.fav.filter) {
                        continue;
                    }
                    let (tag, color) = kind_tag(&a.kind);
                    let short: String = title.chars().take(70).collect();
                    let head = if short.trim().is_empty() {
                        a.nreg.clone()
                    } else {
                        format!("{short}  ·  {}", a.nreg)
                    };
                    egui::Frame::none()
                        .stroke(egui::Stroke::new(
                            1.0_f32,
                            crate::themes::border(),
                        ))
                        .inner_margin(egui::Margin::same(6.0))
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width() - 6.0);
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(tag).small().color(color));
                                ui.label(egui::RichText::new(head).small());
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            egui::RichText::new(&a.created_at).weak().small(),
                                        );
                                    },
                                );
                            });
                            if !a.text.trim().is_empty() {
                                ui.label(
                                    egui::RichText::new(&a.text)
                                        .small()
                                        .color(crate::themes::weak()),
                                );
                            }
                            ui.horizontal(|ui| {
                                if ui.button(egui::RichText::new("відкрити документ").small()).clicked() {
                                    let (n, t) = (a.nreg.clone(), title.clone());
                                    app.open_reader_by_nreg(&n, &t, 0, "", "", "");
                                }
                                if ui.button(egui::RichText::new("видалити").small()).clicked() {
                                    app.delete_annotation_fresh(a.id);
                                }
                            });
                        });
                    ui.add_space(6.0);
                }
            });
    });
}
