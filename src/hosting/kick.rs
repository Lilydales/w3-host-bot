use super::{Slot, Snapshot};
use crate::{
    bans::BanList,
    engine::{Engine, Phase},
    protocol::{Command, Lobby},
};
use serde_json::json;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct KickRequest {
    lobby: String,
    slot: u8,
    id: i64,
    name: String,
}

impl Snapshot {
    pub fn kick_target(&self, slot: u8) -> Option<KickRequest> {
        let player = self.slots.iter().find(|p| p.number == slot)?;
        (self.can_edit_slots && player.kickable()).then(|| KickRequest {
            lobby: self.name.clone(),
            slot,
            id: player.id.unwrap(),
            name: player.name.clone(),
        })
    }
}

impl Slot {
    pub fn kickable(&self) -> bool {
        self.status == 2
            && !self.computer
            && !self.own
            && self.id.is_some_and(|id| id >= 0 && id != 255)
            && !self.name.trim().is_empty()
    }
}

impl KickRequest {
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The first banned player who can be removed from this lobby right now.
    fn banned(lobby: &Lobby, bans: &BanList, ready: impl Fn(i64) -> bool) -> Option<Self> {
        if !lobby.is_host {
            return None;
        }
        lobby.players.iter().find_map(|p| {
            let id = p.id.filter(|id| *id >= 0 && *id != 255)?;
            (p.slot_status == 2
                && p.slot_type == 0
                && !p.is_self
                && !p.name.trim().is_empty()
                && bans.is_banned(&p.name)
                && ready(id))
            .then(|| Self {
                lobby: lobby.lobby_name.clone(),
                slot: p.slot,
                id,
                name: p.name.clone(),
            })
        })
    }

    /// A kick for this named guest when they can be removed right now. Never the host.
    pub(super) fn guest(lobby: &Lobby, name: &str) -> Option<Self> {
        if !lobby.is_host {
            return None;
        }
        lobby.players.iter().find_map(|p| {
            let id = p.id.filter(|id| *id >= 0 && *id != 255)?;
            (p.name == name && p.slot_status == 2 && p.slot_type == 0 && !p.is_self).then(|| Self {
                lobby: lobby.lobby_name.clone(),
                slot: p.slot,
                id,
                name: p.name.clone(),
            })
        })
    }

    fn matches(&self, lobby: &Lobby) -> bool {
        lobby.is_host
            && lobby
                .local()
                .is_some_and(|p| p.slot_status == 2 && p.slot_type == 0)
            && lobby.lobby_name == self.lobby
            && lobby.players.iter().any(|p| {
                p.slot == self.slot
                    && p.id == Some(self.id)
                    && p.name == self.name
                    && p.slot_status == 2
                    && p.slot_type == 0
                    && !p.is_self
            })
    }
}

#[derive(Default)]
pub(super) struct RefreshGate {
    requested: bool,
    observed: bool,
}

#[derive(Debug, PartialEq)]
pub(super) enum Refresh {
    Wait,
    Request,
    Confirm,
}

impl RefreshGate {
    pub fn advance(&mut self, roster: bool, idle: bool, alias_ready: bool) -> Refresh {
        if !self.requested {
            if idle && alias_ready {
                self.requested = true;
                return Refresh::Request;
            }
        } else {
            self.observed |= roster;
            if self.observed && idle && alias_ready {
                return Refresh::Confirm;
            }
        }
        Refresh::Wait
    }
}

/// Picks banned players to remove, without retrying the same player every tick.
#[derive(Default)]
pub(super) struct BanEnforcer {
    attempted: BTreeMap<i64, u64>,
}

impl BanEnforcer {
    const RETRY_SECONDS: u64 = 4;

    pub fn next(&mut self, lobby: &Lobby, bans: &BanList, now: u64) -> Option<KickRequest> {
        self.attempted
            .retain(|_, at| now.saturating_sub(*at) < Self::RETRY_SECONDS);
        let target = KickRequest::banned(lobby, bans, |id| !self.attempted.contains_key(&id))?;
        self.attempted.insert(target.id, now);
        Some(target)
    }
}

/// Native Warcraft III ban is on by default; set `W3_NATIVE_BAN=0` to send a plain kick.
fn native_ban_enabled() -> bool {
    std::env::var("W3_NATIVE_BAN").map_or(true, |v| v != "0")
}

