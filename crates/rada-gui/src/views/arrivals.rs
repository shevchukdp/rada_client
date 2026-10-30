//! Arrivals: the live feed of recently updated documents (`r.tsv`).

use eframe::egui;

use crate::app::{err, gold, App};

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
    /// Nregs absent from the persisted seen-set at the last successful load
    /// (the «НОВЕ» badges; also what «лише нові» filters by).
    pub new_nregs: Vec<String>,
    /// Show only the `new_nregs` documents.
    pub only_new: bool,
    /// Unix seconds of the last successful load (live or snapshot), for the
    /// «оновлено: …» relative time.
    pub fetched_at: Option<u64>,
}

/// Relative age for the arrivals toolbar: «щойно», «N хв тому», «N год тому»,
/// «N дн тому» (hour granularity after a day; monotonic-safe: `now < then`
/// counts as «щойно»).
pub fn fmt_since(now: u64, then: u64) -> String {
    let d = now.saturating_sub(then);
    if d < 60 {
        "щойно".to_string()
    } else if d < 3600 {
        format!("{} хв тому", d / 60)
    } else if d < 86_400 {
        format!("{} год тому", d / 3600)
    } else {
        format!("{} дн тому", d / 86_400)
    }
}

/// Nregs of `docs` that are not in `seen` (order preserved).
pub fn unseen_docs(docs: &[rada_core::models::UpdatedDoc], seen: &[String]) -> Vec<String> {
    docs.iter()
        .map(|d| d.nreg.clone())
        .filter(|n| !seen.contains(n))
        .collect()
}

/// The docs the arrivals page shows: «лише нові» (`only_new` keeps only
/// `fresh` nregs) then the substring filter over title/nreg/publication
/// (case-insensitive). One source for the list, the CSV export and the
/// Ctrl+Enter «open the top arrival» shortcut.
pub fn visible_arrivals(
    docs: &[rada_core::models::UpdatedDoc],
    filter: &str,
    fresh: &[String],
    only_new: bool,
) -> Vec<rada_core::models::UpdatedDoc> {
    let f = filter.trim().to_lowercase();
    docs.iter()
        .filter(|d| !only_new || fresh.contains(&d.nreg))
        .filter(|d| {
            f.is_empty()
                || d.nazva.to_lowercase().contains(&f)
                || d.nreg.to_lowercase().contains(&f)
                || d.publics.to_lowercase().contains(&f)
        })
        .cloned()
        .collect()
}

