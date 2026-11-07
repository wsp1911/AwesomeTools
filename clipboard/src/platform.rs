//! Windows-facing glue: clipboard sequence polling, the global hotkey and the
//! tray icon.
//!
//! Everything here has to run on the thread that owns the window message loop,
//! which is the GPUI main thread, so the installs happen while the first window
//! is being created.

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

/// Increments every time the clipboard content is replaced, by us or by anyone
/// else. Cheap enough to poll a few times per second.
#[cfg(target_os = "windows")]
pub fn clipboard_sequence() -> u32 {
    unsafe { windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber() }
}

#[cfg(not(target_os = "windows"))]
pub fn clipboard_sequence() -> u32 {
    0
}

pub struct HotkeyRegistration {
    // Dropping the manager unregisters the hotkey, so it is kept alive here.
    _manager: GlobalHotKeyManager,
    hotkey: HotKey,
}

pub const HOTKEY_LABEL: &str = "Ctrl+Shift+V";

pub fn install_hotkey() -> Result<HotkeyRegistration, String> {
    let manager = GlobalHotKeyManager::new().map_err(|error| format!("全局热键不可用: {error}"))?;
    let hotkey = HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyV);
    manager
        .register(hotkey)
        .map_err(|error| format!("{HOTKEY_LABEL} 注册失败: {error}"))?;
    Ok(HotkeyRegistration {
        _manager: manager,
        hotkey,
    })
}

/// Returns true when the registered hotkey was pressed since the last call.
pub fn hotkey_pressed(registration: &HotkeyRegistration) -> bool {
    let id = registration.hotkey.id();
    let mut pressed = false;
    while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
        if event.state == HotKeyState::Pressed && (id == 0 || event.id == id) {
            pressed = true;
        }
    }
    pressed
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayCommand {
    Show,
    ClearUnpinned,
    Quit,
}

pub struct TrayHandle {
    _tray: TrayIcon,
    show: MenuId,
    clear: MenuId,
    quit: MenuId,
}

pub fn install_tray() -> Result<TrayHandle, String> {
    let menu = Menu::new();
    let show = MenuItem::new("显示窗口", true, None);
    let clear = MenuItem::new("清空未置顶", true, None);
    let quit = MenuItem::new("退出", true, None);
    menu.append_items(&[&show, &PredefinedMenuItem::separator(), &clear, &quit])
        .map_err(|error| format!("托盘菜单不可用: {error}"))?;

    let tray = TrayIconBuilder::new()
        .with_tooltip("剪贴板历史 · Ctrl+Shift+V")
        .with_icon(tray_image())
        .with_menu(Box::new(menu))
        .build()
        .map_err(|error| format!("托盘图标不可用: {error}"))?;

    Ok(TrayHandle {
        _tray: tray,
        show: show.id().clone(),
        clear: clear.id().clone(),
        quit: quit.id().clone(),
    })
}

impl TrayHandle {
    pub fn poll(&self) -> Option<TrayCommand> {
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == self.show {
                return Some(TrayCommand::Show);
            }
            if event.id == self.clear {
                return Some(TrayCommand::ClearUnpinned);
            }
            if event.id == self.quit {
                return Some(TrayCommand::Quit);
            }
        }

        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                return Some(TrayCommand::Show);
            }
        }

        None
    }
}

/// The tray icon is drawn at runtime from the same geometry `build.rs` uses for
/// the executable icon, so the tray and the taskbar never drift apart.
fn tray_image() -> Icon {
    const SIZE: u32 = 32;
    Icon::from_rgba(crate::icon::rgba(SIZE), SIZE, SIZE).unwrap_or_else(|_| {
        Icon::from_rgba(vec![0x58, 0xa6, 0xff, 0xff], 1, 1).expect("fallback tray icon")
    })
}
