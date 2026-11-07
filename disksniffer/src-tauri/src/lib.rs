mod fsops;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use disksniffer_core::scan::{self, ScanProgress};
use disksniffer_core::tree::{
    BatchEntry, Candidate, ChildrenPage, NodeDetail, PathEntry, ScanTree, Summary,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

pub struct Session {
    pub tree: Option<Arc<ScanTree>>,
    pub cancel: Arc<AtomicBool>,
    pub scanning: bool,
}

impl Default for Session {
    fn default() -> Self {
        Session {
            tree: None,
            cancel: Arc::new(AtomicBool::new(false)),
            scanning: false,
        }
    }
}

#[derive(Default)]
pub struct AppState {
    pub session: Arc<Mutex<Session>>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ProgressPayload {
    files: u64,
    dirs: u64,
    bytes: u64,
    current: String,
}

fn lock_session<'a>(
    state: &'a State<'_, AppState>,
) -> Result<std::sync::MutexGuard<'a, Session>, String> {
    state
        .session
        .lock()
        .map_err(|_| "扫描状态不可用".to_string())
}

fn current_tree(state: &State<'_, AppState>) -> Result<Arc<ScanTree>, String> {
    let guard = lock_session(state)?;
    guard
        .tree
        .clone()
        .ok_or_else(|| "还没有可用的扫描结果".to_string())
}

#[tauri::command]
fn start_scan(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<(), String> {
    let root = scan::normalize_path(&path)?;
    if !root.is_dir() {
        return Err(format!("这不是一个文件夹：{}", root.display()));
    }

    let session = state.session.clone();
    let cancel = {
        let mut guard = session.lock().map_err(|_| "扫描状态不可用".to_string())?;
        if guard.scanning {
            return Err("已有扫描任务正在进行".to_string());
        }
        // Keep the previous result around so cancelling a new scan does not
        // throw away data the user already has.
        guard.scanning = true;
        guard.cancel = Arc::new(AtomicBool::new(false));
        guard.cancel.clone()
    };

    let app_handle = app.clone();
    let session_handle = session.clone();
    std::thread::spawn(move || {
        let progress = Arc::new(ScanProgress::new(cancel));

        let ticker = {
            let app = app_handle.clone();
            let progress = progress.clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_millis(140));
                let payload = ProgressPayload {
                    files: progress.files.load(Ordering::Relaxed),
                    dirs: progress.dirs.load(Ordering::Relaxed),
                    bytes: progress.bytes.load(Ordering::Relaxed),
                    current: progress
                        .current
                        .lock()
                        .map(|value| value.clone())
                        .unwrap_or_default(),
                };
                let _ = app.emit("scan:progress", payload);
                if progress.finished.load(Ordering::Relaxed) {
                    break;
                }
            })
        };

        let started = Instant::now();
        // A panic inside a worker must not leave the UI stuck in "scanning".
        let scanned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let root_name = root.to_string_lossy().into_owned().into_boxed_str();
            scan::scan_dir(&root, root_name, &progress, 0)
        }));
        let elapsed_ms = started.elapsed().as_millis() as u64;
        progress.finished.store(true, Ordering::Relaxed);
        let _ = ticker.join();

        let raw = match scanned {
            Ok(raw) => raw,
            Err(_) => {
                if let Ok(mut guard) = session_handle.lock() {
                    guard.scanning = false;
                }
                let _ = app_handle.emit("scan:failed", "扫描过程中出现内部错误，请重试");
                return;
            }
        };

        let cancelled = progress.cancel.load(Ordering::Relaxed);
        let errors = progress.errors.load(Ordering::Relaxed);
        drop(progress);

        if cancelled {
            if let Ok(mut guard) = session_handle.lock() {
                guard.scanning = false;
            }
            let _ = app_handle.emit("scan:cancelled", ());
            return;
        }

        let scan_tree = ScanTree::build(raw, root, errors, elapsed_ms);
        // The index now lives in a mapped file, so hand the pages the scan
        // touched back to the OS instead of holding them resident.
        release_working_set();
        let summary = scan_tree.summary();
        if let Ok(mut guard) = session_handle.lock() {
            guard.tree = Some(Arc::new(scan_tree));
            guard.scanning = false;
        }
        let _ = app_handle.emit("scan:done", summary);
    });

    Ok(())
}

#[tauri::command]
fn cancel_scan(state: State<'_, AppState>) -> Result<(), String> {
    let guard = lock_session(&state)?;
    guard.cancel.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
fn get_summary(state: State<'_, AppState>) -> Option<Summary> {
    let guard = state.session.lock().ok()?;
    guard.tree.as_ref().map(|tree| tree.summary())
}

#[tauri::command]
fn get_children(
    state: State<'_, AppState>,
    node_id: u32,
    limit: Option<usize>,
) -> Result<ChildrenPage, String> {
    let tree = current_tree(&state)?;
    tree.children_page(node_id, limit.unwrap_or(400).clamp(1, 4000))
}

#[tauri::command]
fn get_children_batch(
    state: State<'_, AppState>,
    ids: Vec<u32>,
    limit: Option<usize>,
) -> Result<Vec<BatchEntry>, String> {
    let tree = current_tree(&state)?;
    Ok(tree.children_batch(&ids, limit.unwrap_or(64).clamp(1, 1000)))
}

#[tauri::command]
fn get_node(state: State<'_, AppState>, node_id: u32) -> Result<NodeDetail, String> {
    let tree = current_tree(&state)?;
    tree.node_detail(node_id)
}

#[tauri::command]
fn get_chain(state: State<'_, AppState>, node_id: u32) -> Result<Vec<PathEntry>, String> {
    let tree = current_tree(&state)?;
    Ok(tree.chain(node_id))
}

#[tauri::command]
fn top_files(state: State<'_, AppState>, limit: Option<usize>) -> Result<Vec<Candidate>, String> {
    let tree = current_tree(&state)?;
    Ok(tree.top_files(limit.unwrap_or(20).clamp(1, 500)))
}

#[tauri::command]
fn search(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<Candidate>, String> {
    let tree = current_tree(&state)?;
    Ok(tree.search(&query, limit.unwrap_or(200).clamp(1, 2000)))
}

#[tauri::command]
fn trash_node(state: State<'_, AppState>, node_id: u32) -> Result<(), String> {
    let tree = current_tree(&state)?;
    tree.remove_subtree(node_id)
}

#[tauri::command]
fn reveal_path(path: String) -> Result<(), String> {
    fsops::reveal(&path)
}

#[tauri::command]
fn open_path(path: String) -> Result<(), String> {
    fsops::open_path(&path)
}

#[tauri::command]
fn list_drives() -> Vec<String> {
    scan::list_drives()
}

/// Asks Windows to trim this process' working set. Mapped file pages come back
/// on demand, so this only drops pages nobody is going to touch again soon.
#[cfg(target_os = "windows")]
fn release_working_set() {
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};
    unsafe {
        SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}

#[cfg(not(target_os = "windows"))]
fn release_working_set() {}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            start_scan,
            cancel_scan,
            get_summary,
            get_children,
            get_children_batch,
            get_node,
            get_chain,
            top_files,
            search,
            trash_node,
            reveal_path,
            open_path,
            list_drives
        ])
        .run(tauri::generate_context!())
        .expect("error while running DiskSniffer");
}
