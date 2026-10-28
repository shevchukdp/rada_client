//! Favorites page: every saved annotation across all documents.
//!
//! Rows can be grouped under one header per document (nreg) and filtered
//! client-side by a text needle (annotation text + document title) and a
//! kind chip. Filtering lives in the pure [`row_matches`] /
//! [`filter_and_group`] pair so both render modes share one source.

use eframe::egui;

use crate::app::{err, gold, App};

const KIND_FILTERS: [&str; 4] = ["усі", "коментарі", "виділення", "закладки"];

/// One favorites list row: annotation + document title
/// (empty title when the doc is not indexed).
pub type FavRow = (rada_db::Annotation, String);

pub struct FavoritesState {
    /// (annotation, document title — empty when the doc is not indexed)
    pub items: Vec<FavRow>,
    pub loading: bool,
    pub error: Option<String>,
    /// Client-side text filter over annotation text and document title.
    pub filter: String,
    /// Kind chip: 0 = усі, then [`KIND_FILTERS`] order
    /// (1 коментарі, 2 виділення, 3 закладки).
    pub kind_sel: usize,
    /// Render rows nested under per-document headers instead of flat.
    pub grouped: bool,
    /// First visit triggers an auto-load.
    pub loaded: bool,
}

impl Default for FavoritesState {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            loading: false,
            error: None,
            filter: String::new(),
            kind_sel: 0,
            grouped: true,
            loaded: false,
        }
    }
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

/// Does one row pass the text + kind filters? Case-insensitive substring
/// over the annotation text *and* the document title (plain `to_lowercase`,
/// as the rest of the app filters Ukrainian text with).
pub fn row_matches(a: &rada_db::Annotation, title: &str, needle: &str, kind: usize) -> bool {
    if !matches_filter(&a.kind, kind) {
        return false;
    }
    let q = needle.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }
    a.text.to_lowercase().contains(&q) || title.to_lowercase().contains(&q)
}

