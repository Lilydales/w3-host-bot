//! After each Play-as-host match: read `LastReplay.w3g`, list who left early and apply
//! the early-leave ban rule.
//!
//! The replay folder is found automatically (`Documents/Warcraft III/BattleNet/<n>/Replay`,
//! newest first). If it cannot be found the Hosting page asks for it, with a folder
//! picker; the choice is saved in `replay-folder.txt` in the data folder.
use super::{
    app::{App, Message},
    ban_list::EARLY_END_SECONDS,
    profile::EarlyLeaveRule,
};
use ghost_reforged::replay::{self, FILE_NAME, Summary};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::{Duration, SystemTime},
};

/// Players recorded as leaving within this long of the end left with everyone else.
pub const END_GRACE_MS: u64 = 3_000;
const SAVED_FOLDER: &str = "replay-folder.txt";

#[derive(Debug, Clone, PartialEq)]
pub enum ReplayError {
    /// No replay folder could be found, or the replay was not written for this match.
    Missing(String),
    Unreadable(String),
}

/// Newest `LastReplay.w3g` under the standard Warcraft III folders.
pub fn auto_find() -> Option<PathBuf> {
    let root = super::maps::documents_directory()?.join("Warcraft III");
    let mut candidates = vec![root.join("Replay").join(FILE_NAME)];
    if let Ok(accounts) = std::fs::read_dir(root.join("BattleNet")) {
        for account in accounts.flatten() {
            candidates.push(account.path().join("Replay").join(FILE_NAME));
        }
    }
    candidates
        .into_iter()
        .filter_map(|path| Some((std::fs::metadata(&path).ok()?.modified().ok()?, path)))
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

pub fn load_folder(data: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(data.join(SAVED_FOLDER)).ok()?;
    let folder = PathBuf::from(text.trim());
    (!text.trim().is_empty()).then_some(folder)
}

fn save_folder(data: &Path, folder: Option<&Path>) -> Result<(), String> {
    let file = data.join(SAVED_FOLDER);
    match folder {
        Some(folder) => std::fs::write(file, folder.to_string_lossy().as_bytes()),
        None => match std::fs::remove_file(file) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        },
    }
    .map_err(|e| format!("The replay folder could not be saved: {e}"))
}

/// When a leave counts as early, in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    /// Leaving before this is always early.
    pub limit_ms: u64,
    /// Leaving within this of the end is not early (unless before `limit_ms`).
    pub buffer_ms: u64,
}

impl Window {
    pub fn from_minutes(limit: u32, buffer: u32) -> Self {
        Self {
            limit_ms: u64::from(limit) * 60_000,
            buffer_ms: u64::from(buffer) * 60_000,
        }
    }

    /// Whether a leave at `at_ms` in a match of `length_ms` counts as leaving early.
    /// Everyone is recorded leaving at the very end; those never count.
    pub fn is_early(self, at_ms: u64, length_ms: u64) -> bool {
        at_ms + END_GRACE_MS < length_ms
            && (at_ms < self.limit_ms || at_ms + self.buffer_ms < length_ms)
    }
}

/// Leaves that count as early under `window`, in order.
pub fn early<'a>(summary: &'a Summary, window: Window) -> Vec<&'a replay::Leave> {
    summary
        .leaves
        .iter()
        .filter(|leave| window.is_early(leave.at_ms, summary.length_ms))
        .collect()
}

/// Leaves in the end buffer: not early, but before everyone left together.
pub fn near_end<'a>(summary: &'a Summary, window: Window) -> Vec<&'a replay::Leave> {
    summary
        .leaves
        .iter()
        .filter(|leave| {
            leave.at_ms + END_GRACE_MS < summary.length_ms
                && !window.is_early(leave.at_ms, summary.length_ms)
        })
        .collect()
}

/// Who the rule bans: (player, leave time in ms). Never the host, never a name without a
/// BattleTag (observers such as FLO), never someone who left with everyone at the end or
/// inside the end buffer.
pub fn to_ban(
    summary: &Summary,
    rule: EarlyLeaveRule,
    window: Window,
    own: Option<&str>,
) -> Vec<(String, u64)> {
    let limit = match rule {
        EarlyLeaveRule::Off => return Vec::new(),
        EarlyLeaveRule::BeforeSevenMinutes => window.limit_ms,
        EarlyLeaveRule::AnyEarlyLeave => u64::MAX,
    };
    early(summary, window)
        .into_iter()
        .filter(|leave| leave.at_ms < limit)
        .filter(|leave| leave.name.contains('#'))
        .filter(|leave| own.is_none_or(|own| !own.eq_ignore_ascii_case(&leave.name)))
        .map(|leave| (leave.name.clone(), leave.at_ms))
        .collect()
}

