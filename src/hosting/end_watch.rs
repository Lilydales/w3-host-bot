//! Player mode, Direct Strike maps only: watches the game window for the map's end panel
//! (VICTORY/DEFEAT with Quit) and exits the game, so the bot can host the next lobby.
//!
//! Clicking the map's Quit button turned out to be unreliable, so the bot leaves the game
//! the same way the observer host does (`LeaveGame`). Leaving skips the map's own Quit
//! handler, so W3C/FLO may record the result as a leave.
//!
//! Captures run on a short-lived thread every two seconds, only while a match is running.
//! Once the panel has been seen twice in a row and the delay has passed, the bot leaves;
//! if the game is still running 15 s later it tries once more.
use super::{Activity, Event};
use crate::{
    config::{Config, HostMode},
    end_panel,
    engine::{Engine, Phase},
    protocol::Command,
};
use std::sync::mpsc::{self, Receiver, Sender};

const INTERVAL: u64 = 2;
const RETRY_AFTER: u64 = 15;

#[derive(Default)]
pub(super) struct EndWatch {
    flight: Option<Receiver<Result<(f32, f32), String>>>,
    last_capture: Option<u64>,
    hits: u8,
    seen_at: Option<u64>,
    left_at: Option<u64>,
    retried: bool,
    reported_error: bool,
}

/// The next step for the watcher, given whether the panel is on screen. Pure, for tests.
#[derive(Debug, PartialEq)]
pub(super) enum Action {
    Wait,
    Leave,
}

impl EndWatch {
    fn active(config: &Config, engine: &Engine) -> bool {
        config.host_mode == HostMode::Player
            && config.auto_quit
            && config.is_direct_strike()
            && engine.phase == Phase::InGame
    }

    /// Records one capture result and decides what to do.
    pub(super) fn decide(&mut self, config: &Config, visible: bool, now: u64) -> Action {
        if !visible {
            *self = Self {
                last_capture: self.last_capture,
                ..Self::default()
            };
            return Action::Wait;
        }
        self.hits = self.hits.saturating_add(1);
        if self.hits < 2 {
            return Action::Wait;
        }
        let seen = *self.seen_at.get_or_insert(now);
        if now < seen + config.auto_quit_delay_seconds {
            return Action::Wait;
        }
        match self.left_at {
            None => {
                self.left_at = Some(now);
                Action::Leave
            }
            Some(at) if !self.retried && now >= at + RETRY_AFTER => {
                self.retried = true;
                Action::Leave
            }
            _ => Action::Wait,
        }
    }

    /// Called every controller tick. Returns `LeaveGame` when the last resort is due.
    pub fn step(&mut self, engine: &Engine, pid: u32, now: u64, tx: &Sender<Event>) -> Option<Command> {
        if !Self::active(&engine.config, engine) {
            if self.last_capture.is_some() || self.flight.is_some() {
                *self = Self::default();
            }
            return None;
        }
        let note = |line: String| {
            let _ = tx.send(Event::Activity(Activity::Command(line)));
        };
        if let Some(flight) = &self.flight {
            let result = match flight.try_recv() {
                Ok(result) => result,
                Err(mpsc::TryRecvError::Empty) => return None,
                Err(mpsc::TryRecvError::Disconnected) => Err("capture stopped".into()),
            };
            self.flight = None;
            let (quit, link) = match result {
                Ok(scores) => scores,
                Err(error) => {
                    if !std::mem::replace(&mut self.reported_error, true) {
                        note(format!("End panel watch · capture failed · {error}"));
                    }
                    return None;
                }
            };
            let visible = quit >= end_panel::THRESHOLD && link >= end_panel::THRESHOLD;
            if visible && self.hits == 0 {
                note(format!("End panel seen · match {quit:.2} / {link:.2}"));
            }
            match self.decide(&engine.config, visible, now) {
                Action::Wait => {}
                Action::Leave => {
                    note(if self.retried {
                        "End panel still open · exiting the game again".into()
                    } else {
                        "End panel · exiting the game".into()
                    });
                    return Some(Command::empty("LeaveGame"));
                }
            }
            return None;
        }
        if self.last_capture.is_some_and(|at| now < at + INTERVAL) {
            return None;
        }
        self.last_capture = Some(now);
        let (sender, receiver) = mpsc::channel();
        if std::thread::Builder::new()
            .name("ghost-end-panel".into())
            .spawn(move || {
                let result = crate::window_capture::capture(pid).map(|frame| end_panel::scores(&frame));
                let _ = sender.send(result);
            })
            .is_ok()
        {
            self.flight = Some(receiver);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_game_is_left_after_two_sightings_and_the_delay_then_retried_once() {
        let config = Config {
            auto_quit_delay_seconds: 5,
            ..Config::default()
        };
        let mut watch = EndWatch::default();
        assert_eq!(watch.decide(&config, true, 10), Action::Wait, "one sighting is not enough");
        assert_eq!(watch.decide(&config, true, 12), Action::Wait, "delay starts now");
        assert_eq!(watch.decide(&config, true, 16), Action::Wait);
        assert_eq!(watch.decide(&config, true, 17), Action::Leave);
        assert_eq!(watch.decide(&config, true, 25), Action::Wait);
        assert_eq!(watch.decide(&config, true, 32), Action::Leave, "one retry");
        assert_eq!(watch.decide(&config, true, 60), Action::Wait, "only once");
        // The panel closing resets everything.
        assert_eq!(watch.decide(&config, false, 62), Action::Wait);
        assert_eq!(watch.decide(&config, true, 64), Action::Wait);
    }

    #[test]
    fn only_direct_strike_maps_are_watched() {
        use std::path::PathBuf;
        let map = |name: &str| Config {
            maps: vec![PathBuf::from(format!(r"C:\Maps\{name}"))],
            ..Config::default()
        };
        assert!(map("5335_Direct Strike 6.5.13f.w3x").is_direct_strike());
        assert!(map("DirectStrike_7.w3x").is_direct_strike());
        assert!(map("direct_strike.w3m").is_direct_strike());
        assert!(!map("Legion TD.w3x").is_direct_strike());
        assert!(!Config::default().is_direct_strike());
    }
}