#[derive(Default)]
pub(super) struct PendingKick {
    pending: Option<(KickRequest, u64, RefreshGate)>,
    native_ban: bool,
    /// Why the pending player is being removed; shown in lobby chat and the launcher.
    reason: String,
    /// Set when a kick command was just produced: (player, reason).
    kicked: Option<(String, String)>,
}

impl PendingKick {
    pub fn idle(&self) -> bool {
        self.pending.is_none()
    }

    /// Replaces the reason for the request just made with `request` / `request_ban`.
    pub fn set_reason(&mut self, reason: impl Into<String>) {
        self.reason = reason.into();
    }

    /// The player and reason of a kick that was just sent, once.
    pub fn take_kicked(&mut self) -> Option<(String, String)> {
        self.kicked.take()
    }

    pub fn request(&mut self, target: KickRequest, engine: &Engine, now: u64) -> Option<()> {
        if engine.phase != Phase::Lobby
            || !engine.owned_lobby(now).is_some_and(|l| target.matches(l))
        {
            return None;
        }
        self.pending = Some((target, now, RefreshGate::default()));
        self.native_ban = false;
        self.reason = "Kicked by the host".into();
        Some(())
    }

    /// Like `request`, but sends Warcraft III's native ban instead of a plain kick.
    /// The ban list entry is saved separately, so if the game ignores the native
    /// ban the enforcer still removes the player.
    pub fn request_ban(&mut self, target: KickRequest, engine: &Engine, now: u64) -> Option<()> {
        self.request(target, engine, now)?;
        self.native_ban = native_ban_enabled();
        Some(())
    }

    pub fn confirm(
        &mut self,
        engine: &Engine,
        now: u64,
        fresh_roster: bool,
        alias_ready: bool,
        input_idle: bool,
    ) -> Option<(Option<Command>, Option<String>)> {
        let (target, requested, gate) = self.pending.as_mut()?;
        if now.saturating_sub(*requested) >= 5
            || engine.phase != Phase::Lobby
            || !engine.owned_lobby(now).is_some_and(|l| target.matches(l))
        {
            let name = target.name.clone();
            self.pending = None;
            return Some((None, Some(format!("Kick cancelled · {name}"))));
        }
        match gate.advance(fresh_roster, input_idle, alias_ready) {
            Refresh::Wait => return None,
            Refresh::Request => return Some((Some(Command::empty("SendGameLobbySetup")), None)),
            Refresh::Confirm => {}
        }
        let (target, _, _) = self.pending.take()?;
        let native_ban = std::mem::take(&mut self.native_ban);
        if engine.owned_lobby(now).is_some_and(|l| target.matches(l)) {
            let (message, line) = if native_ban {
                ("BanPlayerFromGameLobby", "Native ban requested")
            } else {
                ("KickPlayerFromGameLobby", "Kick requested")
            };
            self.kicked = Some((target.name.clone(), std::mem::take(&mut self.reason)));
            Some((
                Some(Command::new(message, json!({"slot":target.slot}))),
                Some(format!("{line} · {} · {}", target.name, self.kicked.as_ref().map_or("", |k| k.1.as_str()))),
            ))
        } else {
            Some((None, Some(format!("Kick cancelled · {}", target.name))))
        }
    }
}

/// The lobby chat line sent just before a kick.
pub(super) fn reason_chat(name: &str, reason: &str) -> Command {
    let text: String = format!("Removing {name}: {reason}")
        .chars()
        .filter(|c| !c.is_control())
        .take(200)
        .collect();
    // A leading slash would run a native chat command.
    let text = if text.starts_with('/') { format!(" {text}") } else { text };
    Command::new("SendGameChatMessage", json!({"content": text}))
}

/// "Banned: <reason>", or a generic line when no reason was given.
pub(super) fn ban_reason(reason: Option<&str>) -> String {
    match reason.map(str::trim).filter(|r| !r.is_empty()) {
        Some(reason) => format!("Banned: {reason}"),
        None => "On the ban list".into(),
    }
}

pub(super) fn invalidate_vote(
    pending: &mut Option<(crate::commands::KickTarget, u64, RefreshGate)>,
    engine: &Engine,
    now: u64,
) {
    if pending.as_ref().is_some_and(|(target, requested, _)| {
        now.saturating_sub(*requested) >= 5
            || !matches!(engine.phase, Phase::Lobby | Phase::Countdown(_))
            || engine
                .owned_lobby(now)
                .is_none_or(|lobby| target.confirm(lobby).is_none())
    }) {
        *pending = None;
    }
}

#[cfg(test)]
#[path = "ban_tests.rs"]
mod ban_tests;