/// Waits for this match's replay, then reads it. Runs on its own thread.
fn wait_and_read(folder: Option<PathBuf>, since: Option<SystemTime>) -> Result<Summary, ReplayError> {
    thread::sleep(Duration::from_secs(2));
    let fresh = |path: &Path| {
        let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        match (modified, since) {
            (Some(modified), Some(since)) => modified >= since,
            // Launcher opened mid-match: accept a replay written in the last two minutes.
            (Some(modified), None) => modified
                .elapsed()
                .is_ok_and(|age| age < Duration::from_secs(120)),
            (None, _) => false,
        }
    };
    let mut seen = None;
    for _ in 0..20 {
        let path = folder
            .as_ref()
            .map(|folder| folder.join(FILE_NAME))
            .filter(|path| path.is_file())
            .or_else(auto_find);
        if let Some(path) = &path
            && fresh(path)
        {
            // The game may still be writing; a second read covers a half-written file.
            return replay::read(path)
                .or_else(|_| {
                    thread::sleep(Duration::from_secs(2));
                    replay::read(path)
                })
                .map_err(ReplayError::Unreadable);
        }
        seen = path.or(seen);
        thread::sleep(Duration::from_secs(1));
    }
    Err(ReplayError::Missing(match seen {
        Some(path) => format!("{} was not updated by this match.", path.display()),
        None => format!("{FILE_NAME} was not found."),
    }))
}

#[cfg(windows)]
unsafe extern "C" {
    fn ghost_pick_folder(start: *const u16, output: *mut u16, capacity: u32) -> i32;
}

#[cfg(windows)]
fn native_pick(start: Option<PathBuf>) -> Result<Option<PathBuf>, String> {
    let start: Vec<u16> = start
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let mut output = vec![0u16; 32768];
    match unsafe { ghost_pick_folder(start.as_ptr(), output.as_mut_ptr(), output.len() as u32) } {
        0 => {
            let length = output.iter().position(|unit| *unit == 0).unwrap_or(0);
            Ok(Some(PathBuf::from(String::from_utf16_lossy(&output[..length]))))
        }
        1 => Ok(None),
        code => Err(format!("The folder picker could not open ({code:#x}).")),
    }
}

#[cfg(not(windows))]
fn native_pick(_: Option<PathBuf>) -> Result<Option<PathBuf>, String> {
    Err("Choosing a folder requires Windows.".into())
}

pub async fn pick(start: Option<PathBuf>) -> Result<Option<PathBuf>, String> {
    let (sender, receiver) = iced::futures::channel::oneshot::channel();
    thread::Builder::new()
        .name("ghost-replay-picker".into())
        .spawn(move || {
            let _ = sender.send(native_pick(start));
        })
        .map_err(|_| "The folder picker could not open.")?;
    receiver.await.map_err(|_| "The folder picker stopped.")?
}

impl App {
    /// The folder shown in Options. Never touches the disk: it is called while drawing.
    pub(super) fn replay_folder_label(&self) -> String {
        match &self.replay_folder {
            Some(folder) => folder.display().to_string(),
            None => "found automatically (Documents\\Warcraft III\\BattleNet\\…\\Replay)".into(),
        }
    }

    /// Starts reading the replay of the match that just ended (Play-as-host only).
    pub(super) fn read_replay(&mut self) {
        if !self.form.profile.player_host {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        let (folder, since) = (self.replay_folder.clone(), self.game_started_wall);
        if thread::Builder::new()
            .name("ghost-replay".into())
            .spawn(move || {
                let _ = sender.send(wait_and_read(folder, since));
            })
            .is_ok()
        {
            self.replay_job = Some(receiver);
        }
    }

    pub(super) fn poll_replay(&mut self) {
        let Some(job) = &self.replay_job else {
            return;
        };
        let result = match job.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err(ReplayError::Unreadable(
                "Reading the replay stopped unexpectedly.".into(),
            )),
        };
        self.replay_job = None;
        self.replay_read(result);
    }

    pub(super) fn early_window(&self) -> Window {
        Window::from_minutes(
            self.form.profile.early_leave_minutes,
            self.form.profile.end_buffer_minutes,
        )
    }

