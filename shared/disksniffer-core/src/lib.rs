//! Everything the DiskSniffer tools need from the file system side:
//!
//! - [`scan`] walks a directory tree in parallel and reports every file and
//!   folder with its real on-disk footprint (allocation size, hard links
//!   counted once, reparse points skipped).
//! - [`tree`] packs that result into a compact index, spills it to a memory
//!   mapped temp file and answers the queries the UI makes: children pages,
//!   paths, search, top files, totals and recycle bin deletion.
//!
//! Both modules are GUI agnostic: the Tauri build and the egui build use this
//! exact code.

pub mod scan;
pub mod tree;
