use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub title: String,
    pub map: Option<PathBuf>,
    #[serde(default, rename = "bot_name", skip_serializing)]
    pub legacy_bot_name: serde::de::IgnoredAny,
    pub lobby_pattern: String,
    #[serde(default)]
    pub automatic_name: bool,
    pub next_number: u32,
    pub players: u8,
    pub countdown: u16,
    #[serde(default = "private_by_default", skip_serializing)]
    pub auto_start: bool,
    pub rehost: bool,
    #[serde(default = "private_by_default")]
    pub private_game: bool,
    #[serde(default)]
    pub speed: GameSpeed,
    #[serde(default = "private_by_default")]
    pub require_full: bool,
    #[serde(default)]
    pub commands: ghost_reforged::commands::CommandSettings,
    #[serde(default)]
    pub welcome: ghost_reforged::welcome::Welcome,
    #[serde(default = "rehost_delay")]
    pub rehost_delay_seconds: u64,
    #[serde(default)]
    pub empty_rehost_seconds: u64,
    #[serde(default)]
    pub player_host: bool,
    /// Observer slots kept open in player-host lobbies (0..=12).
    #[serde(default)]
    pub observer_slots: u8,
    #[serde(default = "ds_patch")]
    pub ds_patch_version: String,
    /// 0 = filter off.
    #[serde(default)]
    pub min_ds_level: u32,
    /// 0 = filter off.
    #[serde(default)]
    pub min_ds_matches: u32,
    /// Play as host: press the Direct Strike end panel's Quit button automatically.
    #[serde(default = "private_by_default")]
    pub auto_quit: bool,
    #[serde(default = "auto_quit_delay")]
    pub auto_quit_delay_seconds: u64,
    /// Last resort when Quit cannot be pressed: leave the game (may count as a leave).
    #[serde(default)]
    pub leave_if_quit_fails: bool,
    /// Play as host: host the next lobby after each of your matches. Separate from
    /// `rehost`, which is the observer bot's "create next lobby after countdown", so
    /// switching modes never inherits the other mode's choice. On by default.
    #[serde(default = "private_by_default")]
    pub player_rehost: bool,
    /// Play as host: who is banned automatically when the replay shows they left early.
    #[serde(default)]
    pub early_leave_rule: EarlyLeaveRule,
    /// Leaving before this many minutes always counts as leaving early.
    #[serde(default = "early_leave_minutes")]
    pub early_leave_minutes: u32,
    /// Leaving in the last this-many minutes of a match does not count (the result is
    /// already decided), unless it is also before `early_leave_minutes`.
    #[serde(default = "end_buffer_minutes")]
    pub end_buffer_minutes: u32,
    /// Play as host: bring Warcraft III to the front when a game starts.
    #[serde(default = "private_by_default")]
    pub focus_on_start: bool,
}

pub const MAX_EARLY_LEAVE_MINUTES: u32 = 30;
pub const MAX_END_BUFFER_MINUTES: u32 = 10;

fn early_leave_minutes() -> u32 {
    8
}

fn end_buffer_minutes() -> u32 {
    2
}

/// Which early leavers (read from the match replay) are banned automatically.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EarlyLeaveRule {
    /// Left before the early-leave limit (`Profile::early_leave_minutes`, default 8).
    /// The serialised name is kept from 0.16.0 so saved profiles still load.
    #[default]
    BeforeSevenMinutes,
    /// Left at any time before the match ended.
    AnyEarlyLeave,
    /// Only list them; ban by hand.
    Off,
}

impl EarlyLeaveRule {
    pub const ALL: [Self; 3] = [Self::BeforeSevenMinutes, Self::AnyEarlyLeave, Self::Off];

    pub fn label(self) -> &'static str {
        match self {
            Self::BeforeSevenMinutes => "Before the limit",
            Self::AnyEarlyLeave => "Any early leave",
            Self::Off => "Off (list only)",
        }
    }
}

fn auto_quit_delay() -> u64 {
    5
}

fn ds_patch() -> String {
    ghost_reforged::ds_report::DEFAULT_PATCH.into()
}

fn rehost_delay() -> u64 {
    15
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GameSpeed {
    Slow,
    Normal,
    #[default]
    Fast,
}

impl GameSpeed {
    pub const ALL: [Self; 3] = [Self::Slow, Self::Normal, Self::Fast];
    pub fn wire(self) -> u8 {
        match self {
            Self::Slow => 0,
            Self::Normal => 1,
            Self::Fast => 2,
        }
    }
}

impl std::fmt::Display for GameSpeed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Slow => "Slow",
            Self::Normal => "Normal",
            Self::Fast => "Fast",
        })
    }
}

