//! A plain-text ban list: one player per line, edited by hand or by the Ban button.
//!
//! `Name#1234` bans exactly that BattleTag. A bare `Name` (no `#`) bans every
//! player whose BattleTag starts with that name. Matching ignores case. Blank
//! lines and lines beginning with `//` are comments. The file is re-read
//! whenever it changes, so edits take effect without restarting the host.
use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::SystemTime,
};

pub const FILE_NAME: &str = "ban.txt";
const MAX_NAME: usize = 160;
const HEADER: &str = "// Banned players, one per line. Lines starting with // are ignored.\n\
// Name#1234 bans that exact BattleTag. A bare Name bans every BattleTag with that name.\n\
// Changes take effect while hosting; no restart is needed.\n";

#[derive(Default)]
pub struct BanList {
    path: Option<PathBuf>,
    names: BTreeSet<String>,
    stamp: Option<(SystemTime, u64)>,
    warning: Option<String>,
}

fn parse(text: &str) -> BTreeSet<String> {
    text.trim_start_matches('\u{feff}')
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//") && line.len() <= MAX_NAME)
        .map(str::to_lowercase)
        .collect()
}

fn short(name: &str) -> &str {
    name.split('#').next().unwrap_or(name)
}

impl BanList {
    /// Chooses `ban.txt` beside the program when that folder is writable, otherwise
    /// inside the data folder, so an install under a protected location still works.
    pub fn locate(preferred: Option<&Path>, fallback: &Path) -> PathBuf {
        if let Some(directory) = preferred {
            let candidate = directory.join(FILE_NAME);
            if OpenOptions::new()
                .create(true)
                .append(true)
                .open(&candidate)
                .is_ok()
            {
                return candidate;
            }
        }
        fallback.join(FILE_NAME)
    }

    pub fn open(path: Option<&Path>) -> Self {
        let mut list = Self {
            path: path.map(Path::to_path_buf),
            ..Self::default()
        };
        if let Some(path) = &list.path
            && !path.exists()
            && let Err(error) = fs::write(path, HEADER)
        {
            list.warning = Some(format!("Ban list could not be created: {error}"));
        }
        list.reload();
        list
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    pub fn warning(&mut self) -> Option<String> {
        self.warning.take()
    }

    pub fn is_banned(&self, name: &str) -> bool {
        let name = name.trim().to_lowercase();
        !name.is_empty() && (self.names.contains(&name) || self.names.contains(short(&name)))
    }

    /// Re-reads the file when it was edited. A read failure keeps the previous list.
    pub fn reload(&mut self) {
        let Some(path) = &self.path else {
            return;
        };
        let stamp = fs::metadata(path)
            .ok()
            .and_then(|meta| Some((meta.modified().ok()?, meta.len())));
        if stamp == self.stamp && stamp.is_some() {
            return;
        }
        match fs::read(path) {
            Ok(bytes) => {
                self.names = parse(&String::from_utf8_lossy(&bytes));
                self.stamp = stamp;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // Deleting the file clears the list.
                self.names.clear();
                self.stamp = None;
            }
            Err(error) => {
                self.warning = Some(format!("Ban list could not be read: {error}"));
            }
        }
    }

    /// Adds a player and saves immediately. Returns `Ok(false)` when already listed.
    pub fn add(&mut self, name: &str) -> Result<bool, String> {
        let name = name.trim();
        if name.is_empty() || name.len() > MAX_NAME || name.chars().any(char::is_control) {
            return Err("That player name cannot be banned.".into());
        }
        if name.starts_with("//") {
            return Err("That player name cannot be banned.".into());
        }
        self.reload();
        if self.names.contains(&name.to_lowercase()) {
            return Ok(false);
        }
        let path = self.path.clone().ok_or("The ban list location is unavailable.")?;
        let existing = fs::read_to_string(&path).unwrap_or_default();
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| format!("Ban list could not be saved: {e}"))?;
        let mut line = String::new();
        if existing.is_empty() {
            line.push_str(HEADER);
        } else if !existing.ends_with('\n') {
            line.push('\n');
        }
        line.push_str(name);
        line.push('\n');
        file.write_all(line.as_bytes())
            .and_then(|()| file.flush())
            .map_err(|e| format!("Ban list could not be saved: {e}"))?;
        drop(file);
        self.names.insert(name.to_lowercase());
        self.stamp = fs::metadata(&path)
            .ok()
            .and_then(|meta| Some((meta.modified().ok()?, meta.len())));
        Ok(true)
    }
}

#[cfg(test)]
#[path = "bans_tests.rs"]
mod tests;
