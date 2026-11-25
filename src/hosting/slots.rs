use super::{
    Slot, Snapshot,
    kick::{Refresh, RefreshGate},
};
use crate::{
    engine::{Engine, Phase},
    protocol::{Command, Lobby, OBSERVER_TEAM},
};
use serde_json::json;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotAction {
    Open,
    Close,
    Observer,
    Computer,
}

impl SlotAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Close => "Close",
            Self::Observer => "Move to observers",
            Self::Computer => "Add computer",
        }
    }

    fn allowed(self, slot: &Slot, observer_space: bool) -> bool {
        if slot.own {
            return false;
        }
        match self {
            Self::Open => slot.change_type && (slot.status == 1 || slot.computer),
            Self::Close => slot.change_type && (slot.status == 0 || slot.computer),
            Self::Computer => !slot.observer && slot.change_type && matches!(slot.status, 0 | 1),
            Self::Observer => {
                slot.change_team
                    && !slot.observer
                    && !slot.computer
                    && slot.status == 2
                    && observer_space
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SlotRequest {
    lobby: String,
    target: Slot,
    action: SlotAction,
}

impl Snapshot {
    pub fn slot_actions(&self, number: u8) -> Vec<(SlotAction, SlotRequest)> {
        let Some(slot) = self.slots.iter().find(|s| s.number == number) else {
            return vec![];
        };
        let observer_space = self.slots.iter().any(|s| s.observer && s.status == 0);
        [
            SlotAction::Open,
            SlotAction::Close,
            SlotAction::Observer,
            SlotAction::Computer,
        ]
        .into_iter()
        .filter(|action| self.can_edit_slots && action.allowed(slot, observer_space))
        .map(|action| {
            (
                action,
                SlotRequest {
                    lobby: self.name.clone(),
                    target: slot.clone(),
                    action,
                },
            )
        })
        .collect()
    }
}

impl SlotRequest {
    fn matches(&self, lobby: &Lobby) -> bool {
        let current = super::summarize(lobby, false, true);
        current.name == self.lobby
            && current
                .slot_actions(self.target.number)
                .iter()
                .any(|(_, request)| request == self)
    }

    fn command(&self) -> Command {
        let slot = self.target.number;
        match self.action {
            SlotAction::Open => Command::new("OpenSlot", json!({"slot":slot})),
            SlotAction::Close => Command::new("CloseSlot", json!({"slot":slot})),
            SlotAction::Observer => {
                Command::new("SetTeam", json!({"slot":slot,"team":OBSERVER_TEAM}))
            }
            SlotAction::Computer => {
                Command::new("SetComputerSlot", json!({"slot":slot,"difficulty":1}))
            }
        }
    }

    fn applied(&self, lobby: &Lobby) -> bool {
        if lobby.lobby_name != self.lobby {
            return false;
        }
        lobby.players.iter().any(|p| match self.action {
            SlotAction::Observer => {
                p.id == self.target.id
                    && p.name == self.target.name
                    && p.slot_status == 2
                    && (p.is_observer || p.team == OBSERVER_TEAM)
            }
            SlotAction::Open => p.slot == self.target.number && p.slot_status == 0,
            SlotAction::Close => p.slot == self.target.number && p.slot_status == 1,
            SlotAction::Computer => {
                p.slot == self.target.number && p.slot_status == 2 && p.slot_type == 1
            }
        })
    }
}

/// Player-mode hosts get every observer slot the map offers (up to 24 slots in all).
/// This closes the extras after creation so only `observer_slots` stay in play.
#[derive(Default)]
pub(super) struct ObserverTrim {
    lobby: String,
    rounds: u8,
    last: u64,
    done: bool,
}

impl ObserverTrim {
    const BATCH: usize = 8;
    const MAX_ROUNDS: u8 = 12;

    /// `roster` is true when the current event is a fresh `GameLobbySetup`;
    /// `idle` is false while a manual slot edit or kick is in progress.
    pub fn next(&mut self, engine: &Engine, now: u64, roster: bool, idle: bool) -> Vec<Command> {
        if engine.config.host_mode != crate::config::HostMode::Player
            || engine.phase != Phase::Lobby
            || !roster
            || !idle
        {
            return vec![];
        }
        let Some(lobby) = engine.owned_lobby(now) else {
            return vec![];
        };
        if self.lobby != lobby.lobby_name {
            *self = Self {
                lobby: lobby.lobby_name.clone(),
                ..Self::default()
            };
        }
        if self.done || self.rounds >= Self::MAX_ROUNDS || (self.rounds > 0 && now <= self.last) {
            return vec![];
        }
        let observers = || {
            lobby
                .players
                .iter()
                .filter(|p| !p.is_self && (p.is_observer || p.team == OBSERVER_TEAM))
        };
        let occupied = observers().filter(|p| p.slot_status == 2).count();
        let mut open: Vec<u8> = observers()
            .filter(|p| p.slot_status == 0 && p.slot_type_change_enabled)
            .map(|p| p.slot)
            .collect();
        // Players already sitting in observer slots use up part of the allowance.
        let keep = engine.config.observer_limit().saturating_sub(occupied);
        if open.len() <= keep {
            self.done = true;
            return vec![];
        }
        open.sort_unstable();
        let extra = open.split_off(keep);
        self.rounds += 1;
        self.last = now;
        let mut commands: Vec<Command> = extra
            .into_iter()
            .rev()
            .take(Self::BATCH)
            .map(|slot| Command::new("CloseSlot", json!({"slot":slot})))
            .collect();
        commands.push(Command::empty("SendGameLobbySetup"));
        commands
    }
}

#[derive(Default)]
pub(super) struct PendingSlot {
    request: Option<(SlotRequest, u64, RefreshGate)>,
    sent: bool,
}

impl PendingSlot {
    pub fn idle(&self) -> bool {
        self.request.is_none()
    }

    pub fn request(&mut self, request: SlotRequest, engine: &Engine, now: u64) {
        if self.request.is_none()
            && engine.phase == Phase::Lobby
            && engine.owned_lobby(now).is_some_and(|l| request.matches(l))
        {
            self.request = Some((request, now, RefreshGate::default()));
            self.sent = false;
        }
    }

    pub fn advance(
        &mut self,
        engine: &Engine,
        now: u64,
        roster: bool,
        idle: bool,
        alias: bool,
    ) -> (Vec<Command>, Option<String>) {
        let Some((request, started, gate)) = self.request.as_mut() else {
            return (vec![], None);
        };
        let lobby = engine.owned_lobby(now);
        let applied = self.sent && roster && lobby.is_some_and(|l| request.applied(l));
        let invalid = now.saturating_sub(*started) >= 6
            || engine.phase != Phase::Lobby
            || (!self.sent && !lobby.is_some_and(|l| request.matches(l)));
        if applied || invalid {
            let line = format!(
                "Slot {} · {} · {}",
                request.target.number + 1,
                request.action.label(),
                if applied { "applied" } else { "not applied" }
            );
            self.request = None;
            return (vec![], Some(line));
        }
        if !self.sent {
            match gate.advance(roster, idle, alias) {
                Refresh::Request => return (vec![Command::empty("SendGameLobbySetup")], None),
                Refresh::Confirm => {
                    self.sent = true;
                    *started = now;
                    return (
                        vec![request.command(), Command::empty("SendGameLobbySetup")],
                        None,
                    );
                }
                Refresh::Wait => {}
            }
        }
        (vec![], None)
    }
}
