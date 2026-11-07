use rayon::prelude::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub const KIND_VIDEO: u8 = 0;
pub const KIND_IMAGE: u8 = 1;
pub const KIND_AUDIO: u8 = 2;
pub const KIND_DOCUMENT: u8 = 3;
pub const KIND_CODE: u8 = 4;
pub const KIND_ARCHIVE: u8 = 5;
pub const KIND_EXECUTABLE: u8 = 6;
pub const KIND_SYSTEM: u8 = 7;
pub const KIND_FONT: u8 = 8;
pub const KIND_OTHER: u8 = 9;

pub const KIND_KEYS: [&str; 10] = [
    "video",
    "image",
    "audio",
    "document",
    "code",
    "archive",
    "executable",
    "system",
    "font",
    "other",
];

const VERBATIM_UNC: &str = "\\\\?\\UNC\\";
const VERBATIM: &str = "\\\\?\\";

/// One directory entry as reported by the OS.
///
/// `size` is the disk space the file actually occupies (allocation size), not
/// its logical length, and `id` identifies the underlying file so hard links
/// can be counted once. A zero `id` means "no identity available".
pub struct FileEntry {
    pub name: String,
    pub is_dir: bool,
    pub is_reparse: bool,
    pub size: u64,
    pub id: u64,
}

/// A node produced by the scan, before being flattened into the tree arena.
pub struct RawNode {
    pub name: Box<str>,
    pub size: u64,
    pub is_dir: bool,
    pub kind: u8,
    pub dominant: u8,
    pub children: Vec<RawNode>,
}

impl RawNode {
    fn empty_dir(name: Box<str>) -> Self {
        RawNode {
            name,
            size: 0,
            is_dir: true,
            kind: KIND_OTHER,
            dominant: KIND_OTHER,
            children: Vec::new(),
        }
    }
}

const SEEN_SHARDS: usize = 64;

/// Shared, thread safe scan counters used for progress reporting.
pub struct ScanProgress {
    pub files: AtomicU64,
    pub dirs: AtomicU64,
    pub bytes: AtomicU64,
    pub errors: AtomicU64,
    pub current: Mutex<String>,
    pub cancel: Arc<AtomicBool>,
    pub finished: AtomicBool,
    /// File identities already counted, sharded to keep contention low.
    seen: Vec<Mutex<HashSet<u64>>>,
}

