//! Named color themes, ids shared 1:1 with the embedded web UI.
//!
//! `apply` rebuilds egui `Visuals` from a dark/light base plus one palette;
//! the semantic colors (gold/ok/err/info/border) are additionally published
//! through atomics so the free helpers the views use (`gold()`, `err()`, …)
//! become theme-aware without threading state through every widget.

use std::sync::atomic::{AtomicU32, Ordering};

use eframe::egui;

#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    /// Window / central / panel background.
    pub bg: egui::Color32,
    /// Text edits and sunken areas.
    pub panel: egui::Color32,
    /// Hover fills, alternating rows, fading.
    pub faint: egui::Color32,
    /// Borders, separators, window outlines.
    pub muted: egui::Color32,
    pub text: egui::Color32,
    pub weak: egui::Color32,
    /// Brand color: «§», active page, highlight marks.
    pub accent: egui::Color32,
    pub ok: egui::Color32,
    pub err: egui::Color32,
    /// Links and informational accents.
    pub info: egui::Color32,
}

pub struct ThemeDef {
    pub id: &'static str,
    pub label: &'static str,
    pub pal: Palette,
}

/// All 17 themes (ids must stay in sync with `ui/index.html` `THEMES`).
pub const THEMES: &[ThemeDef] = &[
    ThemeDef {
        id: "classical",
        label: "Класична",
        pal: Palette {
            dark: true,
            bg: rgb(0x1A, 0x22, 0x33),
            panel: rgb(0x14, 0x1B, 0x2A),
            faint: rgb(0x22, 0x2C, 0x42),
            muted: rgb(0x33, 0x3F, 0x5A),
            text: rgb(0xEC, 0xEF, 0xF6),
            weak: rgb(0x9A, 0xA6, 0xBC),
            accent: rgb(0xFF, 0xB3, 0x00),
            ok: rgb(0x9A, 0xCC, 0x80),
            err: rgb(0xE6, 0x6A, 0x6A),
            info: rgb(0x7A, 0xC7, 0xE4),
        },
    },
    ThemeDef {
        id: "white",
        label: "Біла",
        pal: Palette {
            dark: false,
            bg: rgb(0xFB, 0xFB, 0xFD),
            panel: rgb(0xFF, 0xFF, 0xFF),
            faint: rgb(0xE7, 0xEC, 0xF3),
            muted: rgb(0xC9, 0xCE, 0xD6),
            text: rgb(0x1B, 0x1D, 0x21),
            weak: rgb(0x6E, 0x76, 0x81),
            accent: rgb(0x1F, 0x6F, 0xEB),
            ok: rgb(0x1F, 0x7A, 0x1F),
            err: rgb(0xC9, 0x3A, 0x3A),
            info: rgb(0x1F, 0x6F, 0xEB),
        },
    },
    ThemeDef {
        id: "black",
        label: "Чорна",
        pal: Palette {
            dark: true,
            bg: rgb(0x00, 0x00, 0x00),
            panel: rgb(0x0A, 0x0A, 0x0A),
            faint: rgb(0x16, 0x16, 0x16),
            muted: rgb(0x3A, 0x3A, 0x3A),
            text: rgb(0xF2, 0xF2, 0xF2),
            weak: rgb(0x8A, 0x8A, 0x8A),
            accent: rgb(0xFF, 0xD2, 0x4D),
            ok: rgb(0x59, 0xD0, 0x8A),
            err: rgb(0xFF, 0x6B, 0x6B),
            info: rgb(0x6E, 0xC8, 0xFF),
        },
    },
    ThemeDef {
        id: "invert",
        label: "Інкверт",
        pal: Palette {
            dark: false,
            bg: rgb(0xFF, 0xFF, 0xFF),
            panel: rgb(0xF0, 0xF0, 0xF4),
            faint: rgb(0xE3, 0xE3, 0xEA),
            muted: rgb(0xB8, 0xB8, 0xC2),
            text: rgb(0x00, 0x00, 0x00),
            weak: rgb(0x55, 0x55, 0x66),
            accent: rgb(0x7A, 0x00, 0xCC),
            ok: rgb(0x00, 0x8A, 0x00),
            err: rgb(0xD0, 0x00, 0x00),
            info: rgb(0x00, 0x60, 0xC8),
        },
    },
    ThemeDef {
        id: "auto",
        label: "Авто (системна)",
        // Placeholder: resolved by `resolve` from the OS theme.
        pal: ThemeDef::P_CLASSICAL,
    },
    ThemeDef {
        id: "matrix",
        label: "«Матриця»",
        pal: Palette {
            dark: true,
            bg: rgb(0x00, 0x06, 0x00),
            panel: rgb(0x00, 0x08, 0x00),
            faint: rgb(0x06, 0x26, 0x0F),
            muted: rgb(0x06, 0x39, 0x0F),
            text: rgb(0x22, 0xDD, 0x55),
            weak: rgb(0x0F, 0xA3, 0x4A),
            accent: rgb(0x33, 0xFF, 0x77),
            ok: rgb(0x33, 0xFF, 0x77),
            err: rgb(0xFF, 0x50, 0x50),
            info: rgb(0x33, 0xFF, 0x77),
        },
    },
    ThemeDef {
        id: "opencode",
        label: "opencode",
        pal: Palette {
            dark: true,
            bg: rgb(0x17, 0x19, 0x23),
            panel: rgb(0x10, 0x11, 0x18),
            faint: rgb(0x22, 0x25, 0x33),
            muted: rgb(0x2A, 0x2D, 0x3D),
            text: rgb(0xD8, 0xDE, 0xE9),
            weak: rgb(0x7C, 0x81, 0x99),
            accent: rgb(0xFF, 0x7A, 0x1A),
            ok: rgb(0x4E, 0xD4, 0x8B),
            err: rgb(0xFF, 0x55, 0x66),
            info: rgb(0x7D, 0xC3, 0xFF),
        },
    },
    ThemeDef {
        id: "nord",
        label: "Nord",
        pal: Palette {
            dark: true,
            bg: rgb(0x2E, 0x34, 0x40),
            panel: rgb(0x24, 0x29, 0x33),
            faint: rgb(0x3B, 0x42, 0x52),
            muted: rgb(0x43, 0x4C, 0x5E),
            text: rgb(0xEC, 0xEF, 0xF4),
            weak: rgb(0xA9, 0xB1, 0xC0),
            accent: rgb(0x88, 0xC0, 0xD0),
            ok: rgb(0xA3, 0xBE, 0x8C),
            err: rgb(0xBF, 0x61, 0x6A),
            info: rgb(0x81, 0xA1, 0xC1),
        },
    },
    ThemeDef {
        id: "dracula",
        label: "Dracula",
        pal: Palette {
            dark: true,
            bg: rgb(0x28, 0x2A, 0x36),
            panel: rgb(0x21, 0x22, 0x2C),
            faint: rgb(0x34, 0x37, 0x46),
            muted: rgb(0x44, 0x47, 0x5A),
            text: rgb(0xF8, 0xF8, 0xF2),
            weak: rgb(0x9A, 0x9C, 0xA9),
            accent: rgb(0xBD, 0x93, 0xF9),
            ok: rgb(0x50, 0xFA, 0x7B),
            err: rgb(0xFF, 0x55, 0x55),
            info: rgb(0x8B, 0xE9, 0xFD),
        },
    },
    ThemeDef {
        id: "solarized-dark",
        label: "Solarized Dark",
        pal: Palette {
            dark: true,
            bg: rgb(0x00, 0x2B, 0x36),
            panel: rgb(0x00, 0x21, 0x2B),
            faint: rgb(0x07, 0x3C, 0x46),
            muted: rgb(0x0B, 0x4F, 0x5F),
            text: rgb(0x93, 0xA1, 0xA1),
            weak: rgb(0x65, 0x7B, 0x83),
            accent: rgb(0x26, 0x8B, 0xD2),
            ok: rgb(0x85, 0x99, 0x00),
            err: rgb(0xDC, 0x32, 0x2F),
            info: rgb(0x26, 0x8B, 0xD2),
        },
    },
    ThemeDef {
        id: "solarized-light",
        label: "Solarized Light",
        pal: Palette {
            dark: false,
            bg: rgb(0xFD, 0xF6, 0xE3),
            panel: rgb(0xFF, 0xFA, 0xF0),
            faint: rgb(0xE6, 0xE0, 0xCB),
            muted: rgb(0xD9, 0xD2, 0xB9),
            text: rgb(0x58, 0x6E, 0x75),
            weak: rgb(0x93, 0xA1, 0xA1),
            accent: rgb(0x26, 0x8B, 0xD2),
            ok: rgb(0x85, 0x99, 0x00),
            err: rgb(0xDC, 0x32, 0x2F),
            info: rgb(0x26, 0x8B, 0xD2),
        },
    },
    ThemeDef {
        id: "gruvbox-dark",
        label: "Gruvbox Dark",
        pal: Palette {
            dark: true,
            bg: rgb(0x28, 0x28, 0x28),
            panel: rgb(0x1D, 0x20, 0x21),
            faint: rgb(0x3C, 0x38, 0x36),
            muted: rgb(0x50, 0x49, 0x45),
            text: rgb(0xEB, 0xDB, 0xB2),
            weak: rgb(0xA8, 0x99, 0x84),
            accent: rgb(0xFA, 0xBD, 0x2F),
            ok: rgb(0xB8, 0xBB, 0x26),
            err: rgb(0xFB, 0x49, 0x34),
            info: rgb(0x83, 0xA5, 0x98),
        },
    },
    ThemeDef {
        id: "monokai",
        label: "Monokai",
        pal: Palette {
            dark: true,
            bg: rgb(0x27, 0x28, 0x22),
            panel: rgb(0x1E, 0x1F, 0x1B),
            faint: rgb(0x3E, 0x3D, 0x31),
            muted: rgb(0x48, 0x48, 0x3F),
            text: rgb(0xF8, 0xF8, 0xF2),
            weak: rgb(0x75, 0x71, 0x5E),
            accent: rgb(0xF9, 0x26, 0x72),
            ok: rgb(0xA6, 0xE2, 0x2E),
            err: rgb(0xF9, 0x26, 0x72),
            info: rgb(0x66, 0xD9, 0xEF),
        },
    },
    ThemeDef {
        id: "one-dark",
        label: "One Dark",
        pal: Palette {
            dark: true,
            bg: rgb(0x28, 0x2C, 0x34),
            panel: rgb(0x1B, 0x1E, 0x23),
            faint: rgb(0x3A, 0x3F, 0x4B),
            muted: rgb(0x4B, 0x52, 0x63),
            text: rgb(0xAB, 0xB2, 0xBF),
            weak: rgb(0x7F, 0x84, 0x8E),
            accent: rgb(0xE5, 0xC0, 0x7B),
            ok: rgb(0x98, 0xC3, 0x79),
            err: rgb(0xE0, 0x6C, 0x75),
            info: rgb(0x61, 0xAF, 0xEF),
        },
    },
    ThemeDef {
        id: "tokyo-night",
        label: "Tokyo Night",
        pal: Palette {
            dark: true,
            bg: rgb(0x1A, 0x1B, 0x26),
            panel: rgb(0x16, 0x16, 0x1E),
            faint: rgb(0x29, 0x2E, 0x42),
            muted: rgb(0x2F, 0x34, 0x50),
            text: rgb(0xA9, 0xB1, 0xD6),
            weak: rgb(0x56, 0x5F, 0x89),
            accent: rgb(0xE0, 0xAF, 0x68),
            ok: rgb(0x9E, 0xCE, 0x6A),
            err: rgb(0xF7, 0x76, 0x8E),
            info: rgb(0x7A, 0xA2, 0xF7),
        },
    },
    ThemeDef {
        id: "catppuccin",
        label: "Catppuccin",
        pal: Palette {
            dark: true,
            bg: rgb(0x1E, 0x1E, 0x2E),
            panel: rgb(0x18, 0x18, 0x25),
            faint: rgb(0x31, 0x32, 0x44),
            muted: rgb(0x45, 0x47, 0x5A),
            text: rgb(0xCD, 0xD6, 0xF4),
            weak: rgb(0x6C, 0x70, 0x86),
            accent: rgb(0xFA, 0xB3, 0x87),
            ok: rgb(0xA6, 0xE3, 0xA1),
            err: rgb(0xF3, 0x8B, 0xA8),
            info: rgb(0x89, 0xB4, 0xFA),
        },
    },
    ThemeDef {
        id: "github-dark",
        label: "GitHub Dark",
        pal: Palette {
            dark: true,
            bg: rgb(0x0D, 0x11, 0x17),
            panel: rgb(0x01, 0x04, 0x09),
            faint: rgb(0x21, 0x26, 0x2D),
            muted: rgb(0x30, 0x36, 0x3D),
            text: rgb(0xC9, 0xD1, 0xD9),
            weak: rgb(0x8B, 0x94, 0x9E),
            accent: rgb(0xD2, 0x99, 0x22),
            ok: rgb(0x3F, 0xB9, 0x50),
            err: rgb(0xF8, 0x51, 0x49),
            info: rgb(0x58, 0xA6, 0xFF),
        },
    },
];

