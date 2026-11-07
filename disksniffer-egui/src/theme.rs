use eframe::egui::Color32;

pub struct KindMeta {
    pub key: &'static str,
    pub label: &'static str,
    pub color: Color32,
}

/// Order matches disksniffer_core::scan::KIND_KEYS.
pub const KINDS: [KindMeta; 10] = [
    KindMeta { key: "video", label: "视频", color: Color32::from_rgb(0xe0, 0x60, 0x54) },
    KindMeta { key: "image", label: "图片", color: Color32::from_rgb(0xdf, 0xa1, 0x4a) },
    KindMeta { key: "audio", label: "音频", color: Color32::from_rgb(0xa9, 0x8c, 0xf0) },
    KindMeta { key: "document", label: "文档", color: Color32::from_rgb(0x4f, 0x9d, 0xf0) },
    KindMeta { key: "code", label: "代码", color: Color32::from_rgb(0x37, 0xb3, 0xa8) },
    KindMeta { key: "archive", label: "压缩包", color: Color32::from_rgb(0xc8, 0xac, 0x3a) },
    KindMeta { key: "executable", label: "程序", color: Color32::from_rgb(0x8a, 0x7c, 0xf2) },
    KindMeta { key: "system", label: "系统", color: Color32::from_rgb(0x67, 0x78, 0x8e) },
    KindMeta { key: "font", label: "字体", color: Color32::from_rgb(0x5c, 0xb9, 0x6b) },
    KindMeta { key: "other", label: "其他", color: Color32::from_rgb(0x78, 0x85, 0x9a) },
];

pub const BACKGROUND: Color32 = Color32::from_rgb(0x0b, 0x0e, 0x14);
pub const PANEL: Color32 = Color32::from_rgb(0x12, 0x17, 0x20);
pub const TEXT: Color32 = Color32::from_rgb(0xe8, 0xed, 0xf5);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x8c, 0x98, 0xac);

pub fn meta(kind: &str) -> &'static KindMeta {
    KINDS
        .iter()
        .find(|entry| entry.key == kind)
        .unwrap_or(&KINDS[KINDS.len() - 1])
}

fn with_alpha(color: Color32, alpha: f32) -> Color32 {
    let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a)
}

/// Containers stay translucent so nested cells read through; leaves get the
/// fuller type colour, fading slightly with depth.
pub fn fill(kind: &str, is_dir: bool, depth: u32) -> Color32 {
    let color = meta(kind).color;
    if is_dir {
        with_alpha(color, if depth == 0 { 0.20 } else { 0.13 })
    } else {
        let fade = (0.62 - depth as f32 * 0.12).max(0.34);
        with_alpha(color, fade)
    }
}

pub fn border(kind: &str, is_dir: bool, depth: u32) -> Color32 {
    let color = meta(kind).color;
    if is_dir {
        with_alpha(color, if depth == 0 { 0.45 } else { 0.28 })
    } else {
        let fade = (0.62 - depth as f32 * 0.12).max(0.34);
        with_alpha(color, (fade + 0.16).min(1.0))
    }
}
