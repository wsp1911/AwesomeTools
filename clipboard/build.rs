//! Generates the executable icon from `src/icon.rs` and embeds it as the ID 1
//! icon resource.
//!
//! Windows takes the file, taskbar, title bar and Alt+Tab icons from that
//! resource: GPUI creates its window class with
//! `LoadImageW(GetModuleHandleW(None), MAKEINTRESOURCE(1), IMAGE_ICON, ..)`, so
//! one generated resource covers all of them, and there is no binary icon in the
//! repository.

#[path = "src/icon.rs"]
mod icon;

use std::fs;
use std::path::PathBuf;

/// Sizes Windows asks for: 16 and 20 for the title bar and the tray, 24 to 48
/// for the taskbar and the file manager, 64 and up for large thumbnails.
const SIZES: [u32; 8] = [16, 20, 24, 32, 40, 48, 64, 128];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/icon.rs");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR is set by cargo"));
    let ico_path = out_dir.join("clipboard.ico");
    fs::write(&ico_path, encode_ico(&SIZES)).expect("write the generated icon");

    // The resource script is generated too, so the icon path can point into the
    // build directory. Forward slashes keep the path out of reach of the
    // resource compiler's escape handling.
    let script_path = out_dir.join("clipboard.rc");
    let ico = ico_path.display().to_string().replace('\\', "/");
    fs::write(&script_path, format!("1 ICON \"{ico}\"\n")).expect("write the resource script");

    // The icon is the whole point of this build script, so a resource compiler
    // that is missing or failing is an error, not a warning.
    match embed_resource::compile(&script_path, embed_resource::NONE) {
        embed_resource::CompilationResult::Ok => {}
        other => panic!("embedding the icon resource failed: {other:?}"),
    }
}

/// Encodes a multi-image .ico: every entry is a 32-bit DIB (a BITMAPINFOHEADER
/// followed by BGRA rows, bottom row first) plus the 1-bit AND mask the format
/// still expects. The alpha channel carries the transparency, so the mask is
/// empty.
fn encode_ico(sizes: &[u32]) -> Vec<u8> {
    let images: Vec<Vec<u8>> = sizes
        .iter()
        .map(|&size| encode_image(size, &icon::rgba(size)))
        .collect();

    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // 1 = icon
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes());

    let mut offset = 6 + 16 * sizes.len();
    for (&size, image) in sizes.iter().zip(&images) {
        // 256 pixels are stored as 0 in this byte.
        let edge = u8::try_from(size).unwrap_or(0);
        out.push(edge); // width
        out.push(edge); // height
        out.push(0); // palette entries
        out.push(0); // reserved
        out.extend_from_slice(&1u16.to_le_bytes()); // colour planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&(image.len() as u32).to_le_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += image.len();
    }

    for image in images {
        out.extend_from_slice(&image);
    }
    out
}

fn encode_image(size: u32, rgba: &[u8]) -> Vec<u8> {
    let mask_row = (size as usize).div_ceil(32) * 4;
    let mask_len = mask_row * size as usize;
    let pixels = (size * size * 4) as usize;

    let mut image = Vec::with_capacity(40 + pixels + mask_len);
    image.extend_from_slice(&40u32.to_le_bytes()); // header size
    image.extend_from_slice(&(size as i32).to_le_bytes()); // width
    // The header height covers the colour bitmap and the mask.
    image.extend_from_slice(&(size as i32 * 2).to_le_bytes());
    image.extend_from_slice(&1u16.to_le_bytes()); // planes
    image.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
    image.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    image.extend_from_slice(&((pixels + mask_len) as u32).to_le_bytes());
    for _ in 0..4 {
        image.extend_from_slice(&0u32.to_le_bytes()); // resolution and palette
    }

    for y in (0..size).rev() {
        for x in 0..size {
            let offset = ((y * size + x) * 4) as usize;
            image.push(rgba[offset + 2]);
            image.push(rgba[offset + 1]);
            image.push(rgba[offset]);
            image.push(rgba[offset + 3]);
        }
    }

    image.resize(image.len() + mask_len, 0);
    image
}
