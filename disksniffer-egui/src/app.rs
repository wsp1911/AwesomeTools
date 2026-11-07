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
use crate::layout::{layout_treemap, pick_cell_at, LaidOutRect, MAX_ITEMS};
use crate::theme;

const MUTED_WARM: Color32 = Color32::from_rgb(0xf0, 0x8a, 0x7c);
const LEAF_TEXT: Color32 = Color32::from_rgb(0xf3, 0xf6, 0xfb);
const LEAF_SIZE: Color32 = Color32::from_rgb(0xc9, 0xd3, 0xe2);

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
    selected: Option<u32>,
    status: Option<String>,
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
        self.summary.as_ref().map(|summary| summary.total_size).unwrap_or(0)
    }

    fn invalidate_layout(&mut self) {
        self.layout_key = None;
        self.hovered = None;
    }

    fn start_scan(&mut self, path: PathBuf) {
        if matches!(self.scan, Scan::Running { .. }) {
            return;
        }
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
        self.tree = Some(payload.tree);
        self.summary = Some(payload.summary);
        self.top_files = payload.top_files;
        self.layout.clear();
        self.invalidate_layout();
        self.selected = None;
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
        }
    }

    fn jump_to(&mut self, index: usize) {
        if index + 1 < self.trail.len() {
            self.trail.truncate(index + 1);
            self.layout.clear();
            self.invalidate_layout();
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
                if ui.button("选择文件夹").clicked() {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        self.start_scan(dir);
                    }
                }

                if let Scan::Running { progress, .. } = &self.scan {
                    ui.separator();
                    ui.label(
                        egui::RichText::new(format!(
                            "扫描中 · {} 文件 · {} 目录 · {}",
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
                                egui::RichText::new(current).small().color(theme::TEXT_MUTED),
                            )
                            .truncate(),
                        );
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if matches!(self.scan, Scan::Running { .. }) && ui.button("停止").clicked() {
                        if let Scan::Running { cancel, .. } = &self.scan {
                            cancel.store(true, Ordering::Relaxed);
                        }
                    }
                    if self.tree.is_some() && ui.button("重新扫描").clicked() {
                        if let Some(path) = self.trail.first().map(|entry| entry.1.clone()) {
                            self.start_scan(PathBuf::from(path));
                        }
                    }
                });
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

        let frame = egui::Frame::NONE
            .fill(theme::PANEL)
            .inner_margin(egui::Margin::symmetric(10, 8));
        egui::Panel::right("sidebar")
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
                        ui.label(egui::RichText::new("目录").color(theme::TEXT_MUTED));
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
                ui.label(egui::RichText::new("类型分布").small().color(theme::TEXT_MUTED));
                ui.add_space(4.0);
                for kind in &summary.kinds {
                    if kind.size == 0 {
                        continue;
                    }
                    let meta = theme::meta(&kind.key);
                    ui.horizontal(|ui| {
                        let (dot, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), Sense::hover());
                        ui.painter().rect_filled(dot, CornerRadius::same(2), meta.color);
                        ui.label(meta.label);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(format_percent(kind.size, summary.total_size))
                                    .color(theme::TEXT_MUTED),
                            );
                            ui.label(format_bytes(kind.size));
                        });
                    });
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
                            ui.add(
                                egui::Label::new(egui::RichText::new(&file.name).size(11.5))
                                    .truncate(),
                            )
                            .on_hover_text(&file.path);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(
                                    egui::RichText::new(format_bytes(file.size))
                                        .small()
                                        .color(theme::TEXT_MUTED),
                                );
                            });
                        });
                    }
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
            if ui.button("选择文件夹").clicked() {
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
                painter.rect_filled(caption, CornerRadius::same(2), Color32::from_black_alpha(96));
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

        if let Some(selected) = self.selected {
            if let Some(rect) = self.layout.iter().find(|rect| rect.id == selected) {
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
                let target = self.layout[index].clone();
                if !target.aggregated {
                    self.selected = Some(target.id);
                    if target.is_dir && target.child_count > 0 {
                        self.drill(target.id, target.path.clone());
                    }
                }
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
                        egui::RichText::new(if rect.is_dir { "单击进入" } else { "单击选中" })
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

        if ctx.input(|input| input.key_pressed(egui::Key::Backspace)) {
            self.go_up();
        }
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
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

        if let Some(index) = self.hovered {
            if let Some(pos) = ctx.input(|input| input.pointer.hover_pos()) {
                if let Some(rect) = self.layout.get(index).cloned() {
                    self.tooltip(&ctx, &rect, pos);
                }
            }
        }
    }
}
