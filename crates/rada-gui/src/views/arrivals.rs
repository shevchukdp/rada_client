//! Arrivals: the live feed of recently updated documents (`r.tsv`).

use eframe::egui;

use crate::app::{err, App};

pub struct ArrivalsState {
    pub run: u64,
    pub loading: bool,
    pub docs: Vec<rada_core::models::UpdatedDoc>,
    pub error: Option<String>,
    /// The shown feed is the last saved snapshot (live fetch failed/offline).
    pub from_cache: bool,
    /// Save time of that snapshot (`YYYY-MM-DD HH:MM:SS`).
    pub saved_at: Option<String>,
    /// Client-side substring filter over title/nreg/publication.
    pub filter: String,
}

impl Default for ArrivalsState {
    fn default() -> Self {
        Self {
            run: 0,
            loading: false,
            docs: Vec::new(),
            error: None,
            from_cache: false,
            saved_at: None,
            filter: String::new(),
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
            if app.arrivals.from_cache {
                ui.label(
                    egui::RichText::new(" збережене ")
                        .small()
                        .strong()
                        .color(egui::Color32::BLACK)
                        .background_color(egui::Color32::from_rgb(120, 170, 255)),
                )
                .on_hover_text(
                    "живий фід недоступний — показано останній збережений знімок",
                );
            }
            if let Some(at) = &app.arrivals.saved_at {
                ui.label(egui::RichText::new(format!("збережено: {at}")).small().weak());
            }
            if let Some(e) = &app.arrivals.error {
                ui.label(egui::RichText::new(e).small().color(err()));
            }
        });
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Фільтр:").small());
            let n = app.arrivals.filter.clone();
            ui.add_sized(
                [280.0, 24.0],
                egui::TextEdit::singleline(&mut app.arrivals.filter)
                    .hint_text("назва, номер чи публікація"),
            );
            if n != app.arrivals.filter && !app.arrivals.filter.is_empty() {
                // typing narrows without a reload — nothing else to do.
            }
            if !app.arrivals.filter.is_empty()
                && ui
                    .button(egui::RichText::new("×").small())
                    .clicked()
            {
                app.arrivals.filter.clear();
            }
        });
        ui.add_space(4.0);
        ui.separator();

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let f = app.arrivals.filter.trim().to_lowercase();
                let docs: Vec<rada_core::models::UpdatedDoc> = app
                    .arrivals
                    .docs
                    .iter()
                    .filter(|d| {
                        f.is_empty()
                            || d.nazva.to_lowercase().contains(&f)
                            || d.nreg.to_lowercase().contains(&f)
                            || d.publics.to_lowercase().contains(&f)
                    })
                    .cloned()
                    .collect();
                if docs.is_empty() && !f.is_empty() {
                    ui.label(
                        egui::RichText::new("під фільтр нічого не підійшло")
                            .small()
                            .weak(),
                    );
                }
                for d in docs {
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