fn private_by_default() -> bool {
    true
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            title: "Profile 1".into(),
            map: None,
            legacy_bot_name: serde::de::IgnoredAny,
            lobby_pattern: "Game #{n}".into(),
            automatic_name: true,
            next_number: 1,
            players: 4,
            countdown: 0,
            auto_start: true,
            rehost: true,
            private_game: true,
            speed: GameSpeed::Fast,
            require_full: true,
            commands: Default::default(),
            welcome: Default::default(),
            rehost_delay_seconds: rehost_delay(),
            empty_rehost_seconds: 0,
            player_host: false,
            observer_slots: 0,
            ds_patch_version: ds_patch(),
            min_ds_level: 0,
            min_ds_matches: 0,
            auto_quit: true,
            auto_quit_delay_seconds: auto_quit_delay(),
            leave_if_quit_fails: false,
            player_rehost: true,
            early_leave_rule: EarlyLeaveRule::default(),
            early_leave_minutes: early_leave_minutes(),
            end_buffer_minutes: end_buffer_minutes(),
            focus_on_start: true,
        }
    }
}

impl Profile {
    /// Whether the next lobby is hosted automatically, for the mode in use.
    pub fn rehosts(&self) -> bool {
        if self.player_host {
            self.player_rehost
        } else {
            self.rehost
        }
    }

    pub fn set_rehost(&mut self, value: bool) {
        if self.player_host {
            self.player_rehost = value;
        } else {
            self.rehost = value;
        }
    }

    pub fn live_settings(&self) -> ghost_reforged::hosting::LiveSettings {
        ghost_reforged::hosting::LiveSettings {
            minimum: self.players,
            full: self.require_full,
            rehost: self.rehosts(),
            rehost_delay: self.rehost_delay_seconds,
            empty_refresh: self.empty_rehost_seconds,
            commands: self.commands.clone(),
            welcome: self.welcome.clone(),
            min_ds_level: self.min_ds_level,
            min_ds_matches: self.min_ds_matches,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        self.welcome.validate()?;
        ghost_reforged::ds_report::validate_patch(&self.ds_patch_version)?;
        if self.title.trim().is_empty()
            || self.title.chars().count() > 60
            || self.title.chars().any(char::is_control)
        {
            return Err("Profile names must contain 1 to 60 characters.".into());
        }
        if !self.lobby_pattern.contains("{n}")
            || self.lobby_pattern.replace("{n}", "4294967295").len() > 31
            || self.lobby_pattern.chars().any(char::is_control)
            || self.lobby_pattern.replace("{n}", "").contains(['{', '}'])
        {
            return Err(
                "Game names must contain {n} and fit in 31 UTF-8 bytes including the counter."
                    .into(),
            );
        }
        if !(1..=23).contains(&self.players) || self.countdown > 300 {
            return Err("Choose 1 to 23 players and a countdown from 0 to 300 seconds.".into());
        }
        if !(1..=MAX_EARLY_LEAVE_MINUTES).contains(&self.early_leave_minutes)
            || self.end_buffer_minutes > MAX_END_BUFFER_MINUTES
        {
            return Err("Early-leave minutes must be 1 to 30; the end buffer 0 to 10.".into());
        }
        if self.auto_quit_delay_seconds > 60 {
            return Err("The auto-quit delay must be 0 to 60 seconds.".into());
        }
        if self.next_number == 0 {
            return Err("Next number must be at least 1.".into());
        }
        if !(5..=3600).contains(&self.rehost_delay_seconds) || self.empty_rehost_seconds > 86400 {
            return Err(
                "Rehost delay must be 5–3600 seconds; empty lobby refresh must be 0–86400 seconds."
                    .into(),
            );
        }
        if let Some(map) = &self.map {
            let ext = map.extension().and_then(|s| s.to_str()).unwrap_or("");
            if !ext.eq_ignore_ascii_case("w3x") && !ext.eq_ignore_ascii_case("w3m") {
                return Err("Select a .w3x or .w3m map.".into());
            }
        }
        Ok(())
    }

    pub fn lobby_name(&self, number: u32) -> String {
        self.lobby_pattern.replace("{n}", &number.to_string())
    }

    pub fn map_label(&self) -> String {
        self.map
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Select a .w3x or .w3m map".into())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Profiles {
    pub version: u32,
    pub selected: usize,
    pub items: Vec<Profile>,
}

impl Default for Profiles {
    fn default() -> Self {
        Self {
            version: 2,
            selected: 0,
            items: vec![Profile::default()],
        }
    }
}

impl Profiles {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 2
            || self.items.is_empty()
            || self.items.len() > 30
            || self.selected >= self.items.len()
        {
            return Err("Unsupported profile format. The file has been preserved.".into());
        }
        self.items.iter().try_for_each(Profile::validate)
    }
}
