//! Battle.net desktop app: where it is installed, whether it runs, and whether it is
//! signed in.
//!
//! Sign-in comes from Battle.net's own log (`battle.net-<time>.log` in
//! `%LOCALAPPDATA%\Battle.net\Logs`), the same signal the W3Champions launcher watches.
//! Checked against a real log: a sign-in writes
//! `[BNLogin] ... Logged into Battle.net successfully.`, a dropped connection writes
//! `[BNLogin] ... Disconnected from Battle.net: ...` and the reconnect ends with
//! `[AccountManager] ... Account manager state has changed to: Ready`.
//! Nothing here reads credentials or tokens.
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Battle.net.exe is not running.
    NotRunning,
    /// Running, but its log shows no sign-in (login screen, or signed out).
    SignedOut,
    /// Running and signed in.
    SignedIn,
    /// Running, but its log could not be found or read.
    Unknown,
}

impl State {
    pub fn label(self) -> &'static str {
        match self {
            Self::NotRunning => "Battle.net: not running",
            Self::SignedOut => "Battle.net: not signed in",
            Self::SignedIn => "Battle.net: signed in",
            Self::Unknown => "Battle.net: running",
        }
    }
}

/// The last sign-in signal in a Battle.net log, or `None` when it has none yet.
pub fn signed_in(log: &str) -> Option<bool> {
    let mut state = None;
    for line in log.lines() {
        if line.contains("Logged into Battle.net successfully")
            || line.contains("Account manager state has changed to: Ready")
        {
            state = Some(true);
        } else if line.contains("Disconnected from Battle.net")
            || line.contains("Account manager state has changed to: Incomplete")
            || line.contains("Logged out of Battle.net")
        {
            state = Some(false);
        }
    }
    state
}

/// Folders Battle.net writes its client log to.
fn log_folders() -> Vec<PathBuf> {
    ["LOCALAPPDATA", "PROGRAMDATA"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(|root| PathBuf::from(root).join("Battle.net").join("Logs"))
        .collect()
}

/// The newest `battle.net-*.log`.
pub fn newest_log() -> Option<PathBuf> {
    log_folders()
        .into_iter()
        .filter_map(|folder| std::fs::read_dir(folder).ok())
        .flat_map(|entries| entries.flatten())
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            name.starts_with("battle.net-") && name.ends_with(".log")
        })
        .filter_map(|entry| Some((entry.metadata().ok()?.modified().ok()?, entry.path())))
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

/// The last `limit` bytes of a file (logs grow to several MB).
fn tail(path: &Path, limit: u64) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(length.saturating_sub(limit))).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// Current state. Blocking file and process reads: call off the UI thread.
pub fn state() -> State {
    if running_launcher().is_none() {
        return State::NotRunning;
    }
    // Sign-in is written near the start of a session; reconnects later. Read enough of
    // the tail to cover a long session, and the whole file when it is small.
    match newest_log().and_then(|log| tail(&log, 4 * 1024 * 1024)) {
        Some(text) => match signed_in(&text) {
            Some(true) => State::SignedIn,
            Some(false) => State::SignedOut,
            None => State::SignedOut,
        },
        None => State::Unknown,
    }
}

/// Battle.net.exe: the running one, else the usual install folders on every drive
/// (it is often installed outside `C:\Program Files (x86)`).
pub fn find_launcher() -> Option<PathBuf> {
    if let Some(path) = running_launcher() {
        return Some(path);
    }
    candidates(
        std::env::var_os("ProgramFiles(x86)").map(PathBuf::from),
        std::env::var_os("ProgramFiles").map(PathBuf::from),
        "Battle.net/Battle.net.exe",
    )
    .into_iter()
    .find(|path| path.is_file())
}

/// Warcraft III.exe in the usual install folders on every drive.
pub fn find_game() -> Option<PathBuf> {
    candidates(
        std::env::var_os("ProgramFiles(x86)").map(PathBuf::from),
        std::env::var_os("ProgramFiles").map(PathBuf::from),
        "Warcraft III/_retail_/x86_64/Warcraft III.exe",
    )
    .into_iter()
    .find(|path| path.is_file())
}

/// Install locations to try, most likely first, without duplicates.
pub fn candidates(x86: Option<PathBuf>, programs: Option<PathBuf>, relative: &str) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = x86.into_iter().chain(programs).collect();
    for drive in b'C'..=b'Z' {
        let drive = format!("{}:\\", drive as char);
        roots.push(PathBuf::from(&drive).join("Program Files (x86)"));
        roots.push(PathBuf::from(&drive).join("Program Files"));
        roots.push(PathBuf::from(&drive));
    }
    let mut seen = Vec::new();
    for root in roots {
        let path = root.join(relative);
        if !seen.contains(&path) {
            seen.push(path);
        }
    }
    seen
}

#[cfg(windows)]
pub fn running_launcher() -> Option<PathBuf> {
    use std::mem::{size_of, zeroed};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                TH32CS_SNAPPROCESS,
            },
            Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW},
        },
    };
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut entry: PROCESSENTRY32W = zeroed();
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut found = None;
        let mut more = Process32FirstW(snapshot, &mut entry);
        while more != 0 && found.is_none() {
            let end = entry
                .szExeFile
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(entry.szExeFile.len());
            if String::from_utf16_lossy(&entry.szExeFile[..end]).eq_ignore_ascii_case("Battle.net.exe") {
                let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, entry.th32ProcessID);
                if !process.is_null() {
                    let mut path = [0u16; 32768];
                    let mut size = path.len() as u32;
                    if QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut size) != 0 {
                        found = Some(PathBuf::from(String::from_utf16_lossy(&path[..size as usize])));
                    }
                    CloseHandle(process);
                }
            }
            more = Process32NextW(snapshot, &mut entry);
        }
        CloseHandle(snapshot);
        found
    }
}

#[cfg(not(windows))]
pub fn running_launcher() -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIGNED_IN: &str = "\
I 2026-11-21 23:44:36.492120 [AccountManager] {Main} Account manager state has changed to: Ready
I 2026-11-21 23:44:36.628562 [BNLogin] {Main} Logged into Battle.net successfully. |bnet=1:0:3a124b9";

    #[test]
    fn the_last_sign_in_signal_in_the_log_wins() {
        assert_eq!(signed_in("I ... [Main] starting up"), None);
        assert_eq!(signed_in(SIGNED_IN), Some(true));
        let dropped = format!(
            "{SIGNED_IN}\nE 2026-11-26 10:32:34.215386 [BNLogin] {{Main}} Disconnected from Battle.net: err=ERROR_NETWORK_MODULE_SOCKET_CLOSED (1016)"
        );
        assert_eq!(signed_in(&dropped), Some(false));
        let back = format!(
            "{dropped}\nI 2026-11-26 10:32:40.867901 [AccountManager] {{Main}} Account manager state has changed to: Ready"
        );
        assert_eq!(signed_in(&back), Some(true));
    }

    #[test]
    fn install_folders_on_other_drives_are_searched() {
        let found = candidates(
            Some(PathBuf::from(r"C:\Program Files (x86)")),
            None,
            "Battle.net/Battle.net.exe",
        );
        assert_eq!(found[0], PathBuf::from(r"C:\Program Files (x86)").join("Battle.net/Battle.net.exe"));
        assert!(found.contains(&PathBuf::from(r"D:\Program Files (x86)").join("Battle.net/Battle.net.exe")));
        assert_eq!(
            found.iter().filter(|p| p.starts_with(r"C:\Program Files (x86)")).count(),
            1,
            "no duplicates"
        );
    }
}
