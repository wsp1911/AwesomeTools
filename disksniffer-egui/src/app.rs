use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use disksniffer_core::scan::{self, ScanProgress};
use disksniffer_core::tree::{Candidate, ChildInfo, ScanTree, Summary};
use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Rect, Sense, Stroke, StrokeKind};

use crate::format::{format_bytes, format_count, format_duration, format_percent};
use crate::fsops;
use crate::layout::{layout_treemap, pick_cell_at, LaidOutRect, MAX_ITEMS};
use crate::theme;

const MUTED_WARM: Color32 = Color32::from_rgb(0xf0, 0x8a, 0x7c);
const LEAF_TEXT: Color32 = Color32::from_rgb(0xf3, 0xf6, 0xfb);
const LEAF_SIZE: Color32 = Color32::from_rgb(0xc9, 0xd3, 0xe2);
const DANGER: Color32 = Color32::from_rgb(0xe8, 0x6a, 0x5e);

#[derive(Clone)]
struct ContextTarget {
    id: u32,
    name: String,
    path: String,
    kind: String,
    is_dir: bool,
    size: u64,
    child_count: u32,
}

fn target_of(rect: &LaidOutRect) -> ContextTarget {
    ContextTarget {
        id: rect.id,
        name: rect.name.clone(),
        path: rect.path.clone(),
        kind: rect.kind.clone(),
        is_dir: rect.is_dir,
        size: rect.size,
        child_count: rect.child_count,
    }
}

#[derive(Clone)]
struct MenuState {
    target: ContextTarget,
    pos: egui::Pos2,
}

#[derive(Clone)]
struct Toast {
    text: String,
    error: bool,
    at: Instant,
}

enum MenuAction {
    Drill,
    Open,
    Reveal,
    Copy,
    Trash,
}

struct ScanPayload {
    tree: Arc<ScanTree>,
    summary: Summary,
    top_files: Vec<Candidate>,
}

enum Scan {
    Idle,
    Running {
        progress: Arc<ScanProgress>,
        cancel: Arc<AtomicBool>,
        rx: Receiver<Result<ScanPayload, String>>,
    },
}

pub struct DiskSnifferApp {
    scan: Scan,
    tree: Option<Arc<ScanTree>>,
    summary: Option<Summary>,
    top_files: Vec<Candidate>,
    trail: Vec<(u32, String)>,
    layout: Vec<LaidOutRect>,
    layout_key: Option<(u32, i32, i32)>,
    hovered: Option<usize>,
    selected: Option<ContextTarget>,
    status: Option<String>,
    path_input: String,
    menu: Option<MenuState>,
    pending_delete: Option<ContextTarget>,
    trash: Option<Receiver<(Result<(), String>, String)>>,
    toast: Option<Toast>,
}