impl ScanProgress {
    pub fn new(cancel: Arc<AtomicBool>) -> Self {
        ScanProgress {
            files: AtomicU64::new(0),
            dirs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            current: Mutex::new(String::new()),
            cancel,
            finished: AtomicBool::new(false),
            seen: (0..SEEN_SHARDS)
                .map(|_| Mutex::new(HashSet::new()))
                .collect(),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Returns true the first time an identity is claimed.
    fn claim(&self, id: u64) -> bool {
        let index = ((id.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 58) as usize) & (SEEN_SHARDS - 1);
        match self.seen[index].lock() {
            Ok(mut seen) => seen.insert(id),
            Err(_) => true,
        }
    }

    fn report_path(&self, path: &Path) {
        if let Ok(mut slot) = self.current.lock() {
            *slot = path.to_string_lossy().into_owned();
        }
    }
}

#[cfg(windows)]
fn read_directory(dir: &Path) -> Option<Vec<FileEntry>> {
    windows_dir::read(dir)
}

#[cfg(not(windows))]
fn read_directory(dir: &Path) -> Option<Vec<FileEntry>> {
    let read = std::fs::read_dir(dir).ok()?;
    let mut entries = Vec::new();
    for item in read {
        let item = match item {
            Ok(item) => item,
            Err(_) => continue,
        };
        let meta = match item.metadata() {
            Ok(meta) => meta,
            Err(_) => continue,
        };
        let (size, id) = allocated_size(&meta);
        entries.push(FileEntry {
            name: item.file_name().to_string_lossy().into_owned(),
            is_dir: meta.is_dir(),
            is_reparse: meta.file_type().is_symlink(),
            size,
            id,
        });
    }
    Some(entries)
}

#[cfg(unix)]
fn allocated_size(meta: &std::fs::Metadata) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt;
    let size = meta.blocks().saturating_mul(512);
    let id = if meta.nlink() > 1 {
        meta.dev().wrapping_shl(32) ^ meta.ino()
    } else {
        0
    };
    (size, id)
}

#[cfg(all(not(unix), not(windows)))]
fn allocated_size(meta: &std::fs::Metadata) -> (u64, u64) {
    (meta.len(), 0)
}

/// Recursively walks a directory tree, fanning out work across the rayon pool.
pub fn scan_dir(dir: &Path, name: Box<str>, progress: &ScanProgress, depth: u32) -> RawNode {
    progress.dirs.fetch_add(1, Ordering::Relaxed);
    progress.report_path(dir);

    let entries = match read_directory(dir) {
        Some(entries) => entries,
        None => {
            progress.errors.fetch_add(1, Ordering::Relaxed);
            return RawNode::empty_dir(name);
        }
    };

    let mut children: Vec<RawNode> = entries
        .into_par_iter()
        .filter_map(|entry| {
            if progress.is_cancelled() {
                return None;
            }
            let FileEntry {
                name: raw_name,
                is_dir,
                is_reparse,
                size,
                id,
            } = entry;
            let child_name = raw_name.into_boxed_str();

            if is_dir {
                // Junctions and directory symlinks are skipped so the walk can
                // never loop or visit the same subtree twice.
                if is_reparse {
                    return None;
                }
                return Some(scan_dir(
                    &dir.join(&*child_name),
                    child_name,
                    progress,
                    depth + 1,
                ));
            }

            // Hard links share one identity: only the first path pays for it.
            let counted = if id != 0 && !progress.claim(id) {
                0
            } else {
                size
            };
            progress.files.fetch_add(1, Ordering::Relaxed);
            progress.bytes.fetch_add(counted, Ordering::Relaxed);
            let kind = category_of(&child_name);
            Some(RawNode {
                name: child_name,
                size: counted,
                is_dir: false,
                kind,
                dominant: kind,
                children: Vec::new(),
            })
        })
        .collect();

    children.shrink_to_fit();

    let mut size = 0u64;
    let mut biggest = 0u64;
    let mut dominant = KIND_OTHER;
    for child in &children {
        size = size.saturating_add(child.size);
        if child.size > biggest {
            biggest = child.size;
            dominant = child.dominant;
        }
    }

    RawNode {
        name,
        size,
        is_dir: true,
        kind: KIND_OTHER,
        dominant,
        children,
    }
}

#[cfg(windows)]
mod windows_dir {
    use super::FileEntry;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_NO_MORE_FILES, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FileIdBothDirectoryInfo, GetFileInformationByHandleEx,
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_ID_BOTH_DIR_INFO, FILE_LIST_DIRECTORY, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    const BUFFER_BYTES: usize = 64 * 1024;

    struct HandleGuard(windows_sys::Win32::Foundation::HANDLE);