impl ThemeDef {
    /// Fallback used when the system theme is unknown.
    const P_CLASSICAL: Palette = Palette {
        dark: true,
        bg: egui::Color32::from_rgb(0x1A, 0x22, 0x33),
        panel: egui::Color32::from_rgb(0x14, 0x1B, 0x2A),
        faint: egui::Color32::from_rgb(0x22, 0x2C, 0x42),
        muted: egui::Color32::from_rgb(0x33, 0x3F, 0x5A),
        text: egui::Color32::from_rgb(0xEC, 0xEF, 0xF6),
        weak: egui::Color32::from_rgb(0x9A, 0xA6, 0xBC),
        accent: egui::Color32::from_rgb(0xFF, 0xB3, 0x00),
        ok: egui::Color32::from_rgb(0x9A, 0xCC, 0x80),
        err: egui::Color32::from_rgb(0xE6, 0x6A, 0x6A),
        info: egui::Color32::from_rgb(0x7A, 0xC7, 0xE4),
    };
}

const fn rgb(r: u8, g: u8, b: u8) -> egui::Color32 {
    egui::Color32::from_rgb(r, g, b)
}

pub fn by_id(id: &str) -> Option<&'static ThemeDef> {
    THEMES.iter().find(|t| t.id == id)
}

/// Map a (possibly `auto`) id to the palette in effect, `sys` = OS theme.
pub fn resolve(id: &str, sys: Option<egui::Theme>) -> &'static Palette {
    if id == "auto" {
        match sys {
            Some(egui::Theme::Light) => &by_id("white").unwrap().pal,
            _ => &by_id("classical").unwrap().pal,
        }
    } else {
        &by_id(id)
            .or_else(|| by_id("classical"))
            .unwrap()
            .pal
    }
}

