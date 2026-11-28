//! One GHost Reforged at a time.
//!
//! The running launcher keeps `instance.json` (pid + whether it is hosting) in its data
//! folder and holds `launcher.lock`. A newly started launcher that finds the lock taken:
//! - previous launcher idle  → closes it and starts normally;
//! - previous launcher hosting → warns, offers to bring it to the front, and exits.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const FILE: &str = "instance.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub pid: u32,
    pub hosting: bool,
}

fn path(directory: &Path) -> PathBuf {
    directory.join(FILE)
}

/// Called by the running launcher at start and whenever hosting starts or stops.
pub fn publish(directory: &Path, hosting: bool) {
    let record = Record {
        pid: std::process::id(),
        hosting,
    };
    if let Ok(text) = serde_json::to_string(&record) {
        let temporary = directory.join("instance.json.tmp");
        if fs::write(&temporary, text).is_ok() {
            let _ = fs::rename(&temporary, path(directory));
        }
    }
}

pub fn read(directory: &Path) -> Option<Record> {
    serde_json::from_slice(&fs::read(path(directory)).ok()?).ok()
}

/// What a new launcher should do about an earlier one. Pure, for tests.
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    /// Nobody else holds the lock.
    Start,
    /// Another launcher is idle: close it, then start.
    Replace(u32),
    /// Another launcher is hosting: warn and exit.
    Warn(u32),
    /// The lock is held but nothing identifies the owner: warn without a pid.
    Unknown,
}

pub fn decide(locked: bool, record: Option<Record>, own_pid: u32, alive: impl Fn(u32) -> bool) -> Decision {
    if !locked {
        return Decision::Start;
    }
    match record {
        Some(r) if r.pid != own_pid && alive(r.pid) => {
            if r.hosting {
                Decision::Warn(r.pid)
            } else {
                Decision::Replace(r.pid)
            }
        }
        _ => Decision::Unknown,
    }
}

fn locked(directory: &Path) -> bool {
    use fs2::FileExt;
    let Ok(file) = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(directory.join("launcher.lock"))
    else {
        return false;
    };
    match FileExt::try_lock_exclusive(&file) {
        Ok(()) => {
            let _ = FileExt::unlock(&file);
            false
        }
        Err(_) => true,
    }
}

/// Runs before the window opens. Returns `false` when this launcher must exit.
pub fn ensure_single(directory: &Path) -> bool {
    if fs::create_dir_all(directory).is_err() {
        return true; // Storage will report the real problem.
    }
    let own = std::process::id();
    match decide(locked(directory), read(directory), own, native::is_launcher) {
        Decision::Start => true,
        Decision::Replace(pid) => {
            native::end(pid);
            // Wait for Windows to release the lock (the process handle closes it).
            let deadline = Instant::now() + Duration::from_secs(5);
            while locked(directory) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(100));
            }
            true
        }
        Decision::Warn(pid) => {
            if native::ask(
                "GHost Reforged is already running and hosting a lobby.\n\n\
                 Stop hosting in that window first if you want to restart it.\n\n\
                 Show the running GHost Reforged now?",
            ) {
                native::show(pid);
            }
            false
        }
        Decision::Unknown => {
            native::tell(
                "GHost Reforged is already running (it may be in the system tray).\n\n\
                 Close it from the tray icon (right-click → Quit) and try again.",
            );
            false
        }
    }
}

