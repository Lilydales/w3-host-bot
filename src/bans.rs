//! A JSON ban list, written by the Ban button and safe to edit by hand.
//!
//! The file (`bans.json`) holds an array of `{ "player", "reason", "banned_at" }`
//! objects, where `banned_at` is a unix timestamp in seconds and `reason` may be empty.
//!
//! `Name#1234` bans exactly that BattleTag. A bare `Name` (no `#`) bans every
//! player whose BattleTag starts with that name. Matching ignores case. The file is
//! re-read whenever it changes, so edits take effect without restarting the host.
//!
//! An older plain-text `ban.txt` (one player per line) beside the JSON file is
//! imported automatically the first time the JSON file is empty or missing.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const FILE_NAME: &str = "bans.json";
pub const LEGACY_FILE_NAME: &str = "ban.txt";
pub const MAX_REASON: usize = 200;
const MAX_NAME: usize = 160;
const MIGRATED_REASON: &str = "Imported from ban.txt";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BanEntry {
    pub player: String,
    #[serde(default)]
    pub reason: String,
    /// Unix timestamp in seconds. Zero when unknown (hand-written entries).
    #[serde(default)]
    pub banned_at: u64,
}

#[derive(Default)]
pub struct BanList {
    path: Option<PathBuf>,
    entries: BTreeMap<String, BanEntry>,
    stamp: Option<(SystemTime, u64)>,
    warning: Option<String>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn short(name: &str) -> &str {
    name.split('#').next().unwrap_or(name)
}

fn usable(name: &str) -> bool {
    !name.is_empty() && name.len() <= MAX_NAME && !name.chars().any(char::is_control)
}

/// Single line, no control characters, capped length.
fn clean_reason(reason: &str) -> String {
    reason
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .chars()
        .take(MAX_REASON)
        .collect()
}

fn parse_json(text: &str) -> Result<Vec<BanEntry>, String> {
    let text = text.trim_start_matches('\u{feff}');
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str::<Vec<BanEntry>>(text).map_err(|e| e.to_string())
}

/// The old format: one name per line, `//` comments.
fn parse_legacy(text: &str) -> Vec<String> {
    text.trim_start_matches('\u{feff}')
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//") && line.len() <= MAX_NAME)
        .map(str::to_owned)
        .collect()
}

fn index(entries: Vec<BanEntry>) -> BTreeMap<String, BanEntry> {
    let mut map = BTreeMap::new();
    for mut entry in entries {
        entry.player = entry.player.trim().to_owned();
        if !usable(&entry.player) {
            continue;
        }
        entry.reason = clean_reason(&entry.reason);
        map.entry(entry.player.to_lowercase()).or_insert(entry);
    }
    map
}

fn stamp_of(path: &Path) -> Option<(SystemTime, u64)> {
    fs::metadata(path)
        .ok()
        .and_then(|meta| Some((meta.modified().ok()?, meta.len())))
}

fn save(path: &Path, entries: &BTreeMap<String, BanEntry>) -> Result<(), String> {
    let list: Vec<&BanEntry> = entries.values().collect();
    let mut text = serde_json::to_string_pretty(&list).map_err(|e| e.to_string())?;
    text.push('\n');
    // Write beside the target and rename, so a crash never leaves half a ban list.
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, text)
        .and_then(|()| fs::rename(&temporary, path))
        .map_err(|e| {
            let _ = fs::remove_file(&temporary);
            format!("Ban list could not be saved: {e}")
        })
}

