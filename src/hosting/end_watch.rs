//! Player mode: watches the game window for Direct Strike's end panel and presses its Quit
//! button, so the results screen appears and the bot can host the next lobby.
//!
//! Captures run on a short-lived thread every two seconds, only while a match is running.
//! Order of attempts once the panel has been seen twice in a row and the delay has passed:
//! a posted click (mouse untouched), then a real click 4 s later, then, only when enabled,
//! leaving the game 15 s later. Leaving skips the map's Quit, so the result may count as a leave.
use super::{Activity, Event};
use crate::{
    config::{Config, HostMode},
    end_panel,
    engine::{Engine, Phase},
    protocol::Command,
};
use std::sync::mpsc::{self, Receiver, Sender};

const INTERVAL: u64 = 2;
const REAL_CLICK_AFTER: u64 = 4;
const LEAVE_AFTER: u64 = 15;

#[derive(Default)]
pub(super) struct EndWatch {
    flight: Option<Receiver<Result<(f32, f32), String>>>,
    last_capture: Option<u64>,
    hits: u8,
    seen_at: Option<u64>,
    clicked_at: Option<u64>,
    real_click: bool,
    left: bool,
    reported_error: bool,
}

/// The next step for the watcher, given whether the panel is on screen. Pure, for tests.
#[derive(Debug, PartialEq)]
pub(super) enum Action {
    Wait,
    PostClick,
    RealClick,
    Leave,
}

impl EndWatch {
    fn active(config: &Config, engine: &Engine) -> bool {
        config.host_mode == HostMode::Player && config.auto_quit && engine.phase == Phase::InGame
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
        match self.clicked_at {
            None => {
                self.clicked_at = Some(now);
                Action::PostClick
            }
            Some(at) if !self.real_click && now >= at + REAL_CLICK_AFTER => {
                self.real_click = true;
                Action::RealClick
            }
            Some(at) if config.leave_if_quit_fails && !self.left && now >= at + LEAVE_AFTER => {
                self.left = true;
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
                Action::PostClick => {
                    note("End panel · clicking Quit".into());
                    if let Err(error) = end_panel::post_click(pid) {
                        note(format!("End panel · click failed · {error}"));
                    }
                }
                Action::RealClick => {
                    note("End panel still open · clicking Quit with the mouse".into());
                    if let Err(error) = end_panel::input_click(pid) {
                        note(format!("End panel · click failed · {error}"));
                    }
                }
                Action::Leave => {
                    note("End panel still open · leaving the game (may count as a leave)".into());
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
    fn quit_is_clicked_after_two_sightings_and_the_delay_then_retried_then_left_only_if_enabled() {
        let mut config = Config {
            auto_quit_delay_seconds: 5,
            ..Config::default()
        };
        let mut watch = EndWatch::default();
        assert_eq!(watch.decide(&config, true, 10), Action::Wait, "one sighting is not enough");
        assert_eq!(watch.decide(&config, true, 12), Action::Wait, "delay starts now");
        assert_eq!(watch.decide(&config, true, 16), Action::Wait);
        assert_eq!(watch.decide(&config, true, 17), Action::PostClick);
        assert_eq!(watch.decide(&config, true, 19), Action::Wait);
        assert_eq!(watch.decide(&config, true, 21), Action::RealClick);
        assert_eq!(watch.decide(&config, true, 40), Action::Wait, "leaving is off by default");
        config.leave_if_quit_fails = true;
        assert_eq!(watch.decide(&config, true, 41), Action::Leave);
        assert_eq!(watch.decide(&config, true, 50), Action::Wait, "only once");
        // The panel closing resets everything.
        assert_eq!(watch.decide(&config, false, 52), Action::Wait);
        assert_eq!(watch.decide(&config, true, 54), Action::Wait);
    }
}
