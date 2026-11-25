//! Background ds-report.com lookups for joining players.
//!
//! Requests run on short-lived threads; the controller only ever polls a channel,
//! so a slow or unreachable site never delays the hosting loop.
use super::{
    Activity, Event,
    kick::{KickRequest, PendingKick},
};
use crate::{
    config::Config,
    ds_report::{self, Lookup},
    engine::{Engine, Phase},
    protocol::{Command, Lobby},
    welcome::Announcements,
};
use serde_json::json;
use std::{
    collections::{BTreeSet, VecDeque},
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

/// How long a rejected player may stay un-kickable (roster still resolving) before giving up.
const KICK_PATIENCE: u64 = 20;
const MAX_IN_FLIGHT: usize = 24;

pub(super) struct DsCheck {
    sender: Sender<(String, Lookup)>,
    results: Receiver<(String, Lookup)>,
    pending: BTreeSet<String>,
    seen: BTreeSet<String>,
    messages: VecDeque<(String, String)>,
    kicks: VecDeque<(String, u64)>,
    last_sent: Option<u64>,
}

impl Default for DsCheck {
    fn default() -> Self {
        let (sender, results) = mpsc::channel();
        Self {
            sender,
            results,
            pending: BTreeSet::new(),
            seen: BTreeSet::new(),
            messages: VecDeque::new(),
            kicks: VecDeque::new(),
            last_sent: None,
        }
    }
}

fn announcement(name: &str, result: Lookup) -> String {
    match result {
        Lookup::Found(s) => format!("{name} - Lv. {} ({} matches)", s.level, s.matches),
        _ => format!("{name} - Lv. N/A"),
    }
}

fn chat_safe(text: &str) -> String {
    let clean: String = text.chars().filter(|c| !c.is_control()).collect();
    let end = (0..=clean.len().min(240))
        .rev()
        .find(|i| clean.is_char_boundary(*i))
        .unwrap_or(0);
    let clean = &clean[..end];
    // A slash at the beginning would invoke a native chat command.
    if clean.starts_with('/') {
        format!(" {clean}")
    } else {
        clean.into()
    }
}

impl DsCheck {
    /// Starts a lookup for a joining guest. Returns `false` when the player is not checked
    /// (no BattleTag, the host, or no room for another lookup), so the normal welcome applies.
    pub fn join(&mut self, config: &Config, lobby: Option<&Lobby>, name: &str) -> bool {
        if !name.contains('#')
            || lobby.is_some_and(|l| l.players.iter().any(|p| p.is_self && p.name == name))
        {
            return false;
        }
        if self.pending.contains(name) || self.seen.contains(name) {
            return true;
        }
        if self.pending.len() >= MAX_IN_FLIGHT {
            return false;
        }
        let (sender, tag, patch) = (
            self.sender.clone(),
            name.to_owned(),
            config.ds_patch_version.clone(),
        );
        let started = thread::Builder::new()
            .name("ghost-ds-lookup".into())
            .spawn(move || {
                let result = ds_report::lookup(&tag, &patch);
                let _ = sender.send((tag, result));
            });
        if started.is_err() {
            return false;
        }
        self.pending.insert(name.to_owned());
        self.seen.insert(name.to_owned());
        true
    }

    pub fn left(&mut self, name: &str) {
        self.pending.remove(name);
        self.seen.remove(name);
        self.messages.retain(|(n, _)| n != name);
        self.kicks.retain(|(n, _)| n != name);
    }

    /// New lobby or game: forget everything, including answers still on their way.
    pub fn reset(&mut self) {
        self.pending.clear();
        self.seen.clear();
        self.messages.clear();
        self.kicks.clear();
        self.last_sent = None;
        while self.results.try_recv().is_ok() {}
    }

    /// Collects finished lookups. Never blocks.
    pub fn poll(
        &mut self,
        config: &Config,
        now: u64,
        events: &Sender<Event>,
        announcements: &mut Announcements,
    ) {
        while let Ok((name, result)) = self.results.try_recv() {
            if !self.pending.remove(&name) {
                continue; // The player left, or the lobby changed, while it ran.
            }
            if result == Lookup::Failed {
                // No data and no verdict: fall back to the ordinary welcome.
                announcements.joined(&name);
                continue;
            }
            let _ = events.send(Event::DsStats(name.clone(), result.stats()));
            if ds_report::rejects(config.min_ds_level, config.min_ds_matches, result) {
                let detail = match result.stats() {
                    Some(s) => format!("Lv. {} ({} matches)", s.level, s.matches),
                    None => "not found on W3Champions".into(),
                };
                let _ = events.send(Event::Activity(Activity::Command(format!(
                    "DS filter · {name} · {detail} · removing"
                ))));
                self.kicks.push_back((name, now));
            } else {
                let text = announcement(&name, result);
                self.messages.push_back((name, text));
            }
        }
    }

    /// The next stats line for a player who is still in the lobby, one per second.
    pub fn message(&mut self, lobby: &Lobby, welcome_enabled: bool, now: u64) -> Option<Command> {
        if !welcome_enabled {
            self.messages.clear();
            return None;
        }
        if self.last_sent.is_some_and(|last| now <= last) {
            return None;
        }
        while let Some((name, text)) = self.messages.pop_front() {
            if lobby
                .players
                .iter()
                .any(|p| p.name == name && p.slot_status == 2 && p.slot_type == 0 && !p.is_self)
            {
                self.last_sent = Some(now);
                return Some(Command::new(
                    "SendGameChatMessage",
                    json!({"content": chat_safe(&text)}),
                ));
            }
        }
        None
    }

    /// Hands the next rejected player to the manual kick pipeline. Never bans, never the host.
    pub fn kick(&mut self, engine: &Engine, manual: &mut PendingKick, now: u64) -> bool {
        if !manual.idle() || engine.phase != Phase::Lobby {
            return false;
        }
        let Some(lobby) = engine.owned_lobby(now) else {
            return false;
        };
        while let Some((name, since)) = self.kicks.front().cloned() {
            let present = lobby
                .players
                .iter()
                .any(|p| p.name == name && p.slot_status == 2 && !p.is_self);
            if !present || now.saturating_sub(since) > KICK_PATIENCE {
                self.kicks.pop_front();
                continue;
            }
            let Some(target) = KickRequest::guest(lobby, &name) else {
                return false; // Not kickable yet; try again next tick.
            };
            if manual.request(target, engine, now).is_some() {
                self.kicks.pop_front();
                return true;
            }
            return false;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ds_report::DsStats;

    #[test]
    fn announcements_show_level_and_matches_or_na() {
        let found = Lookup::Found(DsStats { level: 52, matches: 310 });
        assert_eq!(announcement("A#1", found), "A#1 - Lv. 52 (310 matches)");
        assert_eq!(announcement("A#1", Lookup::NotFound), "A#1 - Lv. N/A");
    }

    #[test]
    fn chat_text_cannot_start_a_native_command_or_carry_control_characters() {
        assert_eq!(chat_safe("/x#1 - Lv. 1"), " /x#1 - Lv. 1");
        assert_eq!(chat_safe("a\nb"), "ab");
        assert!(chat_safe(&"é".repeat(300)).len() <= 240);
    }

    #[test]
    fn players_without_a_battletag_and_the_host_are_never_looked_up() {
        let mut check = DsCheck::default();
        let config = Config::default();
        assert!(!check.join(&config, None, "NoTag"));
        let lobby: Lobby = serde_json::from_value(json!({"isHost":true,"lobbyName":"T #1",
            "mapData":{"mapPath":"m.w3x"},"players":[{"name":"Host#1","slot":0,"slotStatus":2,
            "slotType":0,"team":0,"isSelf":true,"isObserver":false,"teamChangeEnabled":true}]}))
        .unwrap();
        assert!(!check.join(&config, Some(&lobby), "Host#1"));
        assert!(check.pending.is_empty());
    }

    #[test]
    fn a_failed_lookup_falls_back_to_the_welcome_and_never_kicks() {
        let mut check = DsCheck::default();
        let (events, received) = mpsc::channel();
        let config = Config { min_ds_level: 50, ..Config::default() };
        check.pending.insert("Net#1".into());
        check.sender.send(("Net#1".into(), Lookup::Failed)).unwrap();
        let mut announcements = Announcements::default();
        check.poll(&config, 1, &events, &mut announcements);
        assert!(check.kicks.is_empty() && check.messages.is_empty());
        assert!(received.try_recv().is_err());
    }

    #[test]
    fn a_rejected_player_is_queued_for_a_kick_and_an_accepted_one_for_an_announcement() {
        let mut check = DsCheck::default();
        let (events, received) = mpsc::channel();
        let config = Config { min_ds_level: 50, ..Config::default() };
        let mut announcements = Announcements::default();
        for (name, level) in [("Low#1", 10), ("High#2", 60)] {
            check.pending.insert(name.into());
            check
                .sender
                .send((name.into(), Lookup::Found(DsStats { level, matches: 5 })))
                .unwrap();
        }
        check.pending.insert("Ghost#3".into());
        check.sender.send(("Ghost#3".into(), Lookup::NotFound)).unwrap();
        check.poll(&config, 1, &events, &mut announcements);
        let kicked: Vec<_> = check.kicks.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(kicked, ["Low#1", "Ghost#3"]);
        assert_eq!(check.messages.len(), 1);
        assert_eq!(check.messages[0].0, "High#2");
        assert!(received.try_iter().count() >= 3);
    }

    #[test]
    fn with_the_filter_off_a_missing_player_is_announced_as_na_not_kicked() {
        let mut check = DsCheck::default();
        let (events, _received) = mpsc::channel();
        let mut announcements = Announcements::default();
        check.pending.insert("Ghost#3".into());
        check.sender.send(("Ghost#3".into(), Lookup::NotFound)).unwrap();
        check.poll(&Config::default(), 1, &events, &mut announcements);
        assert!(check.kicks.is_empty());
        assert_eq!(check.messages[0].1, "Ghost#3 - Lv. N/A");
    }

    #[test]
    fn leaving_or_a_new_lobby_discards_pending_work() {
        let mut check = DsCheck::default();
        check.pending.insert("A#1".into());
        check.messages.push_back(("A#1".into(), "x".into()));
        check.kicks.push_back(("A#1".into(), 0));
        check.left("A#1");
        assert!(check.pending.is_empty() && check.messages.is_empty() && check.kicks.is_empty());
        check.pending.insert("B#2".into());
        check.sender.send(("B#2".into(), Lookup::NotFound)).unwrap();
        check.reset();
        assert!(check.results.try_recv().is_err() && check.pending.is_empty());
    }
}
