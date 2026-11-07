use std::path::Path;

#[cfg(not(target_os = "windows"))]
use std::process::Command;

/// Windows opens folders through the shell instead of spawning explorer.exe, so
/// a third party file manager registered as the default handler takes over.
///
/// shell32's SHOpenFolderAndSelectItems is the entry point Explorer itself uses
/// for "show in folder" (and what browsers and editors call). explorer.exe
/// /select bypasses the shell entirely and therefore always lands in Explorer.
#[cfg(target_os = "windows")]
mod shell {
    use std::ffi::{OsStr, OsString};
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows_sys::Win32::System::Com::{
        CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED,
    };
    use windows_sys::Win32::UI::Shell::Common::ITEMIDLIST;
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteW, ILCreateFromPathW, ILFree, SHOpenFolderAndSelectItems,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    fn wide(text: &OsStr) -> Vec<u16> {
        text.encode_wide().chain(std::iter::once(0)).collect()
    }

    /// Keeps one COM reference for the duration of a shell call. RPC_E_CHANGED_MODE
    /// means the thread already joined another apartment, which is fine to ignore
    /// but must not be balanced with CoUninitialize.
    struct ComGuard {
        release: bool,
    }

    impl ComGuard {
        fn new() -> Self {
            let hr = unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
            ComGuard { release: hr >= 0 }
        }
    }

    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.release {
                unsafe { CoUninitialize() };
            }
        }
    }

    struct Pidl(*const ITEMIDLIST);

    impl Pidl {
        fn of(path: &Path) -> Option<Self> {
            let wide = wide(path.as_os_str());
            let raw = unsafe { ILCreateFromPathW(wide.as_ptr()) };
            if raw.is_null() {
                None
            } else {
                Some(Pidl(raw))
            }
        }

        fn raw(&self) -> *const ITEMIDLIST {
            self.0
        }
    }

    impl Drop for Pidl {
        fn drop(&mut self) {
            unsafe { ILFree(self.0) };
        }
    }

    /// Opens a folder through whatever handler is registered for "open".
    fn shell_open(path: &Path) -> Result<(), String> {
        let verb = wide(OsStr::new("open"));
        let target = wide(path.as_os_str());
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                target.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        // ShellExecuteW reports failure as a pseudo handle with a value of 32 or less.
        if (result as isize) <= 32 {
            return Err(format!("无法打开文件夹：{}", path.display()));
        }
        Ok(())
    }

    /// Last resort, keeps working even when the shell refuses the request.
    fn explorer_select(path: &Path) -> Result<(), String> {
        let mut selector = OsString::from("/select,");
        selector.push(path);
        std::process::Command::new("explorer")
            .arg(selector)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("无法在文件管理器中定位：{}", error))
    }

    pub fn reveal(path: &Path) -> Result<(), String> {
        let parent = match path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent,
            _ => return open_folder(path),
        };
        let _com = ComGuard::new();
        if let (Some(folder), Some(item)) = (Pidl::of(parent), Pidl::of(path)) {
            let items = [item.raw()];
            let hr = unsafe { SHOpenFolderAndSelectItems(folder.raw(), 1, items.as_ptr(), 0) };
            if hr >= 0 {
                return Ok(());
            }
        }
        if shell_open(parent).is_ok() {
            return Ok(());
        }
        explorer_select(path)
    }

    pub fn open_folder(path: &Path) -> Result<(), String> {
        let _com = ComGuard::new();
        if let Some(folder) = Pidl::of(path) {
            let hr = unsafe { SHOpenFolderAndSelectItems(folder.raw(), 0, std::ptr::null(), 0) };
            if hr >= 0 {
                return Ok(());
            }
        }
        if shell_open(path).is_ok() {
            return Ok(());
        }
        Err(format!("无法打开文件夹：{}", path.display()))
    }
}

#[cfg(target_os = "windows")]
pub fn reveal(path: &str) -> Result<(), String> {
    shell::reveal(Path::new(path))
}

#[cfg(target_os = "windows")]
pub fn open_dir(path: &str) -> Result<(), String> {
    shell::open_folder(Path::new(path))
}

#[cfg(target_os = "macos")]
pub fn reveal(path: &str) -> Result<(), String> {
    Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法在访达中定位：{}", error))
}

#[cfg(target_os = "macos")]
pub fn open_dir(path: &str) -> Result<(), String> {
    Command::new("open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开文件夹：{}", error))
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn reveal(path: &str) -> Result<(), String> {
    let parent = Path::new(path).parent().unwrap_or_else(|| Path::new("/"));
    open_dir(&parent.to_string_lossy())
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn open_dir(path: &str) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开文件夹：{}", error))
}

pub fn open_path(path: &str) -> Result<(), String> {
    if Path::new(path).is_dir() {
        open_dir(path)
    } else {
        reveal(path)
    }
}
