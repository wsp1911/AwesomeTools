//! Clipboard history manager: watches the Windows clipboard, keeps a
//! deduplicated list with pinning, and lives in the tray behind a global
//! hotkey.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod format;
mod icon;
mod model;
mod platform;
mod service;
mod view;

use gpui::{App, AppContext, Application};

use service::{ClipboardService, ServiceHandle};

fn main() {
    Application::new().run(|cx: &mut App| {
        // The tray icon, the global hotkey and the clipboard watcher are all
        // created here: this is the thread that pumps the window messages, and
        // they have to outlive the window.
        let service = cx.new(ClipboardService::new);
        cx.set_global(ServiceHandle(service));
        service::install(cx);
        cx.activate(true);
    });
}