/// CSV-quote one field (doubled quotes; quoted when it contains a comma,
/// quote or line break). Shared with the home year-window export.
pub(crate) fn csv_esc(s: &str) -> String {
    if s.contains(['"', ',', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// The arrivals feed rendered as CSV (UTF-8 with BOM so Excel opens
/// Cyrillic correctly): `nreg,status,nazva,publics`.
pub fn arrivals_csv(docs: &[rada_core::models::UpdatedDoc]) -> String {
    let mut out = String::from("\u{FEFF}nreg,status,nazva,publics\n");
    for d in docs {
        out.push_str(&csv_esc(&d.nreg));
        out.push(',');
        out.push_str(&csv_esc(&d.status));
        out.push(',');
        out.push_str(&csv_esc(&d.nazva));
        out.push(',');
        out.push_str(&csv_esc(&d.publics));
        out.push('\n');
    }
    out
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
            new_nregs: Vec::new(),
            only_new: false,
            fetched_at: None,
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
                if let Some(t) = app.arrivals.fetched_at {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(t);
                    ui.label(
                        egui::RichText::new(format!("оновлено: {}", fmt_since(now, t)))
                            .small()
                            .weak(),
                    );
                }
                if !app.arrivals.new_nregs.is_empty() {
                    ui.label(
                        egui::RichText::new(format!("нових: {}", app.arrivals.new_nregs.len()))
                            .small()
                            .strong()
                            .color(gold()),
                    );
                }
                let on = app.arrivals.only_new;
                if ui
                    .add(egui::Button::new(egui::RichText::new("лише нові").small()).selected(on))
                    .on_hover_text("показати лише документи, яких ви ще не бачили")
                    .clicked()
                {
                    app.arrivals.only_new = !on;
                }
                if ui
                    .button(egui::RichText::new("CSV").small())
                    .on_hover_text("зберегти показаний перелік у файл CSV (UTF-8 з BOM)")
                    .clicked()
                {
                    let f = app.arrivals.filter.clone();
                    let fresh = app.arrivals.new_nregs.clone();
                    let only_new = app.arrivals.only_new;
                    let docs = visible_arrivals(&app.arrivals.docs, &f, &fresh, only_new);
                    app.export_arrivals_csv(docs);
                }
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
            let want_f = app.take_focus(crate::app::FocusReq::ArrivalsFilter);
            let fr = ui.add_sized(
                [280.0, 24.0],
                egui::TextEdit::singleline(&mut app.arrivals.filter)
                    .hint_text("назва, номер чи публікація"),
            );
            if want_f {
                fr.request_focus();
            }
            // Persist when the edit commits (focus lost) — not per keystroke.
            if fr.lost_focus() && app.settings.arr_filter != app.arrivals.filter {
                app.settings.arr_filter = app.arrivals.filter.clone();
                crate::settings::save(&app.settings);
            }
            if !app.arrivals.filter.is_empty()
                && ui
                    .button(egui::RichText::new("×").small())
                    .clicked()
            {
                app.arrivals.filter.clear();
                if !app.settings.arr_filter.is_empty() {
                    app.settings.arr_filter.clear();
                    crate::settings::save(&app.settings);
                }
            }
        });
        ui.add_space(4.0);
        ui.separator();

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let f = app.arrivals.filter.clone();
                let fresh = app.arrivals.new_nregs.clone();
                let only_new = app.arrivals.only_new;
                let docs = visible_arrivals(&app.arrivals.docs, &f, &fresh, only_new);
                let f = f.trim().to_lowercase();
                if docs.is_empty() && !f.is_empty() {
                    ui.label(
                        egui::RichText::new("під фільтр нічого не підійшло")
                            .small()
                            .weak(),
                    );
                }
                if docs.is_empty() && only_new && f.is_empty() {
                    ui.label(
                        egui::RichText::new("нових документів немає — усі перечитані")
                            .small()
                            .weak(),
                    );
                }
                for d in docs {
                    let is_new = fresh.contains(&d.nreg);
                    let meta = format!("{}   [{}]   pub: {}", d.nreg, d.status, d.publics);
                    let doc = format!("{}\n{}", d.nazva, meta);
                    let rt = egui::RichText::new(doc).small();
                    let rt = if is_new { rt.color(gold()) } else { rt };
                    if ui
                        .add_sized(
                            [ui.available_width(), 44.0],
                            egui::Button::new(rt).wrap(),
                        )
                        .on_hover_text(if is_new { "новий з минулого перегляду" } else { "" })
                        .clicked()
                    {
                        app.open_reader_by_nreg(&d.nreg, &d.nazva, 0, "", "", "");
                    }
                }
            });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_arrivals_applies_only_new_and_substring_filter() {
        let doc = |num: i64, nreg: &str, nazva: &str, publics: &str| rada_core::models::UpdatedDoc {
            num,
            nreg: nreg.into(),
            nazva: nazva.into(),
            status: "Чинний".into(),
            publics: publics.into(),
            file: String::new(),
            link: String::new(),
            size: None,
        };
        let docs = vec![
            doc(1, "1-23", "Про освіту", "Верховна Рада"),
            doc(2, "2-24", "Податковий кодекс", "Верховна Рада"),
            doc(3, "3-25", "Про практику", "Кабмін"),
        ];
        // no filter, no only_new: everything
        assert_eq!(visible_arrivals(&docs, "", &[], false).len(), 3);
        // substring on title / nreg / publication (case-insensitive, trimmed)
        let n: Vec<String> = visible_arrivals(&docs, " освіт ", &[], false)
            .into_iter()
            .map(|d| d.nreg)
            .collect();
        assert_eq!(n, ["1-23"]);
        assert_eq!(visible_arrivals(&docs, "2-24", &[], false)[0].nazva, "Податковий кодекс");
        assert_eq!(visible_arrivals(&docs, "кабмін", &[], false).len(), 1);
        // only_new keeps fresh nregs only, then the filter applies on top
        let fresh = vec!["2-24".to_string(), "3-25".to_string()];
        let n: Vec<String> = visible_arrivals(&docs, "", &fresh, true)
            .into_iter()
            .map(|d| d.nreg)
            .collect();
        assert_eq!(n, ["2-24", "3-25"]);
        let n: Vec<String> = visible_arrivals(&docs, "практику", &fresh, true)
            .into_iter()
            .map(|d| d.nreg)
            .collect();
        assert_eq!(n, ["3-25"]);
        // only_new with an empty fresh set shows nothing
        assert!(visible_arrivals(&docs, "", &[], true).is_empty());
    }

    #[test]
    fn fmt_since_relative_buckets() {
        assert_eq!(fmt_since(1000, 1000), "щойно");
        assert_eq!(fmt_since(1059, 1000), "щойно", "under a minute");
        assert_eq!(fmt_since(1060, 1000), "1 хв тому");
        assert_eq!(fmt_since(1000 + 3599, 1000), "59 хв тому");
        assert_eq!(fmt_since(1000 + 3600, 1000), "1 год тому");
        assert_eq!(fmt_since(1000 + 86_399, 1000), "23 год тому");
        assert_eq!(fmt_since(1000 + 86_400, 1000), "1 дн тому");
        assert_eq!(fmt_since(1000 + 3 * 86_400 + 700, 1000), "3 дн тому");
        // clock skew never panics nor goes negative
        assert_eq!(fmt_since(900, 1000), "щойно");
    }

    fn doc(nreg: &str) -> rada_core::models::UpdatedDoc {
        rada_core::models::UpdatedDoc {
            num: 1,
            nreg: nreg.into(),
            nazva: "н".into(),
            status: "Чинний".into(),
            publics: String::new(),
            link: String::new(),
            size: None,
            file: String::new(),
        }
    }

    #[test]
    fn unseen_docs_keeps_order_and_new_only() {        let docs = vec![doc("1-1"), doc("2-2"), doc("3-3")];
        let seen = vec!["2-2".to_string()];
        assert_eq!(unseen_docs(&docs, &seen), ["1-1", "3-3"]);
        assert_eq!(unseen_docs(&docs, &[]).len(), 3, "nothing seen: all new");
        let all = vec!["1-1".to_string(), "2-2".to_string(), "3-3".to_string()];
        assert!(unseen_docs(&docs, &all).is_empty(), "all seen: nothing new");
    }

    #[test]
    fn arrivals_csv_escapes_and_boms() {
        let mut d = doc("1-1");
        d.status = "Чинний".into();
        d.nazva = "Про \"зміни\", частина 1".into();
        d.publics = "Бюлетень\n2024".into();
        let csv = arrivals_csv(&[d, doc("2-2")]);
        // Exact whole-output check: BOM, header, quoted specials (doubled
        // quotes, comma and newline inside quotes), plain row unquoted.
        assert_eq!(
            csv,
            "\u{FEFF}nreg,status,nazva,publics\n\
             1-1,Чинний,\"Про \"\"зміни\"\", частина 1\",\"Бюлетень\n2024\"\n\
             2-2,Чинний,н,\n"
        );
        assert!(arrivals_csv(&[]).ends_with("publics\n"), "header-only ok");
    }
}
