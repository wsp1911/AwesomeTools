#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

mod app;
mod format;
mod fsops;
mod layout;
mod theme;

/// `build.rs` decodes `assets/icon.png` into raw pixels, so the title bar,
/// taskbar and Alt+Tab show the DiskSniffer icon instead of eframe's default `e`.
const ICON_RGBA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/icon.rgba"));
include!(concat!(env!("OUT_DIR"), "/icon_size.rs"));

fn window_icon() -> eframe::egui::IconData {
    debug_assert_eq!(
        ICON_RGBA.len(),
        (ICON_WIDTH * ICON_HEIGHT * 4) as usize,
        "the generated icon does not match its reported size"
    );
    eframe::egui::IconData {
        rgba: ICON_RGBA.to_vec(),
        width: ICON_WIDTH,
        height: ICON_HEIGHT,
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([720.0, 480.0])
            .with_title("DiskSniffer")
            .with_icon(Arc::new(window_icon())),
        ..Default::default()
    };
    eframe::run_native(
        "DiskSniffer",
        options,
        Box::new(|cc| Ok(Box::new(app::DiskSnifferApp::new(cc)))),
    )
}