    pub(super) fn replay_read(&mut self, result: Result<Summary, ReplayError>) {
        let summary = match result {
            Ok(summary) => summary,
            Err(ReplayError::Missing(reason)) => {
                self.replay_missing = true;
                self.log(&format!(
                    "Replay · {reason} Choose the Replay folder to detect early leavers."
                ));
                return;
            }
            Err(ReplayError::Unreadable(reason)) => {
                self.log(&format!("Replay · {reason}"));
                return;
            }
        };
        self.replay_missing = false;
        let window = self.early_window();
        let leavers = early(&summary, window);
        let mut line = format!("Replay · match {}", replay::clock(summary.length_ms));
        if leavers.is_empty() {
            line.push_str(" · nobody left early");
        }
        for leave in &leavers {
            line.push_str(&format!(" · {} left at {}", leave.name, replay::clock(leave.at_ms)));
        }
        for leave in near_end(&summary, window) {
            line.push_str(&format!(
                " · {} left at {} (end buffer, not counted)",
                leave.name,
                replay::clock(leave.at_ms)
            ));
        }
        self.log(&line);
        let own = self.own_name.clone();
        let bans = to_ban(&summary, self.form.profile.early_leave_rule, window, own.as_deref());
        let mut banned = Vec::new();
        for (name, at) in bans {
            let reason = format!("Left the game early ({})", replay::clock(at));
            match self.ban_name_with(&name, &reason) {
                Ok(()) => banned.push(name),
                Err(error) => self.log(&format!("Replay · {name} could not be banned: {error}")),
            }
        }
        if let Some(game) = &mut self.last_game {
            // The replay knows the real length; the launcher's own timer is only a fallback.
            let first = leavers.first().map(|leave| leave.at_ms);
            game.ended_early = first
                .filter(|at| *at < EARLY_END_SECONDS * 1000)
                .map(|_| summary.length_ms / 1000);
            game.auto_banned = banned;
            game.replay = Some(summary);
        }
    }