    impl Drop for HandleGuard {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    /// Reads a directory in bulk, returning allocation size and file id per
    /// entry. This avoids one metadata syscall per file and is what makes
    /// hard link de-duplication and "size on disk" accurate.
    pub fn read(dir: &Path) -> Option<Vec<FileEntry>> {
        let wide: Vec<u16> = dir
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let handle = CreateFileW(
                wide.as_ptr(),
                FILE_LIST_DIRECTORY,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS,
                std::ptr::null_mut(),
            );
            if handle == INVALID_HANDLE_VALUE {
                return None;
            }
            let _guard = HandleGuard(handle);

            let mut buffer = vec![0u64; BUFFER_BYTES / std::mem::size_of::<u64>()];
            let mut entries = Vec::new();

            loop {
                let ok = GetFileInformationByHandleEx(
                    handle,
                    FileIdBothDirectoryInfo,
                    buffer.as_mut_ptr() as *mut std::ffi::c_void,
                    BUFFER_BYTES as u32,
                );
                if ok == 0 {
                    let error = std::io::Error::last_os_error();
                    if error.raw_os_error() == Some(ERROR_NO_MORE_FILES as i32) {
                        break;
                    }
                    return None;
                }

                let base = buffer.as_ptr() as *const u8;
                let mut offset = 0usize;
                loop {
                    let info = base.add(offset) as *const FILE_ID_BOTH_DIR_INFO;
                    let name_length = (*info).FileNameLength as usize / 2;
                    let attributes = (*info).FileAttributes;
                    let name_pointer = std::ptr::addr_of!((*info).FileName) as *const u16;
                    let name = if name_length > 0 {
                        String::from_utf16_lossy(std::slice::from_raw_parts(
                            name_pointer,
                            name_length,
                        ))
                    } else {
                        String::new()
                    };

                    if !name.is_empty() && name != "." && name != ".." {
                        entries.push(FileEntry {
                            name,
                            is_dir: attributes & FILE_ATTRIBUTE_DIRECTORY != 0,
                            is_reparse: attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0,
                            size: (*info).AllocationSize.max(0) as u64,
                            id: (*info).FileId as u64,
                        });
                    }

                    let next = (*info).NextEntryOffset;
                    if next == 0 {
                        break;
                    }
                    offset += next as usize;
                }
            }

            Some(entries)
        }
    }
}

fn extension_of(name: &str) -> Option<String> {
    let index = name.rfind('.')?;
    if index == 0 || index + 1 >= name.len() {
        return None;
    }
    Some(name[index + 1..].to_ascii_lowercase())
}

pub fn category_of(name: &str) -> u8 {
    let ext = match extension_of(name) {
        Some(ext) => ext,
        None => return KIND_OTHER,
    };
    match ext.as_str() {
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" | "mpg" | "mpeg"
        | "m2ts" | "mts" | "vob" | "3gp" | "rmvb" | "rm" | "asf" | "ogv" | "divx" => KIND_VIDEO,
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "svg" | "ico" | "tif" | "tiff" | "psd"
        | "raw" | "cr2" | "nef" | "heic" | "avif" | "jfif" | "ai" | "xcf" | "dng" => KIND_IMAGE,
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "wma" | "opus" | "aiff" | "aif" | "mid"
        | "midi" | "ape" | "dsf" | "amr" => KIND_AUDIO,
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "xlsm" | "ppt" | "pptx" | "txt" | "md" | "rtf"
        | "odt" | "ods" | "odp" | "epub" | "mobi" | "csv" | "pages" | "numbers" | "key" | "tex"
        | "one" | "msg" | "eml" => KIND_DOCUMENT,
        "rs" | "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "py" | "java" | "c" | "h" | "cc"
        | "cpp" | "hpp" | "cs" | "go" | "rb" | "php" | "html" | "htm" | "css" | "scss" | "sass"
        | "less" | "json" | "yaml" | "yml" | "toml" | "xml" | "sh" | "bash" | "ps1" | "sql"
        | "vue" | "svelte" | "kt" | "swift" | "lua" | "dart" | "gradle" | "cmake" | "map" => {
            KIND_CODE
        }
        "zip" | "rar" | "7z" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "zst" | "iso" | "cab"
        | "dmg" | "wim" | "lz" | "lzma" | "pkg" | "deb" => KIND_ARCHIVE,
        "exe" | "dll" | "msi" | "sys" | "bat" | "cmd" | "com" | "scr" | "app" | "apk" | "ipa"
        | "so" | "dylib" | "lib" | "o" | "obj" | "pdb" | "class" | "jar" | "bin" | "elf" | "msix"
        | "appx" => KIND_EXECUTABLE,
        "log" | "tmp" | "temp" | "dat" | "bak" | "cache" | "ini" | "cfg" | "conf" | "lock"
        | "db" | "sqlite" | "sqlite3" | "edb" | "lnk" | "thumbs" | "reg" | "plist" => KIND_SYSTEM,
        "ttf" | "otf" | "woff" | "woff2" | "fon" | "ttc" | "eot" => KIND_FONT,
        _ => KIND_OTHER,
    }
}

pub fn kind_key(kind: u8) -> &'static str {
    KIND_KEYS[kind as usize % KIND_KEYS.len()]
}

fn home_dir() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("USERPROFILE") {
        if !home.is_empty() {
            return Some(PathBuf::from(home));
        }
    }
    std::env::var("HOME")
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn expand_home(input: &str) -> PathBuf {
    if let Some(home) = home_dir() {
        if input == "~" {
            return home;
        }
        if let Some(rest) = input.strip_prefix("~/").or_else(|| input.strip_prefix("~\\")) {
            if !rest.is_empty() {
                return home.join(rest);
            }
            return home;
        }
    }
    PathBuf::from(input)
}

/// Resolves user input into an absolute path without the Windows verbatim prefix,
/// so paths stay readable and can be handed to Explorer.
pub fn normalize_path(input: &str) -> Result<PathBuf, String> {
    let trimmed = input.trim().trim_matches('"');
    if trimmed.is_empty() {
        return Err("请输入要扫描的路径".to_string());
    }
    let expanded = expand_home(trimmed);
    let canonical = std::fs::canonicalize(&expanded)
        .map_err(|error| format!("无法访问该路径：{}", error))?;
    Ok(strip_verbatim(canonical))
}

#[cfg(windows)]
fn strip_verbatim(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy().into_owned();
    if let Some(rest) = text.strip_prefix(VERBATIM_UNC) {
        PathBuf::from(format!("\\\\{}", rest))
    } else if let Some(rest) = text.strip_prefix(VERBATIM) {
        PathBuf::from(rest.to_string())
    } else {
        path
    }
}

