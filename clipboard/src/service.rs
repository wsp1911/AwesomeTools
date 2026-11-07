//! The clipboard watcher, the history store and the tray/hotkey wiring.
//!
//! All of this lives in an entity of its own instead of in the window, so
//! closing the window only closes the window: the watcher keeps recording, and
//! the hotkey or the tray icon can bring the window back. GPUI ends the message
//! loop once the last window is gone, which is why an invisible keep-alive
//! window is kept around for as long as the app runs.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui::{
    AnyWindowHandle, App, AppContext, Bounds, ClipboardItem, Context, Entity, Global, IntoElement,
    Render, SharedString, Window, WindowBounds, WindowKind, WindowOptions, div, point, px, size,
};

use crate::format::summarise;
use crate::model::{self, Recorded, Store};
use crate::platform::{self, HotkeyRegistration, TrayCommand, TrayHandle};
use crate::view::ClipboardView;

const POLL_INTERVAL: Duration = Duration::from_millis(300);
const SAVE_DELAY: Duration = Duration::from_millis(400);
const TOAST_LIFETIME: Duration = Duration::from_secs(3);
/// Text this app just wrote to the clipboard must not come back as a record.
const SUPPRESS_WINDOW: Duration = Duration::from_secs(2);
const WINDOW_WIDTH: f32 = 400.;
const WINDOW_HEIGHT: f32 = 560.;
const MIN_WINDOW_WIDTH: f32 = 320.;
const MIN_WINDOW_HEIGHT: f32 = 360.;

/// Keeps the service alive for as long as the application runs, even when no
/// window is open.
pub struct ServiceHandle(pub Entity<ClipboardService>);

impl Global for ServiceHandle {}

pub struct RowModel {
    pub id: u64,
    pub preview: SharedString,
    pub created_at: u64,
    pub chars: usize,
    pub pinned: bool,
}

/// Everything the window needs for one frame, copied out of the store.
pub struct Snapshot {
    pub rows: Vec<RowModel>,
    pub pinned: usize,
    pub total: usize,
    pub unpinned: usize,
    pub status: Option<String>,
    pub warnings: Vec<String>,
}

pub struct ClipboardService {
    store: Store,
    storage: PathBuf,
    /// Last clipboard sequence number already processed.
    last_sequence: u32,
    suppress: Option<(String, Instant)>,
    dirty: bool,
    last_save: Instant,
    toast: Option<(String, Instant)>,
    warnings: Vec<String>,
    hotkey: Option<HotkeyRegistration>,
    tray: Option<TrayHandle>,
    /// Client-area bounds of the last visible window, so it reopens where it
    /// was closed.
    window_bounds: Option<Bounds<gpui::Pixels>>,
    /// Offset between the bounds a new window is opened with and the client
    /// bounds it reports afterwards. GPUI places windows by their frame, so this
    /// has to be subtracted again when the remembered client bounds are reused,
    /// otherwise every close/reopen pair walks the window across the screen.
    frame_offset: Option<gpui::Point<gpui::Pixels>>,
    /// Set when the window should come forward. Opening or activating a window
    /// renders it synchronously, which must not happen while this entity is
    /// being updated, so `tick` only records the intent and the polling task
    /// performs it in its own app update.
    show_requested: bool,
}

impl ClipboardService {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let storage = model::storage_path();
        let store = model::load(&storage);

        let mut warnings = Vec::new();
        let hotkey = match platform::install_hotkey() {
            Ok(registration) => Some(registration),
            Err(error) => {
                warnings.push(error);
                None
            }
        };
        let tray = match platform::install_tray() {
            Ok(handle) => Some(handle),
            Err(error) => {
                warnings.push(error);
                None
            }
        };