/// Filter rows with [`row_matches`], then group them by `nreg`: groups
/// appear in first-appearance order of their rows, and rows keep their
/// input order inside a group. `(nreg, title, rows)`; the group title is
/// the first non-empty title seen among the group's rows.
pub fn filter_and_group(
    rows: &[FavRow],
    needle: &str,
    kind: usize,
) -> Vec<(String, String, Vec<FavRow>)> {
    let mut groups: Vec<(String, String, Vec<FavRow>)> = Vec::new();
    for (a, title) in rows {
        if !row_matches(a, title, needle, kind) {
            continue;
        }
        match groups.iter_mut().find(|(n, _, _)| *n == a.nreg) {
            Some((_, t, rs)) => {
                if t.trim().is_empty() && !title.trim().is_empty() {
                    *t = title.clone();
                }
                rs.push((a.clone(), title.clone()));
            }
            None => groups.push((
                a.nreg.clone(),
                title.clone(),
                vec![(a.clone(), title.clone())],
            )),
        }
    }
    groups
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    if !app.fav.loaded {
        app.load_favorites();
    }

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.heading("Обране");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("оновити").clicked() {
                    app.load_favorites();
                }
                if app.fav.loading {
                    ui.spinner();
                }
                if ui
                    .button(egui::RichText::new("⬇ Експорт .json").small())
                    .on_hover_text("зберегти всі анотації в портативний JSON")
                    .clicked()
                {
                    app.export_annotations();
                }
                if ui
                    .button(egui::RichText::new("⬆ Імпорт .json").small())
                    .on_hover_text("додати анотації з JSON-файлу")
                    .clicked()
                {
                    app.import_annotations();
                }
            });
        });

        // ---- toolbar: text filter + kind chips + grouping toggle -----
        ui.horizontal_wrapped(|ui| {
            ui.add_sized(
                [300.0, 24.0],
                egui::TextEdit::singleline(&mut app.fav.filter)
                    .hint_text("фільтр тексту чи назви…"),
            );
            for (i, lab) in KIND_FILTERS.iter().enumerate() {
                let active = app.fav.kind_sel == i;
                if ui
                    .add(egui::Button::new(egui::RichText::new(*lab).small()).selected(active))
                    .clicked()
                {
                    app.fav.kind_sel = i;
                }
            }
            ui.separator();
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("групувати").small())
                        .selected(app.fav.grouped),
                )
                .on_hover_text("групувати анотації під заголовками документів")
                .clicked()
            {
                app.fav.grouped = !app.fav.grouped;
            }
        });
        if let Some(e) = &app.fav.error {
            ui.label(egui::RichText::new(e).small().color(err()));
        }

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let items = app.fav.items.clone();
                let all_empty = items.is_empty();
                let needle = app.fav.filter.clone();
                let kind = app.fav.kind_sel;
                let mut shown = 0usize;
                if app.fav.grouped {
                    let groups = filter_and_group(&items, &needle, kind);
                    for (nreg, title, rows) in groups {
                        shown += rows.len();
                        let short: String = title.chars().take(90).collect();
                        let head = if short.trim().is_empty() {
                            nreg.clone()
                        } else {
                            short
                        };
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!("{head}  ({})", rows.len()))
                                    .strong()
                                    .color(gold()),
                            );
                            if ui
                                .button(egui::RichText::new("відкрити").small())
                                .on_hover_text("відкрити документ у читалці")
                                .clicked()
                            {
                                let (n, t) = (nreg.clone(), title.clone());
                                app.open_reader_by_nreg(&n, &t, 0, "", "", "");
                            }
                        });
                        ui.indent(egui::Id::new(("fav_group", nreg.clone())), |ui| {
                            for (a, title) in rows {
                                annotation_row(app, ui, &a, &title);
                                ui.add_space(4.0);
                            }
                        });
                        ui.add_space(6.0);
                    }
                } else {
                    let rows: Vec<FavRow> = items
                        .into_iter()
                        .filter(|(a, t)| row_matches(a, t, &needle, kind))
                        .collect();
                    shown = rows.len();
                    for (a, title) in rows {
                        annotation_row(app, ui, &a, &title);
                        ui.add_space(6.0);
                    }
                }
                if !app.fav.loading && app.fav.error.is_none() {
                    if all_empty {
                        ui.label(
                            "тут з’являються збережені коментарі, виділення та закладки \
                             з усіх документів (додаються у «Читалці»)",
                        );
                    } else if shown == 0 {
                        ui.label(
                            egui::RichText::new("нічого не знайдено (перевірте фільтр)")
                                .small()
                                .color(crate::themes::weak()),
                        );
                    }
                }
            });
    });
}

