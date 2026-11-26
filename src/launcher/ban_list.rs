//! Launcher-side ban tools: ban any BattleTag by hand, and ban players from the last game.
//!
//! The launcher writes `bans.json` directly. A running hosting worker re-reads the file
//! within about a second and removes listed players from the lobby, so no worker
//! message is needed and banning also works while nothing is being hosted.
use super::{
    app::{App, Message},
    effects,
    frame_art::Frame,
    palette as p,
    widgets::{action, muted},
};
use ghost_reforged::bans::{BanList, MAX_REASON};
use ghost_reforged::hosting::Snapshot;
use iced::{
    Alignment, Element, Fill,
    widget::{column, row, text, text_input},
};
use std::path::PathBuf;

/// Players remembered from the most recent started game.
#[derive(Debug, Clone, Default)]
pub struct LastGame {
    pub lobby: String,
    pub players: Vec<String>,
    /// Set when the match ended before `EARLY_END_SECONDS` (Direct Strike ends the game
    /// when someone leaves before 7 minutes). Holds the match length in seconds.
    pub ended_early: Option<u64>,
}

/// Direct Strike ends a match early when a player leaves within its first 7 minutes.
pub const EARLY_END_SECONDS: u64 = 7 * 60;
pub const EARLY_LEAVE_REASON: &str = "Left the game early";

/// Whether a match of this length counts as ended by an early leaver.
pub fn ended_early(seconds: u64) -> bool {
    seconds < EARLY_END_SECONDS
}

/// Human guests in a lobby snapshot, excluding the host, computers and unresolved names.
pub fn guests(lobby: &Snapshot) -> Vec<String> {
    let mut names: Vec<String> = lobby
        .slots
        .iter()
        .filter(|s| s.status == 2 && !s.computer && !s.own && !s.name.trim().is_empty())
        .map(|s| s.name.clone())
        .collect();
    names.dedup();
    names
}

impl App {
    /// Same location the hosting worker uses: beside the program, else the data folder.
    /// Resolved once at startup because `locate` touches the file system.
    pub(super) fn locate_ban_file(data: &std::path::Path) -> PathBuf {
        BanList::locate(
            std::env::current_exe()
                .ok()
                .as_deref()
                .and_then(std::path::Path::parent),
            data,
        )
    }

    pub(super) fn ban_path(&self) -> PathBuf {
        self.ban_file.clone()
    }

    pub(super) fn is_recently_banned(&self, name: &str) -> bool {
        self.recent_bans.contains(&name.trim().to_lowercase())
    }

    /// Adds a name to the ban list file. Returns a message for the notice line.
    pub(super) fn ban_name(&mut self, name: &str) -> Result<(), String> {
        let reason = self.ban_form_reason.trim().to_owned();
        self.ban_name_with(name, &reason)
    }

    /// Saves the ban locally (`bans.json`). If the player joins a later lobby, the host
    /// removes them with Warcraft III's native ban.
    pub(super) fn ban_name_with(&mut self, name: &str, reason: &str) -> Result<(), String> {
        let name = name.trim().to_owned();
        if name.is_empty() {
            return Err("Type a BattleTag to ban, for example Name#1234.".into());
        }
        let reason = reason.trim().to_owned();
        let mut list = BanList::open(Some(&self.ban_path()));
        if let Some(warning) = list.warning() {
            self.log(&warning);
        }
        let added = list.add(&name, &reason)?;
        self.recent_bans.insert(name.to_lowercase());
        let line = if added {
            if reason.is_empty() {
                format!("Banned · {name}")
            } else {
                format!("Banned · {name} · {reason}")
            }
        } else {
            format!("Already banned · {name}")
        };
        self.log(&line);
        self.notice = line;
        self.notice_error = false;
        Ok(())
    }

    pub(super) fn ban_message(&mut self, message: &Message) -> bool {
        match message {
            Message::BanFormTag(value) => {
                self.ban_form_tag = value.chars().take(160).collect();
            }
            Message::BanFormReason(value) => {
                self.ban_form_reason = value.chars().take(MAX_REASON).collect();
            }
            Message::BanFormSubmit => {
                let tag = self.ban_form_tag.clone();
                match self.ban_name(&tag) {
                    Ok(()) => self.ban_form_tag.clear(),
                    Err(error) => self.notify(Err(error), ""),
                }
            }
            Message::BanLastPlayer(name) => {
                let name = name.clone();
                if let Err(error) = self.ban_name(&name) {
                    self.notify(Err(error), "");
                }
            }
            Message::BanEarlyLeaver(name) => {
                let name = name.clone();
                if let Err(error) = self.ban_name_with(&name, EARLY_LEAVE_REASON) {
                    self.notify(Err(error), "");
                }
            }
            _ => return false,
        }
        true
    }
}

fn input<'a>(placeholder: &'a str, value: &'a str, on: fn(String) -> Message) -> Element<'a, Message> {
    effects::skin(
        text_input(placeholder, value)
            .on_input(on)
            .on_submit(Message::BanFormSubmit)
            .padding([8, 14])
            .size(16)
            .style(p::input),
        Frame::Input,
    )
}

/// The "Ban list" section shown on the Options page.
pub fn section(app: &App) -> Element<'_, Message> {
    let mut content = column![
        text("Ban list").size(20),
        muted(
            "Ban any BattleTag at any time. Name#1234 bans that exact BattleTag; a bare \
             Name bans every tag with that name. Banned players are removed from the \
             lobby whenever they join, including the current one."
        ),
        row![
            input("BattleTag (Name#1234)", &app.ban_form_tag, Message::BanFormTag),
            input("Reason (optional)", &app.ban_form_reason, Message::BanFormReason),
            action("Ban", Message::BanFormSubmit, !app.ban_form_tag.trim().is_empty()),
        ]
        .spacing(9)
        .align_y(Alignment::Center),
        muted(format!("Saved to {}", app.ban_path().display())),
        text("Last game's players").size(17),
    ]
    .spacing(12);
    match &app.last_game {
        Some(game) if !game.players.is_empty() => {
            content = content.push(muted(format!(
                "{} · the reason above is used for these bans",
                game.lobby
            )));
            for name in &game.players {
                let banned = app.is_recently_banned(name);
                content = content.push(
                    row![
                        text(name.clone()).size(15).width(Fill),
                        if banned { muted("Banned") } else { muted("") },
                        action("Ban", Message::BanLastPlayer(name.clone()), !banned),
                    ]
                    .spacing(9)
                    .align_y(Alignment::Center),
                );
            }
        }
        _ => {
            content = content.push(muted(
                "No game has started since the launcher opened. Players appear here once a game starts.",
            ));
        }
    }
    content.into()
}
