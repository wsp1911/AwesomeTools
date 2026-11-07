//! The window: header, history list and footer.
//!
//! All state lives in ClipboardService; this module only renders it and
//! forwards clicks and key presses.

use gpui::prelude::*;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, FocusHandle, FontWeight, IntoElement, KeyDownEvent,
    Render, SharedString, Window, div, px, rgb,
};

use crate::format::human_time;
use crate::model::now_ms;
use crate::platform::HOTKEY_LABEL;
use crate::service::{ClipboardService, RowModel, Snapshot};

const BG: u32 = 0x14161a;
const PANEL: u32 = 0x1b1e24;
const PANEL_HOVER: u32 = 0x232833;
const BORDER: u32 = 0x2b303a;
const TEXT: u32 = 0xe6e8eb;
const MUTED: u32 = 0x8b93a1;
const ACCENT: u32 = 0x58a6ff;
const PIN: u32 = 0xf0b429;
const WARN: u32 = 0xff8a7a;

#[cfg(target_os = "windows")]
const UI_FONT: &str = "Microsoft YaHei UI";
#[cfg(not(target_os = "windows"))]
const UI_FONT: &str = "sans-serif";

pub struct ClipboardView {
    service: Entity<ClipboardService>,
    focus: FocusHandle,
    selected: Option<u64>,
    titled: bool,
}

impl ClipboardView {
    pub fn new(service: Entity<ClipboardService>, cx: &mut Context<Self>) -> Self {
        cx.observe(&service, |_, _, cx| cx.notify()).detach();
        Self {
            service,
            focus: cx.focus_handle(),
            selected: None,
            titled: false,
        }
    }

    fn copy(&mut self, id: u64, cx: &mut Context<Self>) {
        self.selected = Some(id);
        self.service.update(cx, |service, cx| service.copy(id, cx));
    }

    fn toggle_pin(&mut self, id: u64, cx: &mut Context<Self>) {
        self.selected = Some(id);
        self.service
            .update(cx, |service, cx| service.toggle_pin(id, cx));
    }

    fn remove_entry(&mut self, id: u64, cx: &mut Context<Self>) {
        self.service
            .update(cx, |service, cx| service.remove(id, cx));
        if self.selected == Some(id) {
            self.selected = None;
        }
    }

    fn clear_unpinned(&mut self, cx: &mut Context<Self>) {
        self.service
            .update(cx, |service, cx| service.clear_unpinned(cx));
    }

    fn select_relative(&mut self, delta: isize, rows: &[RowModel], cx: &mut Context<Self>) {
        if rows.is_empty() {
            return;
        }
        let current = self
            .selected
            .and_then(|id| rows.iter().position(|row| row.id == id));
        let next = match current {
            Some(index) => (index as isize + delta).clamp(0, rows.len() as isize - 1),
            None if delta < 0 => rows.len() as isize - 1,
            None => 0,
        } as usize;
        self.selected = Some(rows[next].id);
        cx.notify();
    }

