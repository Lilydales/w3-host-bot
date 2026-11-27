//! Launcher-side helpers for the game and Battle.net windows:
//!
//! - [`reclaim_focus_after_launch`]: Battle.net comes to the front when it starts the game
//!   (`--exec="launch W3"`), and the new game window may too. For a short window after a
//!   bot-requested launch, this brings the GHost Reforged window back. The W3Champions
//!   launcher does the same (it refocuses its own window after launch actions).
//! - [`exit_game`]: closes Warcraft III (asks politely, then ends the process).
//!
//! Nothing here reads game memory.
use std::time::Duration;

/// Battle.net's and the game's executable names.
pub const BATTLENET_EXE: &str = "Battle.net.exe";
pub const GAME_EXE: &str = "Warcraft III.exe";

/// Stop reclaiming this long after the game process first appears.
pub const AFTER_GAME_SEEN: Duration = Duration::from_secs(25);
/// Never reclaim focus more often than this per launch.
pub const MAX_RECLAIMS: u32 = 6;

/// What the reclaim watcher should do on one poll. Pure, for tests.
#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    Wait,
    Reclaim,
    Done,
}

/// Decide one poll of the reclaim watcher.
/// `foreground` is the exe name owning the foreground window, `game_seen_for` how long
/// the game process has existed (since the watcher saw it), `reclaims` how many times
/// focus was already taken back.
pub fn decide(foreground: Option<&str>, game_seen_for: Option<Duration>, reclaims: u32) -> Step {
    if reclaims >= MAX_RECLAIMS || game_seen_for.is_some_and(|d| d >= AFTER_GAME_SEEN) {
        return Step::Done;
    }
    match foreground {
        Some(name)
            if name.eq_ignore_ascii_case(BATTLENET_EXE) || name.eq_ignore_ascii_case(GAME_EXE) =>
        {
            Step::Reclaim
        }
        _ => Step::Wait,
    }
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::{
        mem::{size_of, zeroed},
        sync::atomic::{AtomicBool, Ordering},
        time::Instant,
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HWND, INVALID_HANDLE_VALUE, LPARAM},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
            Threading::{
                OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                PROCESS_TERMINATE, QueryFullProcessImageNameW, TerminateProcess,
            },
        },
        UI::{
            Input::KeyboardAndMouse::{INPUT, INPUT_KEYBOARD, KEYEVENTF_KEYUP, SendInput, VK_MENU},
            WindowsAndMessaging::{
                EnumWindows, GW_OWNER, GetForegroundWindow, GetWindow, GetWindowTextW,
                GetWindowThreadProcessId, IsIconic, IsWindowVisible, PostMessageW,
                SetForegroundWindow, WM_CLOSE,
            },
        },
    };

    /// Process ids whose executable name matches `exe` (case-insensitive).
    pub fn process_ids(exe: &str) -> Vec<u32> {
        let mut ids = Vec::new();
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return ids;
            }
            let mut entry: PROCESSENTRY32W = zeroed();
            entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
            let mut more = Process32FirstW(snapshot, &mut entry);
            while more != 0 {
                let end = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                if String::from_utf16_lossy(&entry.szExeFile[..end]).eq_ignore_ascii_case(exe) {
                    ids.push(entry.th32ProcessID);
                }
                more = Process32NextW(snapshot, &mut entry);
            }
            CloseHandle(snapshot);
        }
        ids
    }

    /// File name of the process that owns the foreground window.
    pub fn foreground_exe() -> Option<String> {
        unsafe {
            let window = GetForegroundWindow();
            if window.is_null() {
                return None;
            }
            let mut pid = 0;
            GetWindowThreadProcessId(window, &mut pid);
            if pid == 0 {
                return None;
            }
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return None;
            }
            let mut buffer = [0u16; 1024];
            let mut length = buffer.len() as u32;
            let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buffer.as_mut_ptr(), &mut length);
            CloseHandle(process);
            if ok == 0 {
                return None;
            }
            let path = String::from_utf16_lossy(&buffer[..length as usize]);
            path.rsplit(['\\', '/']).next().map(str::to_owned)
        }
    }

    struct Search {
        pid: u32,
        title: &'static str,
        found: Vec<HWND>,
        /// Only visible, not minimised windows.
        restored_only: bool,
    }

    unsafe extern "system" fn collect(window: HWND, data: LPARAM) -> i32 {
        let search = unsafe { &mut *(data as *mut Search) };
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(window, &mut pid) };
        if pid != search.pid {
            return 1;
        }
        // Top-level windows only (no owner); optionally visible and not minimised.
        if !unsafe { GetWindow(window, GW_OWNER) }.is_null() {
            return 1;
        }
        if search.restored_only {
            let usable = unsafe { IsWindowVisible(window) != 0 && IsIconic(window) == 0 };
            if !usable {
                return 1;
            }
        }
        let mut title = [0u16; 128];
        let len = unsafe { GetWindowTextW(window, title.as_mut_ptr(), 128) }.max(0) as usize;
        if String::from_utf16_lossy(&title[..len]).starts_with(search.title) {
            search.found.push(window);
        }
        1
    }

    fn windows_of(pid: u32, title: &'static str, restored_only: bool) -> Vec<HWND> {
        let mut search = Search {
            pid,
            title,
            found: Vec::new(),
            restored_only,
        };
        unsafe { EnumWindows(Some(collect), &mut search as *mut Search as LPARAM) };
        search.found
    }

    /// Brings a window forward. A tap of Alt first lets a background process take the
    /// foreground, which Windows otherwise refuses (same trick as `end_panel::focus`).
    fn bring_forward(window: HWND) -> bool {
        unsafe {
            if GetForegroundWindow() == window {
                return true;
            }
            let mut keys: [INPUT; 2] = zeroed();
            for (input, flags) in keys.iter_mut().zip([0, KEYEVENTF_KEYUP]) {
                input.r#type = INPUT_KEYBOARD;
                input.Anonymous.ki.wVk = VK_MENU;
                input.Anonymous.ki.dwFlags = flags;
            }
            SendInput(2, keys.as_ptr(), size_of::<INPUT>() as i32);
            SetForegroundWindow(window) != 0
        }
    }

    /// Watches for Battle.net / the new game window taking the foreground after a
    /// bot-requested launch and brings the GHost Reforged window back each time.
    /// Only acts while our window is visible and not minimised (hidden in the tray or
    /// minimised = the user does not want it in front). Returns how many times focus
    /// was taken back. Blocking: run on its own thread.
    pub fn reclaim_focus_after_launch(limit: Duration, cancel: &AtomicBool) -> u32 {
        let own = std::process::id();
        let started = Instant::now();
        let mut game_seen: Option<Instant> = None;
        let mut reclaims = 0;
        // The game was already running: Battle.net will not launch anything.
        if !process_ids(GAME_EXE).is_empty() {
            return 0;
        }
        while started.elapsed() < limit && !cancel.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(250));
            if game_seen.is_none() && !process_ids(GAME_EXE).is_empty() {
                game_seen = Some(Instant::now());
            }
            let foreground = foreground_exe();
            match decide(foreground.as_deref(), game_seen.map(|at| at.elapsed()), reclaims) {
                Step::Done => break,
                Step::Wait => {}
                Step::Reclaim => {
                    if let Some(window) = windows_of(own, "GHost Reforged", true).first().copied()
                        && bring_forward(window)
                    {
                        reclaims += 1;
                        // Let the launch settle before checking again.
                        std::thread::sleep(Duration::from_millis(750));
                    }
                }
            }
        }
        reclaims
    }

    /// Closes every Warcraft III process: posts WM_CLOSE to its windows, waits up to
    /// `grace`, then ends what is left. Returns a short line for the Activity log.
    pub fn exit_game(grace: Duration) -> Result<String, String> {
        let ids = process_ids(GAME_EXE);
        if ids.is_empty() {
            return Ok("Warcraft III is not running.".into());
        }
        for &pid in &ids {
            for window in windows_of(pid, "", false) {
                unsafe { PostMessageW(window, WM_CLOSE, 0, 0) };
            }
        }
        let deadline = Instant::now() + grace;
        while Instant::now() < deadline {
            if process_ids(GAME_EXE).is_empty() {
                return Ok("Warcraft III closed.".into());
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        let mut failed = 0;
        for pid in process_ids(GAME_EXE) {
            unsafe {
                let process = OpenProcess(PROCESS_TERMINATE, 0, pid);
                if process.is_null() || TerminateProcess(process, 0) == 0 {
                    failed += 1;
                }
                if !process.is_null() {
                    CloseHandle(process);
                }
            }
        }
        if failed > 0 {
            Err("Warcraft III could not be closed (try running GHost Reforged as the same user as the game).".into())
        } else {
            Ok("Warcraft III did not close by itself and was ended.".into())
        }
    }
}

#[cfg(windows)]
pub use native::{exit_game, foreground_exe, process_ids, reclaim_focus_after_launch};

#[cfg(not(windows))]
pub fn reclaim_focus_after_launch(_: Duration, _: &std::sync::atomic::AtomicBool) -> u32 {
    0
}

#[cfg(not(windows))]
pub fn exit_game(_: Duration) -> Result<String, String> {
    Err("Closing Warcraft III requires Windows.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_is_reclaimed_only_from_battlenet_or_the_new_game_window() {
        assert_eq!(decide(Some("Battle.net.exe"), None, 0), Step::Reclaim);
        assert_eq!(decide(Some("battle.net.exe"), None, 0), Step::Reclaim);
        assert_eq!(decide(Some("Warcraft III.exe"), Some(Duration::from_secs(3)), 1), Step::Reclaim);
        // Anything else the user switched to is left alone.
        assert_eq!(decide(Some("chrome.exe"), None, 0), Step::Wait);
        assert_eq!(decide(Some("ghost-reforged.exe"), None, 0), Step::Wait);
        assert_eq!(decide(None, None, 0), Step::Wait);
    }

    #[test]
    fn reclaiming_stops_after_the_launch_settles_or_the_limit() {
        assert_eq!(decide(Some("Battle.net.exe"), Some(AFTER_GAME_SEEN), 0), Step::Done);
        assert_eq!(decide(Some("Battle.net.exe"), None, MAX_RECLAIMS), Step::Done);
    }
}