/// Semantic slots republished for the stateless helper functions.
static GOLD: AtomicU32 = AtomicU32::new(0);
static OK: AtomicU32 = AtomicU32::new(0);
static ERR: AtomicU32 = AtomicU32::new(0);
static INFO: AtomicU32 = AtomicU32::new(0);
static BORDER: AtomicU32 = AtomicU32::new(0);
static WEAK: AtomicU32 = AtomicU32::new(0);

fn put(a: &AtomicU32, c: egui::Color32) {
    let [r, g, b, al] = c.to_array();
    a.store(((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | al as u32, Ordering::Relaxed);
}

fn get(a: &AtomicU32, fallback: egui::Color32) -> egui::Color32 {
    let v = a.load(Ordering::Relaxed);
    if v == 0 {
        return fallback;
    }
    egui::Color32::from_rgba_unmultiplied(
        (v >> 24) as u8,
        (v >> 16) as u8,
        (v >> 8) as u8,
        v as u8,
    )
}

pub fn accent() -> egui::Color32 {
    get(&GOLD, ThemeDef::P_CLASSICAL.accent)
}
pub fn ok() -> egui::Color32 {
    get(&OK, ThemeDef::P_CLASSICAL.ok)
}
pub fn err() -> egui::Color32 {
    get(&ERR, ThemeDef::P_CLASSICAL.err)
}
pub fn info() -> egui::Color32 {
    get(&INFO, ThemeDef::P_CLASSICAL.info)
}
pub fn border() -> egui::Color32 {
    get(&BORDER, ThemeDef::P_CLASSICAL.muted)
}
pub fn weak() -> egui::Color32 {
    get(&WEAK, ThemeDef::P_CLASSICAL.weak)
}

/// Apply the theme (id may be `auto`).
pub fn apply(ctx: &egui::Context, id: &str, sys: Option<egui::Theme>) {
    let pal = resolve(id, sys);
    let mut v = if pal.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.panel_fill = pal.bg;
    v.window_fill = pal.bg;
    v.extreme_bg_color = pal.panel;
    v.faint_bg_color = pal.faint;
    v.hyperlink_color = pal.info;
    v.selection.bg_fill = pal.accent.linear_multiply(0.30);
    v.selection.stroke = egui::Stroke::new(1.0_f32, pal.accent);

    let w = &mut v.widgets;
    w.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, pal.text);
    w.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, pal.muted);
    w.noninteractive.weak_bg_fill = pal.faint;
    w.inactive.fg_stroke = egui::Stroke::new(1.0_f32, pal.text);
    w.inactive.bg_stroke = egui::Stroke::new(1.0_f32, pal.muted);
    w.hovered.weak_bg_fill = pal.faint;
    w.hovered.fg_stroke = egui::Stroke::new(1.0_f32, pal.text);
    w.hovered.bg_stroke = egui::Stroke::new(1.0_f32, pal.info);
    w.active.weak_bg_fill = pal.faint;
    w.active.fg_stroke = egui::Stroke::new(1.0_f32, pal.text);
    w.active.bg_stroke = egui::Stroke::new(1.0_f32, pal.accent);
    w.open.fg_stroke = egui::Stroke::new(1.0_f32, pal.text);

    ctx.set_visuals(v);

    put(&GOLD, pal.accent);
    put(&OK, pal.ok);
    put(&ERR, pal.err);
    put(&INFO, pal.info);
    put(&BORDER, pal.muted);
    put(&WEAK, pal.weak);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_unique_and_labels_set() {
        let mut seen = std::collections::HashSet::new();
        for t in THEMES {
            assert!(seen.insert(t.id), "duplicate id {}", t.id);
            assert!(!t.label.is_empty());
            assert!(!t.id.is_empty());
        }
        assert_eq!(THEMES.len(), 17);
    }

    #[test]
    fn resolve_auto_follows_system() {
        assert!(resolve("auto", Some(egui::Theme::Light)).dark == false);
        assert!(resolve("auto", Some(egui::Theme::Dark)).dark);
        assert!(resolve("auto", None).dark);
        // unknown id falls back to classical
        assert_eq!(resolve("nope", None).accent, resolve("classical", None).accent);
    }
}