    fn handle_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.to_ascii_lowercase();
        match key.as_str() {
            "down" => {
                let rows = self.service.read(cx).snapshot().rows;
                self.select_relative(1, &rows, cx);
            }
            "up" => {
                let rows = self.service.read(cx).snapshot().rows;
                self.select_relative(-1, &rows, cx);
            }
            "enter" => {
                if let Some(id) = self.selected {
                    self.copy(id, cx);
                }
            }
            "delete" => {
                if let Some(id) = self.selected {
                    self.remove_entry(id, cx);
                }
            }
            "escape" => window.minimize_window(),
            "p" if event.keystroke.modifiers.control => {
                if let Some(id) = self.selected {
                    self.toggle_pin(id, cx);
                }
            }
            _ => {}
        }
    }

    fn render_header(&self, snapshot: &Snapshot) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .px(px(16.))
            .pt(px(14.))
            .pb(px(12.))
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("剪贴板历史"),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(MUTED))
                            .child(format!(
                                "置顶 {} · 记录 {}",
                                snapshot.pinned, snapshot.total
                            )),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(MUTED))
                    .child(format!(
                        "{HOTKEY_LABEL} 唤出 · 点击复制 · 方向键选择 · Ctrl+P 置顶 · Del 删除 · Esc 收起"
                    )),
            )
            .children(snapshot.warnings.iter().map(|warning| {
                div()
                    .text_size(px(11.))
                    .text_color(rgb(WARN))
                    .child(warning.clone())
            }))
    }

    fn render_row(&self, row: &RowModel, now: u64, cx: &mut Context<Self>) -> impl IntoElement {
        let id = row.id;
        let selected = self.selected == Some(id);
        let pinned = row.pinned;

        let copy = cx.listener(
            move |this: &mut Self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>| {
                this.copy(id, cx)
            },
        );
        let pin = cx.listener(
            move |this: &mut Self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>| {
                this.toggle_pin(id, cx)
            },
        );
        let remove = cx.listener(
            move |this: &mut Self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>| {
                this.remove_entry(id, cx)
            },
        );

        div()
            .id(SharedString::from(format!("row-{id}")))
            .flex()
            .flex_row()
            .items_start()
            .gap_2()
            .px(px(10.))
            .py(px(8.))
            .mb(px(6.))
            .rounded(px(6.))
            .border_1()
            .border_color(if selected { rgb(ACCENT) } else { rgb(BORDER) })
            .bg(if selected {
                rgb(PANEL_HOVER)
            } else {
                rgb(PANEL)
            })
            .hover(|style| style.bg(rgb(PANEL_HOVER)))
            .child(
                div()
                    .id(SharedString::from(format!("copy-{id}")))
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .on_click(copy)
                    .child(
                        div()
                            .text_size(px(12.))
                            .line_clamp(2)
                            .child(row.preview.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .text_size(px(10.))
                            .text_color(rgb(MUTED))
                            .child(human_time(row.created_at, now))
                            .child(format!("{} 字符", row.chars)),
                    ),
            )
            .child(
                div()
                    .id(SharedString::from(format!("pin-{id}")))
                    .px(px(6.))
                    .py(px(3.))
                    .rounded(px(4.))
                    .text_size(px(10.))
                    .text_color(if pinned { rgb(PIN) } else { rgb(MUTED) })
                    .hover(|style| style.bg(rgb(BORDER)))
                    .on_click(pin)
                    .child(if pinned { "取消置顶" } else { "置顶" }),
            )
            .child(
                div()
                    .id(SharedString::from(format!("remove-{id}")))
                    .px(px(6.))
                    .py(px(3.))
                    .rounded(px(4.))
                    .text_size(px(10.))
                    .text_color(rgb(MUTED))
                    .hover(|style| style.bg(rgb(BORDER)).text_color(rgb(WARN)))
                    .on_click(remove)
                    .child("删除"),
            )
    }

    fn render_list(&self, snapshot: &Snapshot, cx: &mut Context<Self>) -> impl IntoElement {
        let now = now_ms();

        if snapshot.rows.is_empty() {
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(MUTED))
                        .child("还没有记录，复制点什么试试"),
                )
                .into_any_element();
        }

        let mut rows: Vec<AnyElement> = Vec::with_capacity(snapshot.rows.len());
        for row in &snapshot.rows {
            rows.push(self.render_row(row, now, cx).into_any_element());
        }

        div()
            .id("history-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .px(px(10.))
                    .py(px(10.))
                    .children(rows),
            )
            .into_any_element()
    }

    fn render_footer(&self, snapshot: &Snapshot, cx: &mut Context<Self>) -> impl IntoElement {
        let clear = cx.listener(
            |this: &mut Self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>| {
                this.clear_unpinned(cx)
            },
        );

        let actions = if snapshot.unpinned > 0 {
            div()
                .id("clear-unpinned")
                .px(px(8.))
                .py(px(4.))
                .rounded(px(4.))
                .text_size(px(10.))
                .text_color(rgb(MUTED))
                .hover(|style| style.bg(rgb(BORDER)).text_color(rgb(TEXT)))
                .on_click(clear)
                .child("清空未置顶")
                .into_any_element()
        } else {
            div()
                .text_size(px(10.))
                .text_color(rgb(BORDER))
                .child("清空未置顶")
                .into_any_element()
        };

        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_2()
            .px(px(14.))
            .py(px(10.))
            .border_t_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(10.))
                    .text_color(rgb(MUTED))
                    .child(
                        snapshot
                            .status
                            .clone()
                            .unwrap_or_else(|| "记录保存在本机，置顶的条目不会被淘汰".to_string()),
                    ),
            )
            .child(actions)
    }
}

impl Render for ClipboardView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.titled {
            self.titled = true;
            window.set_window_title("剪贴板历史");
        }
        // Nothing else in this window takes focus, so the list keeps it to make
        // the keyboard shortcuts work from the first frame on.
        if !self.focus.is_focused(window) {
            window.focus(&self.focus);
        }

        let snapshot = self.service.read(cx).snapshot();
        if !self
            .selected
            .is_some_and(|id| snapshot.rows.iter().any(|row| row.id == id))
        {
            self.selected = snapshot.rows.first().map(|row| row.id);
        }

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .font_family(UI_FONT)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::handle_key))
            .child(self.render_header(&snapshot))
            .child(self.render_list(&snapshot, cx))
            .child(self.render_footer(&snapshot, cx))
    }
}