impl DiskSnifferApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        cc.egui_ctx.global_style_mut(|style| {
            style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        });
        DiskSnifferApp {
            scan: Scan::Idle,
            tree: None,
            summary: None,
            top_files: Vec::new(),
            trail: Vec::new(),
            layout: Vec::new(),
            layout_key: None,
            hovered: None,
            selected: None,
            status: None,
            path_input: String::new(),
            menu: None,
            pending_delete: None,
            trash: None,
            toast: None,
        }
    }

    fn focus_id(&self) -> u32 {
        self.trail.last().map(|entry| entry.0).unwrap_or(0)
    }

    fn focus_path(&self) -> String {
        self.trail
            .last()
            .map(|entry| entry.1.clone())
            .unwrap_or_default()
    }

    fn total_size(&self) -> u64 {
        self.summary
            .as_ref()
            .map(|summary| summary.total_size)
            .unwrap_or(0)
    }

    fn invalidate_layout(&mut self) {
        self.layout_key = None;
        self.hovered = None;
    }

    fn begin_scan_from_input(&mut self) {
        let text = self.path_input.trim().to_string();
        if text.is_empty() {
            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                self.start_scan(dir);
            }
            return;
        }
        self.start_scan(PathBuf::from(text));
    }

    fn start_scan(&mut self, path: PathBuf) {
        if matches!(self.scan, Scan::Running { .. }) {
            return;
        }
        self.path_input = path.to_string_lossy().into_owned();
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(ScanProgress::new(cancel.clone()));
        let (tx, rx) = mpsc::channel();
        let thread_progress = progress.clone();
        let thread_cancel = cancel.clone();
        self.status = None;
        self.scan = Scan::Running {
            progress,
            cancel,
            rx,
        };

        std::thread::spawn(move || {
            let root = match scan::normalize_path(&path.to_string_lossy()) {
                Ok(root) => root,
                Err(error) => {
                    let _ = tx.send(Err(error));
                    return;
                }
            };
            if !root.is_dir() {
                let _ = tx.send(Err(format!("这不是一个文件夹: {}", root.display())));
                return;
            }
            let started = Instant::now();
            let name = root.to_string_lossy().into_owned().into_boxed_str();
            let raw = scan::scan_dir(&root, name, &thread_progress, 0);
            let elapsed_ms = started.elapsed().as_millis() as u64;
            if thread_cancel.load(Ordering::Relaxed) {
                let _ = tx.send(Err("扫描已取消".to_string()));
                return;
            }
            let errors = thread_progress.errors.load(Ordering::Relaxed);
            let tree = ScanTree::build(raw, root, errors, elapsed_ms);
            let summary = tree.summary();
            let top_files = tree.top_files(12);
            let _ = tx.send(Ok(ScanPayload {
                tree: Arc::new(tree),
                summary,
                top_files,
            }));
        });
    }

    fn apply_payload(&mut self, payload: ScanPayload) {
        self.trail = vec![(payload.summary.root_id, payload.summary.root.clone())];
        self.path_input = payload.summary.root.clone();
        self.tree = Some(payload.tree);
        self.summary = Some(payload.summary);
        self.top_files = payload.top_files;
        self.layout.clear();
        self.invalidate_layout();
        self.selected = None;
        self.menu = None;
        self.pending_delete = None;
        self.status = None;
        self.scan = Scan::Idle;
    }

    fn poll_scan(&mut self, ctx: &egui::Context) {
        let incoming = match &self.scan {
            Scan::Running { rx, .. } => match rx.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err("扫描线程异常结束".to_string())),
            },
            Scan::Idle => None,
        };

        match incoming {
            Some(Ok(payload)) => self.apply_payload(payload),
            Some(Err(error)) => {
                self.status = Some(error);
                self.scan = Scan::Idle;
            }
            None => {
                if matches!(self.scan, Scan::Running { .. }) {
                    ctx.request_repaint_after(Duration::from_millis(100));
                }
            }
        }
    }

    fn drill(&mut self, id: u32, path: String) {
        self.trail.push((id, path));
        self.layout.clear();
        self.invalidate_layout();
        self.selected = None;
    }

    fn go_up(&mut self) {
        if self.trail.len() > 1 {
            self.trail.pop();
            self.layout.clear();
            self.invalidate_layout();
            self.selected = None;
            self.menu = None;
        }
    }

    fn jump_to(&mut self, index: usize) {
        if index + 1 < self.trail.len() {
            self.trail.truncate(index + 1);
            self.layout.clear();
            self.invalidate_layout();
            self.selected = None;
            self.menu = None;
        }
    }

    fn ensure_layout(&mut self, size: egui::Vec2) {
        if size.x < 16.0 || size.y < 16.0 {
            return;
        }
        let tree = match &self.tree {
            Some(tree) => tree.clone(),
            None => return,
        };
        let focus = self.focus_id();
        let key = (
            focus,
            (size.x * 2.0).round() as i32,
            (size.y * 2.0).round() as i32,
        );
        if self.layout_key == Some(key) {
            return;
        }

        let path = self.focus_path();
        let page = match tree.children_page(focus, MAX_ITEMS) {
            Ok(page) => page,
            Err(_) => return,
        };
        let mut nested: HashMap<u32, Vec<ChildInfo>> = HashMap::new();
        for kid in &page.children {
            if !kid.is_dir {
                continue;
            }
            if let Ok(sub) = tree.children_page(kid.id, 64) {
                if !sub.children.is_empty() {
                    nested.insert(kid.id, sub.children);
                }
            }
        }
        self.layout = layout_treemap(&page.children, &nested, &path, focus, size.x, size.y);
        self.layout_key = Some(key);
        self.hovered = None;
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let frame = egui::Frame::NONE
            .fill(theme::PANEL)
            .inner_margin(egui::Margin::symmetric(10, 6));
        egui::Panel::top("toolbar").frame(frame).show(ui, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                let scanning = matches!(self.scan, Scan::Running { .. });
                let width = (ui.available_width() - 260.0).clamp(150.0, 460.0);
                let edit = ui.add_enabled(
                    !scanning,
                    egui::TextEdit::singleline(&mut self.path_input)
                        .hint_text("输入要扫描的磁盘或文件夹路径，例如 C:\\ 或 D:\\Projects")
                        .desired_width(width),
                );
                let submitted =
                    edit.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));

                if ui.button("浏览").clicked() {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        self.start_scan(dir);
                    }
                }

                if scanning {
                    if ui.button("停止扫描").clicked() {
                        if let Scan::Running { cancel, .. } = &self.scan {
                            cancel.store(true, Ordering::Relaxed);
                        }
                    }
                } else if ui.button("开始扫描").clicked() || submitted {
                    self.begin_scan_from_input();
                }

                if let Scan::Running { progress, .. } = &self.scan {
                    ui.separator();
                    ui.label(
                        egui::RichText::new(format!(
                            "扫描中 · {} 文件 · {} 文件夹 · {}",
                            format_count(progress.files.load(Ordering::Relaxed)),
                            format_count(progress.dirs.load(Ordering::Relaxed)),
                            format_bytes(progress.bytes.load(Ordering::Relaxed)),
                        ))
                        .color(theme::TEXT_MUTED),
                    );
                    let current = progress
                        .current
                        .lock()
                        .map(|value| value.clone())
                        .unwrap_or_default();
                    if !current.is_empty() {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(current)
                                    .small()
                                    .color(theme::TEXT_MUTED),
                            )
                            .truncate(),
                        );
                    }
                }
            });

            if self.trail.len() > 1 {
                ui.add_space(2.0);
                ui.horizontal_wrapped(|ui| {
                    let mut jump: Option<usize> = None;
                    let last = self.trail.len() - 1;
                    for (index, entry) in self.trail.iter().enumerate() {
                        let is_last = index == last;
                        let response = ui.selectable_label(is_last, entry.1.as_str());
                        if response.clicked() {
                            jump = Some(index);
                        }
                        if response.secondary_clicked() && !is_last {
                            jump = Some(index);
                        }
                        if !is_last {
                            ui.label(egui::RichText::new(">").color(theme::TEXT_MUTED));
                        }
                    }
                    if let Some(index) = jump {
                        self.jump_to(index);
                    }
                });
            }

            if let Some(status) = self.status.clone() {
                ui.label(egui::RichText::new(status).color(MUTED_WARM));
            }
            ui.add_space(2.0);
        });
    }

    fn side_panel(&mut self, ui: &mut egui::Ui) {
        let summary = match &self.summary {
            Some(summary) => summary.clone(),
            None => return,
        };
        let top_files = self.top_files.clone();
        let selected = self.selected.clone();

        let frame = egui::Frame::NONE
            .fill(theme::PANEL)
            .inner_margin(egui::Margin::symmetric(10, 8));
        egui::Panel::left("sidebar")
            .exact_size(276.0)
            .frame(frame)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("概览").small().color(theme::TEXT_MUTED));
                    ui.label(
                        egui::RichText::new(format_bytes(summary.total_size))
                            .size(28.0)
                            .strong(),
                    );
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&summary.root)
                                .small()
                                .color(theme::TEXT_MUTED),
                        )
                        .truncate(),
                    );
                    ui.add_space(6.0);
                    egui::Grid::new("stats")
                        .num_columns(2)
                        .spacing([12.0, 3.0])
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new("文件").color(theme::TEXT_MUTED));
                            ui.label(format_count(summary.total_files));
                            ui.end_row();
                            ui.label(egui::RichText::new("文件夹").color(theme::TEXT_MUTED));
                            ui.label(format_count(summary.total_dirs));
                            ui.end_row();
                            ui.label(egui::RichText::new("耗时").color(theme::TEXT_MUTED));
                            ui.label(format_duration(summary.elapsed_ms));
                            ui.end_row();
                            ui.label(egui::RichText::new("无法访问").color(theme::TEXT_MUTED));
                            ui.label(format_count(summary.errors));
                            ui.end_row();
                        });

                    ui.add_space(10.0);
                    ui.separator();
                    ui.label(
                        egui::RichText::new("类型分布")
                            .small()
                            .color(theme::TEXT_MUTED),
                    );
                    ui.add_space(4.0);
                    let mut kinds: Vec<(usize, u64)> = summary
                        .kinds
                        .iter()
                        .enumerate()
                        .map(|(index, kind)| (index, kind.size))
                        .filter(|(_, size)| *size > 0)
                        .collect();
                    kinds.sort_by_key(|(_, size)| std::cmp::Reverse(*size));
                    for (index, size) in kinds {
                        let kind = &summary.kinds[index];
                        let meta = theme::meta(&kind.key);
                        ui.horizontal(|ui| {
                            let (dot, _) =
                                ui.allocate_exact_size(egui::vec2(8.0, 8.0), Sense::hover());
                            ui.painter()
                                .rect_filled(dot, CornerRadius::same(2), meta.color);
                            ui.label(meta.label);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        egui::RichText::new(format_percent(
                                            size,
                                            summary.total_size,
                                        ))
                                        .color(theme::TEXT_MUTED),
                                    );
                                    ui.label(format_bytes(size));
                                },
                            );
                        });
                        let width = ui.available_width();
                        let (bar, _) =
                            ui.allocate_exact_size(egui::vec2(width, 3.0), Sense::hover());
                        ui.painter().rect_filled(
                            bar,
                            CornerRadius::same(1),
                            Color32::from_rgb(0x1c, 0x22, 0x2d),
                        );
                        let ratio = size as f32 / summary.total_size.max(1) as f32;
                        let filled = Rect::from_min_size(
                            bar.min,
                            egui::vec2((bar.width() * ratio).max(2.0), bar.height()),
                        );
                        ui.painter()
                            .rect_filled(filled, CornerRadius::same(1), meta.color);
                        ui.add_space(3.0);
                    }

                    if !top_files.is_empty() {
                        ui.add_space(10.0);
                        ui.separator();
                        ui.label(
                            egui::RichText::new("最大的文件")
                                .small()
                                .color(theme::TEXT_MUTED),
                        );
                        ui.add_space(4.0);
                        for file in &top_files {
                            ui.horizontal(|ui| {
                                let (bar, _) =
                                    ui.allocate_exact_size(egui::vec2(3.0, 30.0), Sense::hover());
                                ui.painter().rect_filled(
                                    bar,
                                    CornerRadius::same(2),
                                    theme::meta(&file.kind).color,
                                );
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(&file.name).size(11.5),
                                            )
                                            .truncate(),
                                        )
                                        .on_hover_text(&file.path);
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                ui.label(
                                                    egui::RichText::new(format_bytes(file.size))
                                                        .small()
                                                        .color(theme::TEXT_MUTED),
                                                );
                                            },
                                        );
                                    });
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&file.parent_path)
                                                .size(10.0)
                                                .color(theme::TEXT_MUTED),
                                        )
                                        .truncate(),
                                    );
                                });
                            });
                        }
                    }
                    if let Some(target) = &selected {
                        ui.add_space(10.0);
                        ui.separator();
                        ui.label(
                            egui::RichText::new("已选择")
                                .small()
                                .color(theme::TEXT_MUTED),
                        );
                        ui.add(
                            egui::Label::new(egui::RichText::new(&target.name).strong()).truncate(),
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&target.path)
                                    .size(10.5)
                                    .color(theme::TEXT_MUTED),
                            )
                            .truncate(),
                        )
                        .on_hover_text(&target.path);
                        let mut facts = format!(
                            "{} · {}",
                            format_bytes(target.size),
                            theme::meta(&target.kind).label
                        );
                        if target.is_dir {
                            facts.push_str(&format!(
                                " · {} 项",
                                format_count(target.child_count as u64)
                            ));
                        }
                        ui.label(egui::RichText::new(facts).small());
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            if target.is_dir && ui.button("进入目录").clicked() {
                                self.drill(target.id, target.path.clone());
                            }
                            let reveal_label = if target.is_dir {
                                "在文件管理器中打开"
                            } else {
                                "在文件管理器中定位"
                            };
                            if ui.button(reveal_label).clicked() {
                                let result = if target.is_dir {
                                    fsops::open_dir(&target.path)
                                } else {
                                    fsops::reveal(&target.path)
                                };
                                if let Err(error) = result {
                                    self.notify(error, true);
                                }
                            }
                            if ui.button("复制路径").clicked() {
                                ui.ctx().copy_text(target.path.clone());
                                self.notify("已复制完整路径", false);
                            }
                            if ui
                                .button(egui::RichText::new("移到回收站").color(DANGER))
                                .clicked()
                            {
                                if target.id == self.root_id() {
                                    self.notify("不能删除扫描的根目录", true);
                                } else {
                                    self.pending_delete = Some(target.clone());
                                }
                            }
                        });
                    }

                    ui.add_space(8.0);
                });
            });
    }

    fn empty_state(&mut self, ui: &mut egui::Ui) {
        let height = ui.available_height();
        ui.vertical_centered(|ui| {
            ui.add_space(height * 0.30);
            ui.label(egui::RichText::new("把文件夹拖进来，或者点上面的按钮").size(15.0));
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("按磁盘实际占用统计：硬链接只算一次，稀疏文件按占用算")
                    .small()
                    .color(theme::TEXT_MUTED),
            );
            ui.add_space(14.0);
            if ui.button("浏览文件夹").clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.start_scan(dir);
                }
            }
        });
    }

    fn treemap(&mut self, ui: &mut egui::Ui) {
        let size = ui.available_size();
        let (response, painter) = ui.allocate_painter(size, Sense::click());
        let origin = response.rect.min;
        self.ensure_layout(size);

        if self.layout.is_empty() {
            painter.text(
                response.rect.center(),
                Align2::CENTER_CENTER,
                "这个文件夹没有可显示的内容",
                FontId::proportional(13.0),
                theme::TEXT_MUTED,
            );
            return;
        }

        let clip = painter.clip_rect();
        for rect in &self.layout {
            let cell = Rect::from_min_size(
                origin + egui::vec2(rect.x, rect.y),
                egui::vec2(rect.w.max(0.0), rect.h.max(0.0)),
            );
            if !clip.intersects(cell) {
                continue;
            }
            painter.rect_filled(
                cell,
                CornerRadius::same(2),
                theme::fill(&rect.kind, rect.is_dir, rect.depth),
            );
            painter.rect_stroke(
                cell,
                CornerRadius::same(2),
                Stroke::new(1.0, theme::border(&rect.kind, rect.is_dir, rect.depth)),
                StrokeKind::Inside,
            );

            if rect.subdivided {
                let caption =
                    Rect::from_min_size(cell.min, egui::vec2(cell.width(), rect.caption_height));
                painter.rect_filled(
                    caption,
                    CornerRadius::same(2),
                    Color32::from_black_alpha(96),
                );
                let roomy = rect.w >= if rect.depth == 0 { 124.0 } else { 100.0 };
                let label = if roomy {
                    format!("{} - {}", rect.name, format_bytes(rect.size))
                } else {
                    rect.name.clone()
                };
                painter.with_clip_rect(caption).text(
                    caption.left_center() + egui::vec2(4.0, 0.0),
                    Align2::LEFT_CENTER,
                    label,
                    FontId::proportional(11.0),
                    theme::TEXT,
                );
            } else if rect.w >= 26.0 && rect.h >= 12.0 {
                let inner = painter.with_clip_rect(cell.shrink(3.0));
                if rect.w >= 52.0 && rect.h >= 26.0 {
                    inner.text(
                        cell.center() - egui::vec2(0.0, 7.0),
                        Align2::CENTER_CENTER,
                        &rect.name,
                        FontId::proportional(11.5),
                        LEAF_TEXT,
                    );
                    inner.text(
                        cell.center() + egui::vec2(0.0, 7.0),
                        Align2::CENTER_CENTER,
                        format_bytes(rect.size),
                        FontId::proportional(10.0),
                        LEAF_SIZE,
                    );
                } else {
                    inner.text(
                        cell.center(),
                        Align2::CENTER_CENTER,
                        &rect.name,
                        FontId::proportional(10.5),
                        LEAF_TEXT,
                    );
                }
            }
        }

        let pointer = response.hover_pos();
        self.hovered =
            pointer.and_then(|pos| pick_cell_at(&self.layout, pos.x - origin.x, pos.y - origin.y));

        if let Some(index) = self.hovered {
            let rect = &self.layout[index];
            painter.rect_stroke(
                Rect::from_min_size(
                    origin + egui::vec2(rect.x, rect.y),
                    egui::vec2(rect.w.max(0.0), rect.h.max(0.0)),
                ),
                CornerRadius::same(2),
                Stroke::new(1.5, Color32::WHITE),
                StrokeKind::Inside,
            );
        }

        if let Some(selected) = self.selected.as_ref() {
            if let Some(rect) = self.layout.iter().find(|rect| rect.id == selected.id) {
                painter.rect_stroke(
                    Rect::from_min_size(
                        origin + egui::vec2(rect.x, rect.y),
                        egui::vec2(rect.w.max(0.0), rect.h.max(0.0)),
                    ),
                    CornerRadius::same(2),
                    Stroke::new(2.0, Color32::from_rgb(0xff, 0xc8, 0x5c)),
                    StrokeKind::Inside,
                );
            }
        }

        let clicked = if response.clicked() {
            response.interact_pointer_pos()
        } else {
            None
        };
        if let Some(pos) = clicked {
            if let Some(index) = pick_cell_at(&self.layout, pos.x - origin.x, pos.y - origin.y) {
                let rect = &self.layout[index];
                if !rect.aggregated {
                    let target = target_of(rect);
                    let (id, path, is_dir, child_count) = (
                        target.id,
                        target.path.clone(),
                        target.is_dir,
                        target.child_count,
                    );
                    self.selected = Some(target);
                    if is_dir && child_count > 0 {
                        self.drill(id, path);
                    }
                }
            }
        }

        if response.secondary_clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let target = pick_cell_at(&self.layout, pos.x - origin.x, pos.y - origin.y)
                    .map(|index| &self.layout[index])
                    .filter(|rect| !rect.aggregated)
                    .map(target_of);
                if let Some(target) = &target {
                    self.selected = Some(target.clone());
                }
                self.menu = target.map(|target| MenuState { target, pos });
            }
        }
    }

    #[allow(deprecated)]
    fn tooltip(&self, ctx: &egui::Context, rect: &LaidOutRect, pos: egui::Pos2) {
        let total = self.total_size();
        let meta = theme::meta(&rect.kind);
        egui::Area::new(egui::Id::new("tm-tooltip"))
            .order(egui::Order::Tooltip)
            .fixed_pos(pos + egui::vec2(16.0, 18.0))
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_max_width(340.0);
                    ui.label(egui::RichText::new(&rect.name).strong().color(meta.color));
                    ui.label(format!(
                        "{} · {} · {}",
                        format_bytes(rect.size),
                        format_percent(rect.size, total),
                        meta.label
                    ));
                    if rect.is_dir && !rect.aggregated {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} 个直接子项",
                                format_count(rect.child_count as u64)
                            ))
                            .color(theme::TEXT_MUTED),
                        );
                    }
                    if !rect.path.is_empty() {
                        ui.label(
                            egui::RichText::new(&rect.path)
                                .small()
                                .color(theme::TEXT_MUTED),
                        );
                    }
                    ui.label(
                        egui::RichText::new(if rect.is_dir {
                            "单击进入 · 右键菜单"
                        } else {
                            "单击选中 · 右键菜单"
                        })
                        .small()
                        .color(theme::TEXT_MUTED),
                    );
                });
            });
    }
}