#[cfg(not(windows))]
fn strip_verbatim(path: PathBuf) -> PathBuf {
    path
}

pub fn list_drives() -> Vec<String> {
    #[cfg(windows)]
    {
        let mut drives = Vec::new();
        for letter in b'A'..=b'Z' {
            let candidate = format!("{}:\\", letter as char);
            if Path::new(&candidate).is_dir() {
                drives.push(candidate);
            }
        }
        drives
    }
    #[cfg(not(windows))]
    {
        vec!["/".to_string()]
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "disksniffer-scan-{}-{}",
            tag,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        base
    }

    /// Returns the tree plus (files, dirs, errors).
    fn scan(root: &Path) -> (RawNode, u64, u64, u64) {
        let progress = ScanProgress::new(Arc::new(AtomicBool::new(false)));
        let name = root.to_string_lossy().into_owned().into_boxed_str();
        let raw = scan_dir(root, name, &progress, 0);
        (
            raw,
            progress.files.load(Ordering::Relaxed),
            progress.dirs.load(Ordering::Relaxed),
            progress.errors.load(Ordering::Relaxed),
        )
    }

    #[test]
    fn categories_match_common_extensions() {
        assert_eq!(category_of("movie.MP4"), KIND_VIDEO);
        assert_eq!(category_of("photo.jpeg"), KIND_IMAGE);
        assert_eq!(category_of("song.flac"), KIND_AUDIO);
        assert_eq!(category_of("report.pdf"), KIND_DOCUMENT);
        assert_eq!(category_of("main.rs"), KIND_CODE);
        assert_eq!(category_of("app.exe"), KIND_EXECUTABLE);
        assert_eq!(category_of("font.ttf"), KIND_FONT);
        assert_eq!(category_of("no-extension"), KIND_OTHER);
        assert_eq!(category_of(".gitignore"), KIND_OTHER);
    }

    #[test]
    fn scan_aggregates_sizes_and_counts() {
        let base = scratch("sizes");
        fs::create_dir_all(base.join("sub/deep")).unwrap();
        fs::write(base.join("a.bin"), vec![0u8; 1024]).unwrap();
        fs::write(base.join("b.mp4"), vec![0u8; 100 * 1024]).unwrap();
        fs::write(base.join("sub/c.txt"), vec![0u8; 512]).unwrap();
        fs::write(base.join("sub/deep/d.rs"), vec![0u8; 256]).unwrap();

        let (raw, files, dirs, errors) = scan(&base);
        assert_eq!(files, 4);
        // root + sub + sub/deep
        assert_eq!(dirs, 3);
        assert_eq!(errors, 0);
        assert!(raw.is_dir);
        assert!(
            raw.size >= 1024 + 100 * 1024 + 512 + 256,
            "allocated total too small: {}",
            raw.size
        );
        assert_eq!(raw.dominant, KIND_VIDEO, "largest file is the mp4");

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn reports_allocated_size_not_logical_size() {
        let base = scratch("allocated");
        // 5000 bytes cannot stay resident in the MFT, so it must occupy whole
        // clusters; NTFS reports 8192 for it on a 4 KiB cluster volume.
        fs::write(base.join("spill.bin"), vec![0u8; 5000]).unwrap();

        let (raw, _, _, _) = scan(&base);
        assert!(
            raw.size > 5000,
            "allocated size must exceed the logical length, got {}",
            raw.size
        );
        assert!(
            raw.size <= 5000 + 64 * 1024,
            "allocated size looks wrong: {}",
            raw.size
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn hard_links_are_counted_once() {
        let base = scratch("hardlinks");
        let original = base.join("original.bin");
        fs::write(&original, vec![0u8; 256 * 1024]).unwrap();
        fs::hard_link(&original, base.join("mirror.bin")).unwrap();

        let (raw, files, _, _) = scan(&base);
        assert_eq!(files, 2, "both directory entries are still listed");
        assert!(raw.size >= 256 * 1024, "file was under-counted: {}", raw.size);
        assert!(
            raw.size <= 256 * 1024 + 4096,
            "hard link was counted twice: {}",
            raw.size
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn cancelling_stops_the_walk() {
        let base = scratch("cancel");
        fs::write(base.join("a.bin"), vec![0u8; 4096]).unwrap();

        let flag = Arc::new(AtomicBool::new(true));
        let progress = ScanProgress::new(flag);
        let name = base.to_string_lossy().into_owned().into_boxed_str();
        let raw = scan_dir(&base, name, &progress, 0);
        assert_eq!(raw.size, 0);

        let _ = fs::remove_dir_all(&base);
    }
}

