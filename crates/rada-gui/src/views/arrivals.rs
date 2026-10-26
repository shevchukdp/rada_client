//! Arrivals: the live feed of recently updated documents (`r.tsv`).

use eframe::egui;

use crate::app::{err, App};

pub struct ArrivalsState {
    pub run: u64,
    pub loading: bool,
    pub docs: Vec<rada_core::models::UpdatedDoc>,
    pub error: Option<String>,
}

impl Default for ArrivalsState {
    fn default() -> Self {
        Self {
            run: 0,
            loading: false,
            docs: Vec::new(),
            error: None,
        }
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    // auto-load on first visit
    if app.arrivals.docs.is_empty() && !app.arrivals.loading && app.arrivals.error.is_none() {
        app.load_arrivals();
    }

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading(
            egui::RichText::new("Оновлення документів (наживо)")
                .strong()
                .size(17.0),
        );
        ui.label(
            egui::RichText::new(
                "Фід /laws/main/r.tsv — документи, змінені за останні години на сервері. \
                 Діє пауза 5 с між запитами.",
            )
            .small(),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new(egui::RichText::new("⟳ Оновити фід").strong())).clicked() {
                app.load_arrivals();
            }
            if app.arrivals.loading {
                ui.spinner();
            }
            if !app.arrivals.docs.is_empty() {
                ui.label(
                    egui::RichText::new(format!("{} документів", app.arrivals.docs.len())).small(),
                );
            }
            if let Some(e) = &app.arrivals.error {
                ui.label(egui::RichText::new(e).small().color(err()));
            }
        });
        ui.add_space(4.0);
        ui.separator();

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for d in app.arrivals.docs.clone() {
                    let meta = format!("{}   [{}]   pub: {}", d.nreg, d.status, d.publics);
                    let doc = format!("{}\n{}", d.nazva, meta);
                    if ui
                        .add_sized(
                            [ui.available_width(), 44.0],
                            egui::Button::new(egui::RichText::new(doc).small()).wrap(),
                        )
                        .clicked()
                    {
                        app.open_reader_by_nreg(&d.nreg, &d.nazva, 0, "", "", "");
                    }
                }
            });
    });
}
