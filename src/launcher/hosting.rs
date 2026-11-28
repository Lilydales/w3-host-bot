use super::app::App;
use ghost_reforged::{
    config::{Config, HostMode, MapSettings, Observers},
    hosting::{Activity, Control, Event, Worker},
};
use std::time::{Duration, Instant};

/// Player-host lobbies with 0 observer slots are created with observers off, so the
/// game shows no observer team at all instead of a column of closed slots.
/// The observer-host mode needs an observer slot for the bot, so it keeps "Full".
pub(super) fn map_settings(player_host: bool, observer_slots: u8) -> MapSettings {
    MapSettings {
        observers: if player_host && observer_slots == 0 {
            Observers::None
        } else {
            Observers::Full
        },
        ..MapSettings::default()
    }
}

impl App {
    pub(super) fn start_hosting(&mut self) -> Result<(), String> {
        self.save()?;
        let p = self.form.read()?;
        let rehost = p.rehosts(); // before fields are moved out of `p`
        let map = p.map.ok_or("Select a Warcraft III map first.")?;
        let map = super::maps::validate(&map)?;
        let config = Config {
            maps: vec![map],
            host_mode: if p.player_host {
                HostMode::Player
            } else {
                HostMode::Observer
            },
            count_host: p.player_host,
            ds_patch_version: p.ds_patch_version,
            min_ds_level: p.min_ds_level,
            min_ds_matches: p.min_ds_matches,
            observer_slots: p.observer_slots.min(ghost_reforged::config::MAX_OBSERVER_SLOTS),
            lobby_name: p.lobby_pattern,
            first_number: p.next_number,
            min_players: p.players,
            game_speed: p.speed.wire(),
            require_full: p.require_full,
            rehost_delay_seconds: p.rehost_delay_seconds,
            empty_rehost_seconds: p.empty_rehost_seconds,
            auto_start: true,
            commands: p.commands,
            stats_path: Some(self.data_directory.join("players.json")),
            ban_path: Some(self.ban_path()),
            welcome: p.welcome,
            resume_lobby: true,
            // LobbyStart owns the game's native countdown. Never add a second one.
            countdown_seconds: 0,
            leave_on_load: p.rehost && !p.player_host,
            restart_on_load: p.rehost && !p.player_host,
            // Player and observer hosts keep separate rehost choices (`Profile::rehosts`).
            private_game: p.private_game,
            max_games: if rehost { 0 } else { 1 },
            map_settings: map_settings(p.player_host, p.observer_slots),
            auto_quit: p.player_host && p.auto_quit,
            auto_quit_delay_seconds: p.auto_quit_delay_seconds,
            leave_if_quit_fails: p.leave_if_quit_fails,
            focus_on_start: p.player_host && p.focus_on_start,
            seat: if p.player_host { p.seat } else { Default::default() },
            ..Config::default()
        };
        self.worker = Some(Worker::start(config)?);
        self.hosting_requested = true;
        self.last_reserved = None;
        self.lobby = None;
        self.hosting_status = "Connecting…".into();
        self.log("Auto Host started.");
        Ok(())
    }

    pub(super) fn reserve_number(&mut self, number: u32) -> Result<(), String> {
        let next = number.checked_add(1).ok_or("Lobby number exhausted.")?;
        let mut updated = self.profiles.clone();
        // An acknowledgement can be lost after the atomic save succeeded.
        if self.last_reserved == Some(number) && updated.items[updated.selected].next_number == next
        {
            return Ok(());
        }
        if updated.items[updated.selected].next_number != number {
            return Err("The lobby counter changed unexpectedly.".into());
        }
        updated.items[updated.selected].next_number = next;
        self.store
            .as_ref()
            .ok_or("Profile storage is unavailable.")?
            .save(&updated)
            .map_err(|e| e.to_string())?;
        self.profiles = updated;
        self.form.number = next.to_string();
        self.form.profile.next_number = next;
        self.last_reserved = Some(number);
        Ok(())
    }

