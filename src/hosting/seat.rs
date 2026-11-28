//! Player mode, Direct Strike maps only: moves the host to the chosen colour (position)
//! in each new lobby, or to a random free colour other than red.
//!
//! Direct Strike is a fixed-player-settings map: choosing a colour moves the player to
//! that colour's slot (`SetColor`). A colour of the other team first needs `SetTeam`.
//! Each step waits for the next roster; after a few tries the picker gives up and
//! says so in Activity.
use crate::{
    commands::{COLORS, color_allowed, color_name},
    config::{HostMode, Seat},
    engine::{Engine, Phase},
    protocol::{Command, Lobby, OBSERVER_TEAM, Player},
};
use serde_json::{Value, json};

/// Seconds to wait for a roster after a move before trying again.
const RETRY_AFTER: u64 = 3;
/// Moves sent per lobby before giving up.
const MAX_TRIES: u8 = 4;

#[derive(Default)]
pub(super) struct SeatPicker {
    lobby: String,
    choice: Seat,
    target: Option<u8>,
    sent_at: Option<u64>,
    tries: u8,
    done: bool,
}

/// The team whose colour list contains `color`, for fixed-player-settings lobbies.
fn team_of(lobby: &Lobby, color: u8) -> Option<u8> {
    let holds = |group: &Value| {
        group
            .as_array()
            .is_some_and(|colors| colors.iter().any(|c| c.as_u64() == Some(color as u64)))
    };
    match &lobby.available_team_colors {
        Value::Array(groups) => groups.iter().position(holds).map(|t| t as u8),
        Value::Object(groups) => groups
            .iter()
            .find(|(_, group)| holds(group))
            .and_then(|(team, _)| team.parse().ok()),
        _ => None,
    }
}

/// Another occupied player slot already has this colour.
fn taken(lobby: &Lobby, me: &Player, color: u8) -> bool {
    lobby.players.iter().any(|p| {
        p.slot != me.slot
            && p.slot_status == 2
            && !p.is_observer
            && p.team != OBSERVER_TEAM
            && p.color == Some(color)
    })
}

/// The move that brings `me` closer to `color`, if any.
pub(super) fn step_towards(lobby: &Lobby, me: &Player, color: u8) -> Option<Command> {
    if color_allowed(lobby, me, color) {
        return Some(Command::new("SetColor", json!({"slot":me.slot,"color":color})));
    }
    let team = team_of(lobby, color)?;
    (lobby.is_custom_players && team != me.team && me.team_change_enabled && !taken(lobby, me, color))
        .then(|| Command::new("SetTeam", json!({"slot":me.slot,"team":team})))
}

/// Free colours other than red that the host can reach (directly or via a team change).
pub(super) fn random_candidates(lobby: &Lobby, me: &Player) -> Vec<u8> {
    (1..COLORS.len() as u8)
        .filter(|&color| me.color != Some(color))
        .filter(|&color| step_towards(lobby, me, color).is_some())
        .collect()
}

/// A random index below `len` without a rand dependency.
fn random_below(len: usize) -> usize {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default(),
    );
    (hasher.finish() % len.max(1) as u64) as usize
}

impl SeatPicker {
    /// Called every controller tick while nothing else edits the lobby.
    /// Returns a move to send and/or a line for Activity.
    pub(super) fn step(&mut self, engine: &Engine, ready: bool, now: u64) -> (Option<Command>, Option<String>) {
        let config = &engine.config;
        if config.host_mode != HostMode::Player
            || !config.is_direct_strike()
            || config.seat == Seat::Default
            || engine.phase != Phase::Lobby
        {
            return (None, None);
        }
        let Some(lobby) = engine.owned_lobby(now) else {
            return (None, None);
        };
        let Some(me) = lobby.local() else {
            return (None, None);
        };
        if lobby.lobby_name != self.lobby || config.seat != self.choice {
            *self = Self {
                lobby: lobby.lobby_name.clone(),
                choice: config.seat,
                ..Self::default()
            };
        }
        if self.done || !ready {
            return (None, None);
        }
        if self.sent_at.is_some_and(|at| now < at + RETRY_AFTER) {
            return (None, None);
        }
        let target = match (self.target, config.seat) {
            (Some(target), _) => target,
            (None, Seat::Color(color)) => color,
            (None, Seat::Random) => {
                let choices = random_candidates(lobby, me);
                if choices.is_empty() {
                    self.done = true;
                    return (None, Some("Seat · no free colour for a random pick; staying put".into()));
                }
                choices[random_below(choices.len())]
            }
            (None, Seat::Default) => return (None, None),
        };
        self.target = Some(target);
        if me.color == Some(target) {
            self.done = true;
            return (None, Some(format!("Seat · you are {}", color_name(target))));
        }
        if self.tries >= MAX_TRIES {
            self.done = true;
            return (None, Some(format!("Seat · could not move to {}", color_name(target))));
        }
        match step_towards(lobby, me, target) {
            Some(command) => {
                self.tries += 1;
                self.sent_at = Some(now);
                (Some(command), None)
            }
            // A random pick that someone took meanwhile: pick again.
            None if config.seat == Seat::Random && taken(lobby, me, target) => {
                self.target = None;
                self.tries += 1;
                (None, None)
            }
            None => {
                self.done = true;
                let why = if taken(lobby, me, target) { "is taken" } else { "is not available on this map" };
                (None, Some(format!("Seat · {} {why}; staying put", color_name(target))))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lobby(players: Value) -> Lobby {
        serde_json::from_value(json!({
            "isCustomPlayers": true,
            "availableTeamColors": [[0, 1, 2, 3], [4, 5, 6, 7]],
            "isHost": true,
            "lobbyName": "DS #1",
            "players": players,
            "mapData": {"mapPath": "Maps/5335_Direct Strike 6.5.13f.w3x"},
        }))
        .unwrap()
    }

    fn player(slot: u8, team: u8, color: u8, me: bool) -> Value {
        json!({"slot":slot,"slotStatus":2,"slotType":0,"team":team,"color":color,
            "colorChangeEnabled":true,"isSelf":me,"isObserver":false,"teamChangeEnabled":true,
            "name": if me { "Host#1" } else { "Guest#2" }})
    }

    #[test]
    fn a_colour_on_the_own_team_is_one_set_color() {
        let lobby = lobby(json!([player(0, 0, 0, true)]));
        let me = lobby.local().unwrap();
        let command = step_towards(&lobby, me, 2).unwrap();
        assert_eq!(command.message, "SetColor");
        assert_eq!(command.payload["color"], 2);
    }

    #[test]
    fn a_colour_on_the_other_team_starts_with_a_team_change() {
        let lobby = lobby(json!([player(0, 0, 0, true)]));
        let me = lobby.local().unwrap();
        let command = step_towards(&lobby, me, 5).unwrap();
        assert_eq!(command.message, "SetTeam");
        assert_eq!(command.payload["team"], 1);
    }

    #[test]
    fn random_never_picks_red_or_a_taken_colour() {
        let lobby = lobby(json!([player(0, 0, 0, true), player(1, 0, 1, false)]));
        let me = lobby.local().unwrap();
        let choices = random_candidates(&lobby, me);
        assert!(!choices.is_empty());
        assert!(!choices.contains(&0), "never red");
        assert!(!choices.contains(&1), "taken by a guest");
        assert!(choices.iter().all(|c| *c < 8), "only the map's colours");
    }
}