impl BanList {
    /// Chooses `bans.json` beside the program when that folder is writable, otherwise
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
        if let Some(path) = list.path.clone() {
            list.prepare(&path);
        }
        list.reload();
        list
    }

    /// Creates the file, importing a legacy `ban.txt` when there is nothing in it yet.
    fn prepare(&mut self, path: &Path) {
        let blank = fs::read_to_string(path).map_or(true, |text| text.trim().is_empty());
        if !blank {
            return;
        }
        let legacy = path.with_file_name(LEGACY_FILE_NAME);
        let imported = fs::read(&legacy)
            .map(|bytes| parse_legacy(&String::from_utf8_lossy(&bytes)))
            .unwrap_or_default();
        let migrated = !imported.is_empty();
        let at = now();
        let entries = index(
            imported
                .into_iter()
                .map(|player| BanEntry {
                    player,
                    reason: MIGRATED_REASON.into(),
                    banned_at: at,
                })
                .collect(),
        );
        if migrated || !path.exists() {
            match save(path, &entries) {
                Ok(()) if migrated => {
                    // Keep the old file as a backup, out of the way of a second import.
                    let _ = fs::rename(&legacy, legacy.with_extension("txt.migrated"));
                }
                Ok(()) => {}
                Err(error) => {
                    self.warning = Some(if migrated {
                        format!("Old ban list could not be imported: {error}")
                    } else {
                        format!("Ban list could not be created: {error}")
                    });
                }
            }
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn warning(&mut self) -> Option<String> {
        self.warning.take()
    }

    /// The saved entry for exactly this name, as typed in the list.
    pub fn get(&self, name: &str) -> Option<&BanEntry> {
        self.entries.get(&name.trim().to_lowercase())
    }

    /// The entry that bans this player: the exact BattleTag first, then the bare name.
    pub fn matching(&self, name: &str) -> Option<&BanEntry> {
        let name = name.trim().to_lowercase();
        self.entries
            .get(&name)
            .or_else(|| self.entries.get(short(&name)))
    }

    pub fn is_banned(&self, name: &str) -> bool {
        let name = name.trim().to_lowercase();
        !name.is_empty() && (self.entries.contains_key(&name) || self.entries.contains_key(short(&name)))
    }

    /// Re-reads the file when it was edited. A read or parse failure keeps the previous list.
    pub fn reload(&mut self) {
        let Some(path) = &self.path else {
            return;
        };
        let stamp = stamp_of(path);
        if stamp == self.stamp && stamp.is_some() {
            return;
        }
        match fs::read(path) {
            Ok(bytes) => {
                self.stamp = stamp;
                match parse_json(&String::from_utf8_lossy(&bytes)) {
                    Ok(entries) => self.entries = index(entries),
                    Err(error) => {
                        self.warning = Some(format!(
                            "Ban list is not valid JSON, keeping the previous list: {error}"
                        ));
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // Deleting the file clears the list.
                self.entries.clear();
                self.stamp = None;
            }
            Err(error) => {
                self.warning = Some(format!("Ban list could not be read: {error}"));
            }
        }
    }

    /// Adds a player with an optional reason and saves immediately.
    /// Returns `Ok(false)` when already listed.
    pub fn add(&mut self, name: &str, reason: &str) -> Result<bool, String> {
        let name = name.trim();
        if !usable(name) || name.starts_with("//") {
            return Err("That player name cannot be banned.".into());
        }
        let path = self.path.clone().ok_or("The ban list location is unavailable.")?;
        // Start from what is on disk now, so a hand edit is never overwritten.
        let text = match fs::read(&path) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(format!("Ban list could not be read: {error}")),
        };
        let mut entries = index(parse_json(&text).map_err(|e| {
            format!("Ban list is not valid JSON, fix or delete {FILE_NAME}: {e}")
        })?);
        if entries.contains_key(&name.to_lowercase()) {
            self.entries = entries;
            return Ok(false);
        }
        entries.insert(
            name.to_lowercase(),
            BanEntry {
                player: name.to_owned(),
                reason: clean_reason(reason),
                banned_at: now(),
            },
        );
        save(&path, &entries)?;
        self.entries = entries;
        self.stamp = stamp_of(&path);
        Ok(true)
    }
}

#[cfg(test)]
#[path = "bans_tests.rs"]
mod tests;