    pub(super) fn replay_message(&mut self, message: &Message) -> Option<iced::Task<Message>> {
        match message {
            Message::PickReplayFolder => {
                let start = self
                    .replay_folder
                    .clone()
                    .or_else(|| auto_find().and_then(|p| p.parent().map(Path::to_path_buf)))
                    .or_else(|| {
                        super::maps::documents_directory().map(|d| d.join("Warcraft III"))
                    });
                Some(iced::Task::perform(pick(start), Message::ReplayFolderPicked))
            }
            Message::ReplayFolderPicked(result) => {
                match result {
                    Ok(Some(folder)) => {
                        let result = save_folder(&self.data_directory, Some(folder.as_path()));
                        if result.is_ok() {
                            self.replay_folder = Some(folder.clone());
                            self.log(&format!("Replay folder · {}", folder.display()));
                            if !folder.join(FILE_NAME).is_file() {
                                self.log(&format!(
                                    "Replay folder · no {FILE_NAME} there yet; it appears after a match."
                                ));
                            }
                        }
                        self.notify(result, "Replay folder saved.");
                    }
                    Ok(None) => {}
                    Err(error) => self.notify(Err(error.clone()), ""),
                }
                Some(iced::Task::none())
            }
            Message::AutoReplayFolder => {
                let result = save_folder(&self.data_directory, None);
                if result.is_ok() {
                    self.replay_folder = None;
                }
                self.notify(result, "Replay folder is found automatically.");
                Some(iced::Task::none())
            }
            Message::EarlyLeaveRule(rule) => {
                self.form.profile.early_leave_rule = *rule;
                Some(iced::Task::none())
            }
            Message::EarlyLeaveMinutes(minutes) => {
                self.form.profile.early_leave_minutes =
                    (*minutes).clamp(1, super::profile::MAX_EARLY_LEAVE_MINUTES);
                Some(iced::Task::none())
            }
            Message::EndBufferMinutes(minutes) => {
                self.form.profile.end_buffer_minutes =
                    (*minutes).min(super::profile::MAX_END_BUFFER_MINUTES);
                Some(iced::Task::none())
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghost_reforged::replay::Leave;

    fn summary() -> Summary {
        let leave = |name: &str, minutes: u64, seconds: u64| Leave {
            name: name.into(),
            at_ms: (minutes * 60 + seconds) * 1000,
        };
        Summary {
            players: vec!["Me#1".into(), "Early#2".into(), "Late#3".into(), "End#4".into()],
            leaves: vec![
                leave("Early#2", 4, 10),
                leave("Me#1", 5, 0),
                leave("Late#3", 23, 56),
                leave("End#4", 28, 1),
            ],
            length_ms: (28 * 60 + 1) * 1000,
        }
    }

    #[test]
    fn the_rule_picks_who_to_ban_and_never_the_host_or_end_of_match_leavers() {
        // 8-minute limit, no end buffer.
        let window = Window::from_minutes(8, 0);
        let names = |rule| -> Vec<String> {
            to_ban(&summary(), rule, window, Some("me#1")).into_iter().map(|b| b.0).collect()
        };
        assert_eq!(names(EarlyLeaveRule::BeforeSevenMinutes), ["Early#2"]);
        assert_eq!(names(EarlyLeaveRule::AnyEarlyLeave), ["Early#2", "Late#3"]);
        assert!(names(EarlyLeaveRule::Off).is_empty());
        let mut observer = summary();
        observer.leaves[0].name = "FLO".into();
        assert!(to_ban(&observer, EarlyLeaveRule::BeforeSevenMinutes, window, None).is_empty());
    }

    #[test]
    fn the_end_buffer_forgives_late_leavers_but_never_leaves_before_the_limit() {
        // Late#3 left at 23:56 of 28:01, 4:05 before the end.
        let late = |buffer| {
            to_ban(&summary(), EarlyLeaveRule::AnyEarlyLeave, Window::from_minutes(8, buffer), None)
                .into_iter()
                .map(|b| b.0)
                .collect::<Vec<_>>()
        };
        assert!(late(4).contains(&"Late#3".to_string()), "4:05 before the end is outside 4 min");
        assert!(!late(5).contains(&"Late#3".to_string()), "inside a 5 min buffer");
        assert_eq!(near_end(&summary(), Window::from_minutes(8, 5)).len(), 1);
        // A short match (someone left at 3:00, the map ended it at 3:10): the buffer
        // covers the whole match, but leaving before 8 minutes is still early.
        let short = Summary {
            players: vec!["Q#1".into(), "S#2".into()],
            leaves: vec![
                Leave { name: "Q#1".into(), at_ms: 180_000 },
                Leave { name: "S#2".into(), at_ms: 190_000 },
            ],
            length_ms: 190_000,
        };
        let window = Window::from_minutes(8, 2);
        let names: Vec<_> = early(&short, window).iter().map(|l| l.name.clone()).collect();
        assert_eq!(names, ["Q#1"], "the end-of-match leave of S#2 never counts");
        // Defaults: 8 min limit, 2 min buffer.
        let profile = super::super::profile::Profile::default();
        assert_eq!((profile.early_leave_minutes, profile.end_buffer_minutes), (8, 2));
    }

    #[test]
    fn reading_a_replay_bans_by_rule_and_a_missing_one_asks_for_the_folder() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = App::new(directory.path(), None);
        app.ban_file = directory.path().join("bans-replay.json");
        app.own_name = Some("Me#1".into());
        app.last_game = Some(super::super::ban_list::LastGame {
            lobby: "DS #1".into(),
            players: vec!["Early#2".into(), "Late#3".into()],
            ..Default::default()
        });
        app.replay_read(Err(ReplayError::Missing("LastReplay.w3g was not found.".into())));
        assert!(app.replay_missing);
        app.replay_read(Ok(summary()));
        assert!(!app.replay_missing);
        let game = app.last_game.as_ref().unwrap();
        assert_eq!(game.auto_banned, ["Early#2"]);
        assert!(game.ended_early.is_some(), "a leave before 7 minutes ended the match");
        let list = ghost_reforged::bans::BanList::open(Some(&app.ban_file));
        assert_eq!(list.get("early#2").unwrap().reason, "Left the game early (4:10)");
        assert!(!list.is_banned("Late#3") && !list.is_banned("Me#1"));
    }

    #[test]
    fn the_chosen_folder_is_saved_and_cleared() {
        let directory = tempfile::tempdir().unwrap();
        assert!(load_folder(directory.path()).is_none());
        save_folder(directory.path(), Some(Path::new("D:/W3/Replay"))).unwrap();
        assert_eq!(load_folder(directory.path()), Some(PathBuf::from("D:/W3/Replay")));
        save_folder(directory.path(), None).unwrap();
        assert!(load_folder(directory.path()).is_none());
        save_folder(directory.path(), None).unwrap();
    }
}
