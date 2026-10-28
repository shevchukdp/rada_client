//! `rada-client-gui` — desktop GUI (egui) for the local Ukrainian-legislation
//! index. Same engine as the CLI and the web UI: `rada-db` index + `rada-core`
//! live API client with enforced pacing. Local search and annotations are fully
//! offline; document text and live updates come from `data.rada.gov.ua`.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod settings;
mod themes;
mod views;

use eframe::egui::IconData;
use std::path::PathBuf;

fn main() {
    let mut db_path: Option<PathBuf> = None;
    let mut pace: Option<u64> = None;
    let mut start: Option<String> = None;
    let mut query: Option<String> = None;
    let mut nreg: Option<String> = None;
    let mut cache_query: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--db" => db_path = args.next().map(PathBuf::from),
            "--pace" => {
                if let Some(v) = args.next().and_then(|s| s.parse::<u64>().ok()) {
                    pace = Some(v);
                }
            }
            "--start" => start = args.next(),
            "--query" => query = args.next(),
            "--nreg" => nreg = args.next(),
            "--cache-search" => cache_query = args.next(),
            _ => {}
        }
    }

    // Portable default: the index lives next to the exe.
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    let db_path = db_path.unwrap_or_else(|| {
        if cfg!(debug_assertions) {
            // run from the workspace root during development
            let p = PathBuf::from("C:/rada/data/test-index.sqlite");
            if p.exists() {
                p
            } else {
                PathBuf::from("data/zakon-index.sqlite")
            }
        } else {
            exe_dir.join("data").join("zakon-index.sqlite")
        }
    });

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Законодавство України — rada-client")
            .with_inner_size([1180.0, 780.0])
            .with_min_inner_size([860.0, 560.0])
            .with_icon(load_icon()),
        ..Default::default()
    };

    let startup = app::Startup {
        start_page: start,
        query,
        nreg,
        cache_query,
    };

    eframe::run_native(
        "rada-client-gui",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, db_path, pace, startup)))),
    )
    .unwrap_or_else(|e| {
        eprintln!("eframe: {e:?}");
        std::process::exit(1);
    });
}

/// Window icon from the repo asset (the 256px PNG inside the .ico family).
fn load_icon() -> eframe::egui::IconData {
    // exe-relative first (portable), then repo-relative (dev build)
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_default();
    let candidates: [Option<PathBuf>; 3] = [
        exe_dir.parent().map(|p| p.join("assets/icon/rada-client-256.png")),
        Some(exe_dir.join("assets/icon/rada-client-256.png")),
        Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/icon/rada-client-256.png"),
        ),
    ];
    let path = candidates
        .into_iter()
        .flatten()
        .find(|p| p.exists())
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/icon/rada-client-256.png")
        });
    match std::fs::read(&path) {
        Ok(bytes) => match image::load_from_memory_with_format(&bytes, image::ImageFormat::Png) {
            Ok(png) => {
                let rgba = png.into_rgba8();
                IconData {
                    width: rgba.width(),
                    height: rgba.height(),
                    rgba: rgba.into_raw(),
                }
            }
            Err(_) => IconData::default(),
        },
        Err(_) => IconData::default(),
    }
}