impl eframe::App for DiskSnifferApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_scan(&ctx);
        self.poll_trash(&ctx);

        let dropped = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .first()
                .map(|file| file.path().to_path_buf())
        });
        if let Some(path) = dropped {
            let target = if path.is_dir() {
                Some(path)
            } else {
                path.parent().map(|parent| parent.to_path_buf())
            };
            if let Some(target) = target {
                self.start_scan(target);
            }
        }

        if self.menu.is_none()
            && self.pending_delete.is_none()
            && ctx.input(|input| input.key_pressed(egui::Key::Backspace))
        {
            self.go_up();
        }
        if self.menu.is_none()
            && self.pending_delete.is_none()
            && ctx.input(|input| input.key_pressed(egui::Key::Escape))
        {
            if let Scan::Running { cancel, .. } = &self.scan {
                cancel.store(true, Ordering::Relaxed);
            }
        }

        self.top_bar(ui);
        self.side_panel(ui);

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme::BACKGROUND))
            .show(ui, |ui| {
                if self.tree.is_some() {
                    self.treemap(ui);
                } else {
                    self.empty_state(ui);
                }
            });

        self.draw_menu(&ctx);
        self.draw_confirm(&ctx);
        self.draw_toast(&ctx);

        if let Some(index) = self.hovered {
            if let Some(pos) = ctx.input(|input| input.pointer.hover_pos()) {
                if let Some(rect) = self.layout.get(index).cloned() {
                    self.tooltip(&ctx, &rect, pos);
                }
            }
        }
    }
}

