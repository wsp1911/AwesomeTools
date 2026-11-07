//! Gives DiskSniffer its own icon instead of eframe's default `e`.
//!
//! Two artefacts come out of the artwork in `assets/`, and they cover two
//! different surfaces:
//! - `icon.rgba` feeds `egui::IconData`, which is what the title bar, the
//!   taskbar and Alt+Tab show. eframe only uses its built-in icon when this is
//!   left unset.
//! - an ID 1 icon resource inside the executable, which is where Windows takes
//!   the *file* icon from; without it the exe keeps the generic application
//!   icon.
//!
//! No decoding or resource compiler runs at runtime, so the released binary
//! stays free of extra dependencies.

use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/icon.png");
    println!("cargo:rerun-if-changed=assets/icon.ico");

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR is set by cargo"));

    let (width, height) = decode_png(Path::new("assets/icon.png"), &out_dir.join("icon.rgba"));
    fs::write(
        out_dir.join("icon_size.rs"),
        format!("const ICON_WIDTH: u32 = {width};\nconst ICON_HEIGHT: u32 = {height};\n"),
    )
    .expect("write the icon size");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    // The resource script is generated so it can point at the artwork through an
    // absolute path; forward slashes keep the path clear of the resource
    // compiler's escape handling.
    let ico = Path::new("assets/icon.ico")
        .canonicalize()
        .expect("locate assets/icon.ico")
        .display()
        .to_string();
    let ico = ico
        .strip_prefix("\\\\?\\")
        .unwrap_or(&ico)
        .replace('\\', "/");
    let script = out_dir.join("icon.rc");
    fs::write(&script, format!("1 ICON \"{ico}\"\n")).expect("write the resource script");

    // The icon is the whole point of this build script, so a missing or failing
    // resource compiler is an error rather than a warning.
    match embed_resource::compile(&script, embed_resource::NONE) {
        embed_resource::CompilationResult::Ok => {}
        other => panic!("embedding the icon resource failed: {other:?}"),
    }
}

/// Writes the PNG's pixels to `target` as straight-alpha RGBA rows and returns
/// the image size.
fn decode_png(source: &Path, target: &Path) -> (u32, u32) {
    let file =
        File::open(source).unwrap_or_else(|error| panic!("open {}: {error}", source.display()));
    let mut reader = png::Decoder::new(BufReader::new(file))
        .read_info()
        .unwrap_or_else(|error| panic!("read {}: {error}", source.display()));

    let mut pixels = vec![
        0u8;
        reader
            .output_buffer_size()
            .expect("the icon has a buffer size")
    ];
    let info = reader
        .next_frame(&mut pixels)
        .unwrap_or_else(|error| panic!("decode {}: {error}", source.display()));
    assert_eq!(
        info.color_type,
        png::ColorType::Rgba,
        "{} must be RGBA",
        source.display()
    );
    assert_eq!(
        info.bit_depth,
        png::BitDepth::Eight,
        "{} must be 8 bits per channel",
        source.display()
    );
    pixels.truncate(info.buffer_size());
    fs::write(target, &pixels).expect("write the decoded icon");

    (info.width, info.height)
}
