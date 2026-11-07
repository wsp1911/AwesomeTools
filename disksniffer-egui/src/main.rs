#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod format;
mod fsops;
mod layout;
mod theme;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([720.0, 480.0])
            .with_title("DiskSniffer"),
        ..Default::default()
    };
    eframe::run_native(
        "DiskSniffer",
        options,
        Box::new(|cc| Ok(Box::new(app::DiskSnifferApp::new(cc)))),
    )
}