impl DiskSnifferApp {
    fn root_id(&self) -> u32 {
        self.summary
            .as_ref()
            .map(|summary| summary.root_id)
            .unwrap_or(0)
    }

    fn notify(&mut self, text: impl Into<String>, error: bool) {
        self.toast = Some(Toast {
            text: text.into(),
            error,
            at: Instant::now(),
        });
    }

    /// Drops the cached layout and refreshes the totals after the tree changed
    /// on disk (only a recycle bin move does that today).
    fn refresh_tree_views(&mut self) {
        if let Some(tree) = &self.tree {
            self.summary = Some(tree.summary());
            self.top_files = tree.top_files(12);
        }
        self.layout.clear();
        self.invalidate_layout();
    }

    /// The shell move can take a moment on big folders, so it leaves the UI thread.
    fn start_trash(&mut self, target: ContextTarget) {
        let tree = match &self.tree {
            Some(tree) => tree.clone(),
            None => return,
        };
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = tree.remove_subtree(target.id);
            let _ = tx.send((result, target.name));
        });
        self.trash = Some(rx);
    }

    fn poll_trash(&mut self, ctx: &egui::Context) {
        let incoming = match &self.trash {
            Some(rx) => match rx.try_recv() {
                Ok(value) => Some(value),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => {
                    Some((Err("回收站线程异常结束".to_string()), String::new()))
                }
            },
            None => None,
        };

        match incoming {
            Some((Ok(()), name)) => {
                self.trash = None;
                self.selected = None;
                self.refresh_tree_views();
                self.notify(format!("已移到回收站: {}", name), false);
            }
            Some((Err(error), _)) => {
                self.trash = None;
                self.notify(error, true);
            }
            None => {
                if self.trash.is_some() {
                    ctx.request_repaint_after(Duration::from_millis(120));
                }
            }
        }
    }

    #[allow(deprecated)]
    fn draw_menu(&mut self, ctx: &egui::Context) {
        let Some(menu) = self.menu.clone() else {
            return;
        };
        let target = menu.target.clone();
        let mut action: Option<MenuAction> = None;

        let area = egui::Area::new(egui::Id::new("node-menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(menu.pos + egui::vec2(2.0, 2.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(200.0);
                    ui.label(
                        egui::RichText::new(&target.name)
                            .strong()
                            .color(theme::TEXT),
                    );
                    ui.separator();
                    if target.is_dir
                        && target.child_count > 0
                        && ui.button("在树图中打开").clicked()
                    {
                        action = Some(MenuAction::Drill);
                    }
                    let open_label = if target.is_dir {
                        "在文件管理器中打开"
                    } else {
                        "在文件管理器中定位"
                    };
                    if ui.button(open_label).clicked() {
                        action = Some(if target.is_dir {
                            MenuAction::Open
                        } else {
                            MenuAction::Reveal
                        });
                    }
                    if ui.button("复制完整路径").clicked() {
                        action = Some(MenuAction::Copy);
                    }
                    ui.separator();
                    if ui
                        .button(egui::RichText::new("移到回收站").color(DANGER))
                        .clicked()
                    {
                        action = Some(MenuAction::Trash);
                    }
                });
            });

        // The menu is a plain area, so it closes itself on any click outside its
        // rect or on Esc. The anchor point is part of the rect because the click
        // that opened the menu sits exactly there.
        let rect = area
            .response
            .rect
            .union(Rect::from_min_size(menu.pos, egui::Vec2::ZERO));
        let outside_click = ctx.input(|input| {
            input.pointer.any_click()
                && input
                    .pointer
                    .interact_pos()
                    .map(|pos| !rect.contains(pos))
                    .unwrap_or(false)
        });
        let escaped = ctx.input(|input| input.key_pressed(egui::Key::Escape));

        match action {
            Some(action) => {
                self.menu = None;
                self.apply_menu_action(ctx, target, action);
            }
            None if outside_click || escaped => self.menu = None,
            None => {}
        }
    }

    #[allow(deprecated)]
    fn draw_confirm(&mut self, ctx: &egui::Context) {
        let Some(target) = self.pending_delete.clone() else {
            return;
        };

        let modal = egui::Modal::new(egui::Id::new("confirm-trash")).show(ctx, |ui| {
            ui.set_min_width(320.0);
            ui.label(egui::RichText::new("移到回收站").strong().size(15.0));
            ui.add_space(4.0);
            ui.label(format!("{} · {}", target.name, format_bytes(target.size)));
            ui.add(
                egui::Label::new(
                    egui::RichText::new(&target.path)
                        .small()
                        .color(theme::TEXT_MUTED),
                )
                .truncate(),
            );
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("目录会连同里面的内容一起移走，之后可以从回收站还原。")
                    .small()
                    .color(theme::TEXT_MUTED),
            );
            ui.add_space(10.0);
            let mut confirm = false;
            let mut cancel = false;
            ui.horizontal(|ui| {
                if ui.button("取消").clicked() {
                    cancel = true;
                }
                if ui
                    .button(egui::RichText::new("移到回收站").color(DANGER))
                    .clicked()
                {
                    confirm = true;
                }
            });
            (confirm, cancel)
        });

        let (confirm, cancel) = modal.inner;
        if confirm {
            self.pending_delete = None;
            self.selected = None;
            self.start_trash(target);
        } else if cancel || modal.should_close() {
            self.pending_delete = None;
        }
    }

    #[allow(deprecated)]
    fn draw_toast(&mut self, ctx: &egui::Context) {
        let expired = match &self.toast {
            Some(toast) => toast.at.elapsed() > Duration::from_secs(4),
            None => false,
        };
        if expired {
            self.toast = None;
            return;
        }
        let Some(toast) = self.toast.clone() else {
            return;
        };
        ctx.request_repaint_after(Duration::from_millis(200));

        let color = if toast.error { MUTED_WARM } else { theme::TEXT };
        egui::Area::new(egui::Id::new("toast"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -28.0))
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(egui::RichText::new(&toast.text).color(color));
                });
            });
    }

    fn apply_menu_action(
        &mut self,
        ctx: &egui::Context,
        target: ContextTarget,
        action: MenuAction,
    ) {
        match action {
            MenuAction::Drill => {
                if target.child_count > 0 {
                    self.drill(target.id, target.path.clone());
                } else {
                    self.selected = Some(target);
                }
            }
            MenuAction::Open => {
                if let Err(error) = fsops::open_dir(&target.path) {
                    self.notify(error, true);
                }
            }
            MenuAction::Reveal => {
                if let Err(error) = fsops::reveal(&target.path) {
                    self.notify(error, true);
                }
            }
            MenuAction::Copy => {
                ctx.copy_text(target.path.clone());
                self.notify("已复制完整路径", false);
            }
            MenuAction::Trash => {
                if target.id == self.root_id() {
                    self.notify("不能删除扫描的根目录", true);
                } else {
                    self.pending_delete = Some(target);
                }
            }
        }
    }
}