        let service = Self {
            store,
            storage,
            last_sequence: platform::clipboard_sequence(),
            suppress: None,
            dirty: false,
            last_save: Instant::now(),
            toast: None,
            warnings,
            hotkey,
            tray,
            window_bounds: None,
            frame_offset: None,
            show_requested: false,
        };

        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL_INTERVAL).await;
                match this.update(cx, |service, cx| service.tick(cx)) {
                    Ok(true) => {
                        // A separate app update: the window draws itself as soon
                        // as it is created, and that frame must not read this
                        // entity while the update above still holds it.
                        let _ = cx.update(show_window);
                    }
                    Ok(false) => {}
                    Err(_) => break,
                }
            }
        })
        .detach();

        service
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            rows: self
                .store
                .entries
                .iter()
                .map(|entry| RowModel {
                    id: entry.id,
                    preview: SharedString::from(summarise(&entry.text, 320)),
                    created_at: entry.created_at,
                    chars: entry.text.chars().count(),
                    pinned: entry.pinned,
                })
                .collect(),
            pinned: self.store.pinned_count(),
            total: self.store.entries.len(),
            unpinned: self.store.unpinned_count(),
            status: self.toast.as_ref().and_then(|(text, shown_at)| {
                (shown_at.elapsed() < TOAST_LIFETIME).then(|| text.clone())
            }),
            warnings: self.warnings.clone(),
        }
    }

    /// Copies an entry back to the system clipboard without recording it again.
    pub fn copy(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(text) = self.store.get(id).map(|entry| entry.text.clone()) else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        self.last_sequence = platform::clipboard_sequence();
        self.suppress = Some((text.clone(), Instant::now()));
        self.set_toast(format!("已复制 · {}", summarise(&text, 40)));
        cx.notify();
    }

    pub fn toggle_pin(&mut self, id: u64, cx: &mut Context<Self>) {
        self.store.toggle_pin(id, model::now_ms());
        self.dirty = true;
        cx.notify();
    }

    pub fn remove(&mut self, id: u64, cx: &mut Context<Self>) {
        self.store.remove(id);
        self.dirty = true;
        cx.notify();
    }

    pub fn clear_unpinned(&mut self, cx: &mut Context<Self>) {
        let before = self.store.entries.len();
        self.store.clear_unpinned();
        let removed = before - self.store.entries.len();
        self.dirty = true;
        if removed > 0 {
            self.set_toast(format!("已清空 {removed} 条未置顶记录"));
        }
        self.save_now();
        cx.notify();
    }

    pub fn quit(&mut self, cx: &mut Context<Self>) {
        self.save_now();
        cx.quit();
    }

    fn set_toast(&mut self, message: String) {
        self.toast = Some((message, Instant::now()));
    }

    /// Returns whether the window should be brought forward; the caller runs
    /// that outside this entity's update.
    fn tick(&mut self, cx: &mut Context<Self>) -> bool {
        let mut changed = false;
        let mut show = std::mem::take(&mut self.show_requested);

        // Remember where the visible window is, so closing and reopening it
        // through the tray does not move it back to the middle of the screen.
        if let Some(handle) = real_window(cx)
            && let Ok(bounds) = handle.update(cx, |_, window, _| window.bounds())
        {
            self.window_bounds = Some(bounds);
        }

        let sequence = platform::clipboard_sequence();
        if sequence != self.last_sequence {
            self.last_sequence = sequence;
            let text = cx
                .read_from_clipboard()
                .and_then(|item| item.text())
                .map(|text| text.to_string());
            if let Some(text) = text {
                if self.is_self_copy(&text) {
                    self.suppress = None;
                } else {
                    match self.store.record(&text, model::now_ms()) {
                        Recorded::Added(_) | Recorded::Refreshed(_) => {
                            self.dirty = true;
                            changed = true;
                        }
                        Recorded::Ignored(_) => {}
                    }
                }
            }
        }

        if self.hotkey.as_ref().is_some_and(platform::hotkey_pressed) {
            show = true;
        }

        match self.tray.as_ref().and_then(TrayHandle::poll) {
            Some(TrayCommand::Show) => show = true,
            Some(TrayCommand::ClearUnpinned) => self.clear_unpinned(cx),
            Some(TrayCommand::Quit) => self.quit(cx),
            None => {}
        }

        if self.dirty && self.last_save.elapsed() >= SAVE_DELAY {
            self.save_now();
        }

        if let Some((_, shown_at)) = &self.toast
            && shown_at.elapsed() >= TOAST_LIFETIME
        {
            self.toast = None;
            changed = true;
        }

        if changed {
            cx.notify();
        }

        show
    }

    fn is_self_copy(&self, text: &str) -> bool {
        match &self.suppress {
            Some((expected, when)) => expected == text && when.elapsed() < SUPPRESS_WINDOW,
            None => false,
        }
    }

    fn save_now(&mut self) {
        match model::save(&self.store, &self.storage) {
            Ok(()) => {
                self.dirty = false;
                self.last_save = Instant::now();
            }
            Err(error) => {
                let message = format!("历史保存失败: {error}");
                if !self.warnings.contains(&message) {
                    self.warnings.push(message);
                }
            }
        }
    }
}

