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
    /// Kick reason per queued player, for chat and the launcher.
    reasons: std::collections::BTreeMap<String, String>,
    last_sent: Option<u64>,
    /// Answers for players still in the lobby, so a live filter change can re-check them.
    verdicts: std::collections::BTreeMap<String, Lookup>,
    /// The host's own BattleTag, looked up for display only (never filtered or kicked).
    own: Option<String>,
    /// The host's own lookup still running. Not cleared by `reset`, so a new lobby
    /// does not start a second lookup (and a second Activity line) while one is running.
    own_flight: Option<String>,
    /// Why a lookup ended as it did ("HTTP 403"...), shown in Activity for failures.
    notes: (Sender<(String, String)>, Receiver<(String, String)>),
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
            reasons: Default::default(),
            last_sent: None,
            verdicts: Default::default(),
            own: None,
            own_flight: None,
            notes: mpsc::channel(),
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
        // ds-report only knows Direct Strike; other maps get the ordinary welcome.
        if !config.is_direct_strike() || !name.contains('#')
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
        let (sender, notes, tag, patch) = (
            self.sender.clone(),
            self.notes.0.clone(),
            name.to_owned(),
            config.ds_patch_version.clone(),
        );
        let started = thread::Builder::new()
            .name("ghost-ds-lookup".into())
            .spawn(move || {
                let (result, note) = ds_report::lookup_explained(&tag, &patch);
                let _ = notes.send((tag.clone(), note));
                let _ = sender.send((tag, result));
            });
        if started.is_err() {
            return false;
        }
        self.pending.insert(name.to_owned());
        self.seen.insert(name.to_owned());
        true
    }

    /// Looks up the host's own stats once per lobby, so the launcher can show them and the
    /// host can see the lookup works. The answer is never used to kick or announce.
    pub fn lookup_self(&mut self, config: &Config, name: &str) {
        if !config.is_direct_strike()
            || !name.contains('#')
            || self.own.as_deref() == Some(name)
            || self.own_flight.as_deref() == Some(name)
        {
            return;
        }
        let (sender, notes, tag, patch) = (
            self.sender.clone(),
            self.notes.0.clone(),
            name.to_owned(),
            config.ds_patch_version.clone(),
        );
        if thread::Builder::new()
            .name("ghost-ds-lookup".into())
            .spawn(move || {
                let (result, note) = ds_report::lookup_explained(&tag, &patch);
                let _ = notes.send((tag.clone(), note));
                let _ = sender.send((tag, result));
            })
            .is_ok()
        {
            self.own = Some(name.to_owned());
            self.own_flight = Some(name.to_owned());
        }
    }

    pub fn left(&mut self, name: &str) {
        self.pending.remove(name);
        self.seen.remove(name);
        self.messages.retain(|(n, _)| n != name);
        self.kicks.retain(|(n, _)| n != name);
        self.reasons.remove(name);
        self.verdicts.remove(name);
    }

    /// New lobby or game: forget everything, including answers still on their way.
    pub fn reset(&mut self) {
        self.pending.clear();
        self.seen.clear();
        self.messages.clear();
        self.kicks.clear();
        self.reasons.clear();
        self.last_sent = None;
        self.verdicts.clear();
        self.own = None;
        while self.results.try_recv().is_ok() {}
    }

    /// A game started: nothing is kicked or announced any more, but lookups still running
    /// are kept so the Current/Last game panels get their levels.
    pub fn game_started(&mut self) {
        self.messages.clear();
        self.kicks.clear();
        self.reasons.clear();
        self.verdicts.clear();
        self.last_sent = None;
    }

    /// Collects finished lookups. Never blocks.
    pub fn poll(
        &mut self,
        config: &Config,
        now: u64,
        events: &Sender<Event>,
        announcements: &mut Announcements,
    ) {
        let mut notes = std::collections::BTreeMap::new();
        while let Ok((name, note)) = self.notes.1.try_recv() {
            notes.insert(name, note);
        }
        while let Ok((name, result)) = self.results.try_recv() {
            let note = notes.remove(&name).unwrap_or_default();
            if self.own_flight.as_deref() == Some(name.as_str()) && !self.pending.contains(&name) {
                // Reported once; counts as this lobby's lookup even if the lobby changed.
                self.own_flight = None;
                self.own = Some(name.clone());
                // Display only: the host is never filtered, kicked or announced.
                let detail = match result {
                    Lookup::Found(s) => format!("Lv. {} ({} matches)", s.level, s.matches),
                    Lookup::NotFound => "not found".into(),
                    Lookup::Failed => "lookup failed".into(),
                };
                let _ = events.send(Event::Activity(Activity::Command(format!(
                    "DS lookup for you · {name} · {detail} · {note}"
                ))));
                let _ = events.send(match result {
                    Lookup::Failed => Event::DsFailed(name),
                    _ => Event::DsStats(name, result.stats()),
                });
                continue;
            }
            if !self.pending.remove(&name) {
                continue; // The player left, or the lobby changed, while it ran.
            }
            if !matches!(result, Lookup::Found(_)) {
                let _ = events.send(Event::Activity(Activity::Command(format!(
                    "DS lookup · {name} · {} · {note}",
                    if result == Lookup::Failed { "failed" } else { "not found" }
                ))));
            }
            if result == Lookup::Failed {
                // No data and no verdict: fall back to the ordinary welcome.
                let _ = events.send(Event::DsFailed(name.clone()));
                announcements.joined(&name);
                continue;
            }
            let _ = events.send(Event::DsStats(name.clone(), result.stats()));
            self.verdicts.insert(name.clone(), result);
            if ds_report::rejects(config.min_ds_level, config.min_ds_matches, result) {
                let detail = match result.stats() {
                    Some(s) => format!("Lv. {} ({} matches)", s.level, s.matches),
                    None => "not found on W3Champions".into(),
                };
                let _ = events.send(Event::Activity(Activity::Command(format!(
                    "DS filter · {name} · {detail} · removing"
                ))));
                self.reasons.insert(
                    name.clone(),
                    ds_report::reject_reason(config.min_ds_level, config.min_ds_matches, result),
                );
                self.kicks.push_back((name, now));
            } else {
                let text = announcement(&name, result);
                self.messages.push_back((name, text));
            }
        }
    }

    /// The filter minimums changed while hosting: queue anyone already checked who no
    /// longer passes. Players who now pass are left alone (they were never removed).
    pub fn refilter(&mut self, config: &Config, now: u64, events: &Sender<Event>) {
        for (name, result) in &self.verdicts {
            if self.kicks.iter().any(|(queued, _)| queued == name)
                || !ds_report::rejects(config.min_ds_level, config.min_ds_matches, *result)
            {
                continue;
            }
            let _ = events.send(Event::Activity(Activity::Command(format!(
                "DS filter changed · {name} no longer passes · removing"
            ))));
            self.reasons.insert(
                name.clone(),
                ds_report::reject_reason(config.min_ds_level, config.min_ds_matches, *result),
            );
            self.kicks.push_back((name.clone(), now));
        }
        // Players who now pass should not be kicked from an older, stricter queue.
        let reasons = &mut self.reasons;
        self.kicks.retain(|(name, _)| {
            let keep = self.verdicts.get(name).is_none_or(|r| {
                ds_report::rejects(config.min_ds_level, config.min_ds_matches, *r)
            });
            if !keep {
                reasons.remove(name);
            }
            keep
        });
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
                if let Some(reason) = self.reasons.remove(&name) {
                    manual.set_reason(reason);
                }
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
        let all: Vec<_> = received.try_iter().collect();
        assert!(all.iter().any(|e| matches!(e, Event::DsFailed(n) if n == "Net#1")));
        assert!(!all.iter().any(|e| matches!(e, Event::DsStats(..))));
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

    #[test]
    fn a_stricter_live_filter_queues_players_already_in_the_lobby() {
        let mut check = DsCheck::default();
        let (events, received) = mpsc::channel();
        check.pending.insert("Low#1".into());
        check.pending.insert("High#2".into());
        check.sender.send(("Low#1".into(), Lookup::Found(ds_report::DsStats { level: 3, matches: 10 }))).unwrap();
        check.sender.send(("High#2".into(), Lookup::Found(ds_report::DsStats { level: 30, matches: 500 }))).unwrap();
        let mut announcements = Announcements::default();
        check.poll(&Config::default(), 1, &events, &mut announcements);
        assert!(check.kicks.is_empty(), "the filter is off at first");

        let strict = Config { min_ds_level: 10, ..Config::default() };
        check.refilter(&strict, 5, &events);
        let queued: Vec<_> = check.kicks.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(queued, ["Low#1"]);
        assert!(check.reasons["Low#1"].contains("level 3 is below 10"));
        check.refilter(&strict, 6, &events);
        assert_eq!(check.kicks.len(), 1, "never queued twice");

        // Relaxing the filter again cancels the queued kick.
        check.refilter(&Config::default(), 7, &events);
        assert!(check.kicks.is_empty() && check.reasons.is_empty());
        assert!(received.try_iter().count() >= 1);
    }

    #[test]
    fn lookups_running_when_the_game_starts_still_report_levels() {
        let mut check = DsCheck::default();
        let (events, received) = mpsc::channel();
        check.pending.insert("Late#1".into());
        check.kicks.push_back(("Old#2".into(), 0));
        check.game_started();
        assert!(check.kicks.is_empty() && check.pending.contains("Late#1"));
        check
            .sender
            .send(("Late#1".into(), Lookup::Found(ds_report::DsStats { level: 9, matches: 40 })))
            .unwrap();
        check.poll(&Config::default(), 1, &events, &mut Announcements::default());
        assert!(received.try_iter().any(|e| matches!(e, Event::DsStats(n, Some(_)) if n == "Late#1")));
    }

    #[test]
    fn the_hosts_own_lookup_is_shown_but_never_kicked_or_announced() {
        let mut check = DsCheck::default();
        let (events, received) = mpsc::channel();
        check.own = Some("Me#1".into());
        check.own_flight = Some("Me#1".into());
        check.sender.send(("Me#1".into(), Lookup::NotFound)).unwrap();
        let strict = Config { min_ds_level: 50, ..Config::default() };
        check.poll(&strict, 1, &events, &mut Announcements::default());
        assert!(check.kicks.is_empty() && check.messages.is_empty());
        let all: Vec<_> = received.try_iter().collect();
        assert!(all.iter().any(|e| matches!(e, Event::DsStats(n, None) if n == "Me#1")));
    }

    #[test]
    fn a_new_lobby_does_not_repeat_the_hosts_lookup_line() {
        let mut check = DsCheck::default();
        let (events, received) = mpsc::channel();
        // Lookup started, then the lobby was (re)created before the answer arrived.
        check.own = Some("Me#1".into());
        check.own_flight = Some("Me#1".into());
        check.reset();
        let config = Config::default(); // not Direct Strike: lookup_self would not spawn anyway
        check.lookup_self(&config, "Me#1");
        // A stray second answer (from an older lookup) must not be shown twice.
        check.sender.send(("Me#1".into(), Lookup::NotFound)).unwrap();
        check.sender.send(("Me#1".into(), Lookup::NotFound)).unwrap();
        check.poll(&config, 1, &events, &mut Announcements::default());
        let lines = received
            .try_iter()
            .filter(|e| matches!(e, Event::Activity(Activity::Command(l)) if l.starts_with("DS lookup for you")))
            .count();
        assert_eq!(lines, 1);
    }
}