    pub(super) fn poll_host(&mut self) {
        self.poll_replay();
        if self.settings_due.is_some_and(|due| Instant::now() >= due) {
            self.settings_due = None;
            self.apply_settings();
        }
        for _ in 0..64 {
            let Some(event) = self.worker.as_ref().and_then(Worker::receive) else {
                break;
            };
            match event {
                Event::ClientChanged(pid) => self.preview.target(pid),
                Event::VisualChanged => self.preview.request(),
                Event::Activity(event) => {
                    self.preview.request();
                    self.record_activity(event);
                }
                Event::Status(text) => {
                    self.preview.request();
                    self.hosting_status = text.clone();
                    self.log(&text);
                }
                Event::Lobby(lobby) => {
                    self.last_roster = (lobby.name.clone(), super::ban_list::guests(&lobby));
                    if let Some(me) = lobby.slots.iter().find(|s| s.own && !s.name.trim().is_empty()) {
                        self.own_name = Some(me.name.clone());
                    }
                    self.in_game = false;
                    self.lobby = Some(lobby);
                }
                Event::LobbyCleared => self.lobby = None,
                Event::Kicked(name, reason) => {
                    self.sound(super::sounds::Sound::PlayerKicked);
                    self.record_kick(name, reason)
                }
                Event::MatchEnded => {
                    self.sound(super::sounds::Sound::GameEnded);
                    self.match_ended()
                }
                Event::Chat { sender, text, own } => {
                    if !own {
                        self.sound(super::sounds::Sound::Chat);
                    }
                    if self.side_tab != super::app::SideTab::Chat {
                        self.chat_unread += 1;
                    }
                    let seconds = self.started.elapsed().as_secs();
                    self.chat_lines.push_front(super::app::ChatLine {
                        time: format!("{:02}:{:02}", seconds / 60, seconds % 60),
                        sender,
                        text,
                        own,
                    });
                    self.chat_lines.truncate(super::app::CHAT_LINES);
                }
                Event::DsStats(name, stats) => {
                    self.ds_failed.remove(&name);
                    self.ds_stats.insert(name, stats);
                }
                Event::DsFailed(name) => {
                    self.ds_failed.insert(name);
                }
                Event::Reserve(number) => {
                    let result = self.reserve_number(number);
                    if let Some(worker) = &self.worker {
                        worker.send(Control::Reserved {
                            number,
                            saved: result.is_ok(),
                        });
                    }
                    if result.is_err() {
                        self.notify(result, "");
                    }
                }
                Event::Finished(result) => {
                    self.hosting_requested = false;
                    self.preview.target(None);
                    self.worker = None;
                    self.lobby = None;
                    self.in_game = false;
                    self.game_started_at = None;
                    self.settings_open = !self.settings_complete();
                    self.ds_stats.clear();
                    let explained = self.hosting_status.starts_with("Hosting stopped");
                    self.hosting_status = if explained {
                        std::mem::take(&mut self.hosting_status)
                    } else if result.is_ok() {
                        "Stopped".to_string()
                    } else {
                        "Stopped with an error".to_string()
                    };
                    if let Err(error) = &result {
                        self.log(error);
                    }
                    let done = if explained {
                        self.hosting_status.clone()
                    } else {
                        "Auto Host stopped.".to_string()
                    };
                    self.notify(result, &done);
                    break;
                }
            }
        }
    }

    pub(super) fn queue_settings(&mut self) {
        self.notice.clear();
        self.notice_error = false;
        if self.worker.is_some() {
            self.settings_due = Some(Instant::now() + Duration::from_millis(500));
        }
    }

    /// The host's match ended. A short match means Direct Strike ended it because
    /// someone left early; the launcher then offers one-click "left early" bans.
    pub(super) fn match_ended(&mut self) {
        self.in_game = false;
        self.read_replay();
        let Some(started) = self.game_started_at.take() else {
            return;
        };
        let seconds = started.elapsed().as_secs();
        let length = format!("{}:{:02}", seconds / 60, seconds % 60);
        if super::ban_list::ended_early(seconds)
            && let Some(game) = &mut self.last_game
        {
            game.ended_early = Some(seconds);
            self.log_line(
                &format!("Match ended early after {length} · someone left before 7 minutes"),
                true,
            );
        } else {
            self.log(&format!("Match finished after {length}"));
        }
    }

    pub(super) fn record_kick(&mut self, name: String, reason: String) {
        let seconds = self.started.elapsed().as_secs();
        self.log(&format!("Kicked · {name} · {reason}"));
        self.recent_kicks.push_front(super::app::KickEntry {
            time: format!("{:02}:{:02}", seconds / 60, seconds % 60),
            name,
            reason,
        });
        self.recent_kicks.truncate(8);
    }

    pub(super) fn record_activity(&mut self, event: Activity) {
        use super::sounds::Sound;
        match &event {
            Activity::LobbyCreated(_) => self.sound(Sound::LobbyCreated),
            Activity::Joined(_) => self.sound(Sound::PlayerJoined),
            Activity::Left(_) => self.sound(Sound::PlayerLeft),
            Activity::GameStarted(_) => self.sound(Sound::GameStarted),
            Activity::Command(_) => {}
        }
        match event {
            Activity::LobbyCreated(name) => {
                // Keep the host's own stats and the last game's, which its card still shows.
                let own = self.own_name.clone();
                let last: Vec<String> = self
                    .last_game
                    .as_ref()
                    .map(|game| game.players.clone())
                    .unwrap_or_default();
                self.ds_stats.retain(|player, _| {
                    own.as_deref() == Some(player.as_str()) || last.contains(player)
                });
                self.log_line(&format!("Lobby created · {name}"), true)
            }
            Activity::GameStarted(name) => {
                self.in_game = true;
                self.game_started_at = Some(Instant::now());
                self.game_started_wall = Some(std::time::SystemTime::now());
                self.replay_missing = false;
                // Keep the levels of the players who are now in the game.
                let own = self.own_name.clone();
                self.ds_stats.retain(|player, _| {
                    self.last_roster.1.contains(player) || own.as_deref() == Some(player.as_str())
                });
                // Remember who played so they can be banned after the match.
                if self.last_roster.0 == name && !self.last_roster.1.is_empty() {
                    self.last_game = Some(super::ban_list::LastGame {
                        lobby: name.clone(),
                        players: self.last_roster.1.clone(),
                        ..Default::default()
                    });
                }
                self.log_line(&format!("Game started · {name}"), true)
            }
            Activity::Joined(name) => self.log(&format!("{name} joined")),
            Activity::Left(name) => {
                self.ds_stats.remove(&name);
                self.log(&format!("{name} left"))
            }
            Activity::Command(line) => self.log(&line),
        }
    }

    pub(super) fn apply_settings(&mut self) {
        if self.worker.is_none() {
            return;
        }
        let result = self.form.read().and_then(|profile| {
            let settings = profile.live_settings();
            settings.validate()?;
            self.save()?;
            if let Some(worker) = &self.worker {
                worker.send(Control::Settings(settings));
            }
            Ok(())
        });
        self.notify(result, "");
    }
}