/// One annotation card: kind tag + document line + text (or cached-doc
/// preview for bare bookmarks) + open/delete actions.
fn annotation_row(app: &mut App, ui: &mut egui::Ui, a: &rada_db::Annotation, title: &str) {
    let (tag, color) = kind_tag(&a.kind);
    let short: String = title.chars().take(70).collect();
    let head = if short.trim().is_empty() {
        a.nreg.clone()
    } else {
        format!("{short}  ·  {}", a.nreg)
    };
    egui::Frame::none()
        .stroke(egui::Stroke::new(1.0_f32, crate::themes::border()))
        .inner_margin(egui::Margin::same(6.0))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width() - 6.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(tag).small().color(color));
                ui.label(egui::RichText::new(head).small());
                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        ui.label(egui::RichText::new(&a.created_at).weak().small());
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
            // Context snippet from the cached document text
            // (only when the annotation itself is empty, e.g.
            // a bare bookmark — otherwise the text says it).
            if a.text.trim().is_empty() {
                if let Some(preview) = app.doc_preview(&a.nreg, 160) {
                    ui.label(
                        egui::RichText::new(preview)
                            .small()
                            .italics()
                            .color(crate::themes::weak()),
                    );
                }
            }
            ui.horizontal(|ui| {
                if ui
                    .button(egui::RichText::new("відкрити документ").small())
                    .clicked()
                {
                    let (n, t) = (a.nreg.clone(), title.to_string());
                    app.open_reader_by_nreg(&n, &t, 0, "", "", "");
                }
                if ui
                    .button(egui::RichText::new("видалити").small())
                    .clicked()
                {
                    app.delete_annotation_fresh(a.id);
                }
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ann(id: i64, nreg: &str, kind: &str, text: &str) -> rada_db::Annotation {
        rada_db::Annotation {
            id,
            nreg: nreg.to_string(),
            kind: kind.to_string(),
            text: text.to_string(),
            span_start: None,
            span_end: None,
            created_at: "2026-10-28 10:00:00".to_string(),
        }
    }

    /// Newest-first, two docs interleaved + one title-less doc.
    fn rows() -> Vec<FavRow> {
        vec![
            (
                ann(5, "2961-20", "comment", "треба перечитати"),
                "Про освіту".to_string(),
            ),
            (
                ann(4, "1556-18", "bookmark", ""),
                "Про запобігання корупції".to_string(),
            ),
            (
                ann(3, "2961-20", "highlight", "стаття 7"),
                "Про освіту".to_string(),
            ),
            (
                ann(2, "777-21", "comment", "перевірити дату"),
                String::new(),
            ),
            (
                ann(1, "1556-18", "comment", "підсумок"),
                "Про запобігання корупції".to_string(),
            ),
        ]
    }

    #[test]
    fn empty_needle_passes_all_and_groups_keep_first_appearance_order() {
        let g = filter_and_group(&rows(), "", 0);
        assert_eq!(g.len(), 3, "one header per nreg");
        // Group order = first appearance (5 → 2961-20, 4 → 1556-18, 2 → 777-21).
        assert_eq!(g[0].0, "2961-20");
        assert_eq!(g[1].0, "1556-18");
        assert_eq!(g[2].0, "777-21");
        // Rows keep their input order inside a group.
        assert_eq!(g[0].2.len(), 2);
        assert_eq!(g[0].2[0].0.id, 5);
        assert_eq!(g[0].2[1].0.id, 3);
        assert_eq!(g[1].2[0].0.id, 4);
        assert_eq!(g[1].2[1].0.id, 1);
        assert_eq!(g[0].1, "Про освіту");
        assert!(g[2].1.is_empty(), "title-less doc keeps the empty title");
    }

    #[test]
    fn needle_matches_title_only_rows() {
        // «корупці» occurs only in document titles, never in annotation text.
        let g = filter_and_group(&rows(), "корупці", 0);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].0, "1556-18");
        assert_eq!(g[0].1, "Про запобігання корупції");
        // Both rows of the matching document pass (title match lifts them).
        assert_eq!(g[0].2.len(), 2);
    }

    #[test]
    fn needle_is_case_insensitive_over_annotation_text() {
        let g = filter_and_group(&rows(), "ПЕРЕЧИТАТИ", 0);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].0, "2961-20");
        assert_eq!(g[0].2.len(), 1);
        assert_eq!(g[0].2[0].0.id, 5);
    }

    #[test]
    fn kind_filter_picks_only_one_kind() {
        // 3 = закладки: the single bookmark row survives.
        let g = filter_and_group(&rows(), "", 3);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].0, "1556-18");
        assert_eq!(g[0].2.len(), 1);
        assert_eq!(g[0].2[0].0.kind, "bookmark");
        // 1 = коментарі: all three comments, across all their docs.
        let g = filter_and_group(&rows(), "", 1);
        let kinds: Vec<&str> = g
            .iter()
            .flat_map(|(_, _, rs)| rs.iter().map(|(a, _)| a.kind.as_str()))
            .collect();
        assert_eq!(kinds, vec!["comment"; 3]);
    }

    #[test]
    fn no_match_gives_no_groups() {
        assert!(filter_and_group(&rows(), "нічого подібного", 0).is_empty());
        assert!(filter_and_group(&[], "", 0).is_empty());
    }
}
