use std::path::PathBuf;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let initial = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("WINSCORE_FILE").map(PathBuf::from));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 900.0])
            .with_min_inner_size([960.0, 680.0])
            .with_title("WinScore 5.01 — Rust Edition"),
        ..Default::default()
    };
    eframe::run_native(
        "WinScore 5.01 — Rust Edition",
        options,
        Box::new(move |cc| {
            Ok(Box::new(winscore_rs::app::WinScoreApp::new(
                cc,
                initial.clone(),
            )))
        }),
    )
}
