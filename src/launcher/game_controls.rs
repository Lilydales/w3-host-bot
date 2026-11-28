//! Status-bar game controls: Leave game (and host the next lobby), Exit Warcraft III,
//! "Exit Warcraft III on Stop", and bringing the launcher back to the front after
//! Battle.net starts the game.
use super::{
    app::{App, Message},
    process::GameState,
};
use ghost_reforged::hosting::Control;
use iced::Task;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

/// A second click within this time confirms a destructive in-game action.
pub const CONFIRM: Duration = Duration::from_secs(5);
/// How long after Host the launcher watches for Battle.net taking the foreground.
const REFOCUS_LIMIT: Duration = Duration::from_secs(120);
/// How long Warcraft III gets to close by itself before it is ended.
const EXIT_GRACE: Duration = Duration::from_secs(6);

/// Background job bringing the launcher back after a game launch.
pub struct Refocus {
    cancel: Arc<AtomicBool>,
    done: mpsc::Receiver<u32>,
}

impl Drop for Refocus {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

/// Whether a two-click confirmation is still waiting for its second click.
pub fn armed(at: Option<Instant>) -> bool {
    at.is_some_and(|at| at.elapsed() < CONFIRM)
}

pub fn game_running(state: GameState) -> bool {
    matches!(state, GameState::Background | GameState::Visible | GameState::Multiple)
}

impl App {
    /// Handles the game-control messages. Returns `None` for everything else, and also
    /// for `Stop`, which only records the exit request here and is handled as usual.
    pub(super) fn game_message(&mut self, message: &Message) -> Option<Task<Message>> {
        match message {
            Message::LeaveGame => {
                self.exit_armed = None;
                if !self.in_game || self.worker.is_none() {
                    self.leave_armed = None;
                    return Some(Task::none());
                }
                if armed(self.leave_armed) {
                    self.leave_armed = None;
                    if let Some(worker) = &self.worker {
                        worker.send(Control::LeaveGame);
                    }
                    self.log("Leave game clicked · leaving the match.");
                    self.notify(Ok(()), "Leaving the match…");
                } else {
                    self.leave_armed = Some(Instant::now());
                }
                Some(Task::none())
            }
            Message::ExitGame => {
                self.leave_armed = None;
                if self.exiting_game {
                    return Some(Task::none());
                }
                // In a match, closing the game needs a second click.
                if self.in_game && !armed(self.exit_armed) {
                    self.exit_armed = Some(Instant::now());
                    return Some(Task::none());
                }
                self.exit_armed = None;
                if self.worker.is_some() {
                    // Stop first, or hosting would start the game again.
                    self.exit_after_stop = true;
                    return Some(Task::done(Message::Stop));
                }
                Some(self.exit_game_task())
            }
            Message::GameExited(result) => {
                self.exiting_game = false;
                match result {
                    Ok(line) => {
                        self.log(line);
                        self.notify(Ok(()), line);
                    }
                    Err(error) => {
                        self.log(error);
                        self.notify(Err(error.clone()), "");
                    }
                }
                Some(Task::none())
            }
            Message::ExitOnStop(value) => {
                self.form.profile.exit_game_on_stop = *value;
                let result = self.save();
                if result.is_err() {
                    self.notify(result, "");
                }
                Some(Task::none())
            }
            Message::RefocusAfterLaunch(value) => {
                self.form.profile.refocus_after_launch = *value;
                Some(Task::none())
            }
            Message::Stop => {
                self.leave_armed = None;
                self.exit_armed = None;
                self.refocus = None;
                if self.form.profile.exit_game_on_stop && self.worker.is_some() {
                    self.exit_after_stop = true;
                }
                None
            }
            Message::PollHost => {
                self.poll_refocus();
                None
            }
            _ => None,
        }
    }

    fn exit_game_task(&mut self) -> Task<Message> {
        self.exiting_game = true;
        self.notify(Ok(()), "Closing Warcraft III…");
        Task::perform(
            async { ghost_reforged::game_control::exit_game(EXIT_GRACE) },
            Message::GameExited,
        )
    }

    /// After hosting has fully stopped, closes the game if Stop asked for it.
    pub(super) fn exit_after_stop_task(&mut self) -> Option<Task<Message>> {
        if self.worker.is_some() || !std::mem::take(&mut self.exit_after_stop) {
            return None;
        }
        Some(self.exit_game_task())
    }

    /// Starts watching for Battle.net / the new game window taking the foreground and
    /// brings the launcher back. Only when the game is not already running.
    pub(super) fn start_refocus(&mut self) {
        let profile = &self.form.profile;
        // Observer hosting keeps Battle.net and the game hidden; nothing to take back.
        // Battle.net may need the user (login screen): leave it in front then.
        let signed_out = self.battlenet == Some(ghost_reforged::battlenet::State::SignedOut);
        if signed_out || !profile.player_host || !profile.refocus_after_launch || game_running(self.game) || self.hidden {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let (sender, done) = mpsc::channel();
        let flag = cancel.clone();
        let spawned = std::thread::Builder::new()
            .name("ghost-refocus".into())
            .spawn(move || {
                let count = ghost_reforged::game_control::reclaim_focus_after_launch(REFOCUS_LIMIT, &flag);
                let _ = sender.send(count);
            });
        if spawned.is_ok() {
            self.refocus = Some(Refocus { cancel, done });
        }
    }

    fn poll_refocus(&mut self) {
        let Some(job) = &self.refocus else {
            return;
        };
        match job.done.try_recv() {
            Ok(count) => {
                self.refocus = None;
                if count > 0 {
                    self.log("Game launched · GHost Reforged brought back to the front");
                }
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => self.refocus = None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_confirmation_click_expires() {
        assert!(!armed(None));
        assert!(armed(Some(Instant::now())));
        let old = Instant::now().checked_sub(CONFIRM + Duration::from_secs(1));
        if let Some(old) = old {
            assert!(!armed(Some(old)));
        }
    }

    #[test]
    fn exit_is_offered_only_while_the_game_runs() {
        assert!(game_running(GameState::Visible));
        assert!(game_running(GameState::Background));
        assert!(game_running(GameState::Multiple));
        assert!(!game_running(GameState::Stopped));
        assert!(!game_running(GameState::Checking));
        assert!(!game_running(GameState::Unavailable));
    }
}