/// The visible window, if it is currently open.
fn real_window(cx: &App) -> Option<AnyWindowHandle> {
    cx.windows()
        .into_iter()
        .find(|handle| handle.downcast::<ClipboardView>().is_some())
}

/// Starts the app: the invisible keep-alive window, then the real one.
pub fn install(cx: &mut App) {
    ensure_keep_alive(cx);
    open_window(cx);
}

/// GPUI ends the message loop as soon as the last window is gone, so an
/// invisible window stays behind to keep the tray icon, the global hotkey and
/// the clipboard watcher alive while the visible window is closed.
fn ensure_keep_alive(cx: &mut App) {
    if cx
        .windows()
        .iter()
        .any(|handle| handle.downcast::<KeepAlive>().is_some())
    {
        return;
    }

    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(-32_000.), px(-32_000.)),
            size: size(px(1.), px(1.)),
        })),
        show: false,
        focus: false,
        kind: WindowKind::PopUp,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        ..Default::default()
    };
    let _ = cx.open_window(options, |_, cx| cx.new(|_| KeepAlive));
}

/// Brings the visible window forward: activates it when it exists, opens a new
/// one otherwise.
///
/// This must run in its own app update, never from inside an update of
/// `ClipboardService`: creating a window draws its first frame immediately, and
/// that frame reads the service.
fn show_window(cx: &mut App) {
    if let Some(handle) = real_window(cx) {
        let _ = handle.update(cx, |_, window, cx| {
            window.activate_window();
            cx.activate(true);
        });
        return;
    }
    open_window(cx);
}

/// Opens the window at the position it was last closed at.
pub fn open_window(cx: &mut App) {
    if real_window(cx).is_some() {
        return;
    }
    let Some(service) = cx
        .try_global::<ServiceHandle>()
        .map(|handle| handle.0.clone())
    else {
        return;
    };

    ensure_keep_alive(cx);

    let remembered = service.read(cx).window_bounds;
    let frame_offset = service.read(cx).frame_offset;
    // The remembered bounds are client bounds; subtract the frame offset that was
    // measured on the last window so the new one lands in the same place.
    let restored = remembered.map(|client| match frame_offset {
        Some(offset) => Bounds {
            origin: point(client.origin.x - offset.x, client.origin.y - offset.y),
            size: client.size,
        },
        None => client,
    });

    let bounds = restored
        .unwrap_or_else(|| Bounds::centered(None, size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)), cx));
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(size(px(MIN_WINDOW_WIDTH), px(MIN_WINDOW_HEIGHT))),
        ..Default::default()
    };

    let root_service = service.clone();
    let result = cx.open_window(options, move |_, cx| {
        cx.new(|cx| ClipboardView::new(root_service.clone(), cx))
    });

    // Remember how far the created window ended up from where it was asked to
    // be, so the next reopen can compensate for it.
    let measured = result.as_ref().ok().and_then(|handle| {
        handle
            .update(cx, |_, window, _| {
                let actual = window.bounds();
                point(
                    actual.origin.x - bounds.origin.x,
                    actual.origin.y - bounds.origin.y,
                )
            })
            .ok()
    });
    if let Some(offset) = measured {
        service.update(cx, |service, _| service.frame_offset = Some(offset));
    }

    cx.activate(true);
}

/// Invisible window whose only job is to keep the application (and with it the
/// tray icon) running while the visible window is closed.
struct KeepAlive;

impl Render for KeepAlive {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