#[cfg(windows)]
mod native {
    use std::ptr::null_mut;
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HWND, LPARAM},
        System::Threading::{
            OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
            PROCESS_TERMINATE, QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject,
        },
        UI::WindowsAndMessaging::{
            EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IDYES, MB_ICONINFORMATION,
            MB_ICONWARNING, MB_OK, MB_SETFOREGROUND, MB_YESNO, MessageBoxW, SW_RESTORE, SW_SHOW,
            SetForegroundWindow, ShowWindow,
        },
    };

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }

    fn image_name(pid: u32) -> Option<String> {
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return None;
            }
            let mut buffer = [0u16; 1024];
            let mut length = buffer.len() as u32;
            let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buffer.as_mut_ptr(), &mut length);
            CloseHandle(process);
            (ok != 0).then(|| {
                String::from_utf16_lossy(&buffer[..length as usize])
                    .rsplit(['\\', '/'])
                    .next()
                    .unwrap_or_default()
                    .to_owned()
            })
        }
    }

    /// The pid is a running copy of this launcher (same exe name), not a reused pid.
    pub fn is_launcher(pid: u32) -> bool {
        let own = std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
        match (image_name(pid), own) {
            (Some(name), Some(own)) => name.eq_ignore_ascii_case(&own),
            _ => false,
        }
    }

    /// Ends an idle launcher. It has no hosting worker, so nothing is left half done;
    /// its game, if any, keeps running and the new launcher attaches to it.
    pub fn end(pid: u32) {
        if !is_launcher(pid) {
            return;
        }
        unsafe {
            let process = OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, 0, pid);
            if process.is_null() {
                return;
            }
            if TerminateProcess(process, 0) != 0 {
                WaitForSingleObject(process, 3000);
            }
            CloseHandle(process);
        }
    }

    pub fn ask(text: &str) -> bool {
        let (text, title) = (wide(text), wide("GHost Reforged"));
        unsafe {
            MessageBoxW(null_mut(), text.as_ptr(), title.as_ptr(), MB_YESNO | MB_ICONWARNING | MB_SETFOREGROUND)
                == IDYES
        }
    }

    pub fn tell(text: &str) {
        let (text, title) = (wide(text), wide("GHost Reforged"));
        unsafe {
            MessageBoxW(null_mut(), text.as_ptr(), title.as_ptr(), MB_OK | MB_ICONINFORMATION | MB_SETFOREGROUND);
        }
    }

    struct Search {
        pid: u32,
        found: Option<HWND>,
    }

    unsafe extern "system" fn collect(window: HWND, data: LPARAM) -> i32 {
        let search = unsafe { &mut *(data as *mut Search) };
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(window, &mut pid) };
        if pid != search.pid {
            return 1;
        }
        let mut title = [0u16; 64];
        let len = unsafe { GetWindowTextW(window, title.as_mut_ptr(), 64) }.max(0) as usize;
        if String::from_utf16_lossy(&title[..len]) == "GHost Reforged" {
            search.found = Some(window);
            return 0;
        }
        1
    }

    /// Brings the running launcher's main window forward, also out of the tray.
    pub fn show(pid: u32) {
        let mut search = Search { pid, found: None };
        unsafe { EnumWindows(Some(collect), &mut search as *mut Search as LPARAM) };
        if let Some(window) = search.found {
            unsafe {
                ShowWindow(window, SW_SHOW);
                ShowWindow(window, SW_RESTORE);
                SetForegroundWindow(window);
            }
        }
    }
}

#[cfg(not(windows))]
mod native {
    pub fn is_launcher(_: u32) -> bool {
        false
    }
    pub fn end(_: u32) {}
    pub fn ask(_: &str) -> bool {
        false
    }
    pub fn tell(text: &str) {
        eprintln!("{text}");
    }
    pub fn show(_: u32) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_idle_launcher_is_replaced_and_a_hosting_one_is_kept() {
        let idle = Some(Record { pid: 10, hosting: false });
        let hosting = Some(Record { pid: 10, hosting: true });
        let alive = |_| true;
        assert_eq!(decide(false, hosting, 1, alive), Decision::Start);
        assert_eq!(decide(true, idle, 1, alive), Decision::Replace(10));
        assert_eq!(decide(true, hosting, 1, alive), Decision::Warn(10));
        // Stale record (dead pid or our own) never kills anything.
        assert_eq!(decide(true, idle, 1, |_| false), Decision::Unknown);
        assert_eq!(decide(true, Some(Record { pid: 1, hosting: false }), 1, alive), Decision::Unknown);
        assert_eq!(decide(true, None, 1, alive), Decision::Unknown);
    }

    #[test]
    fn the_record_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        publish(dir.path(), true);
        assert_eq!(
            read(dir.path()),
            Some(Record { pid: std::process::id(), hosting: true })
        );
    }
}
