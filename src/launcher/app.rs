use super::{
    preview::Preview,
    process::{self, GameState},
    profile::{GameSpeed, Profile, Profiles},
    storage::Store,
};
use ghost_reforged::hosting::{Control, Snapshot, Worker};
use iced::{Subscription, Task};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Page {
    Hosting,
    Profiles,
    Activity,
    Options,
}

#[derive(Debug, Clone, Copy)]
pub enum Field {
    Title,
    Pattern,
    Number,
    Players,
    RehostDelay,
    EmptyRefresh,
    Welcome,
    DsPatch,
    DsLevel,
    DsMatches,
}

#[derive(Debug, Clone)]
pub enum Message {
    UpdateChecked(Result<Option<super::updates::Release>, String>),
    InstallUpdate,
    DismissUpdate,
    UpdateDownloaded(Result<super::updates::Download, String>),
    Tray(super::tray::Event),
    QuitRequested(iced::window::Id),
    Window(WindowAction),
    PollBackdrop,
    WindowFocus(bool),
    Animate(Instant),
    Menu(super::motion::Menu),
    DismissMenus,
    Page(Page),
    Edit(Field, String),
    Command(ghost_reforged::commands::ChatCommand, bool),
    Welcome(bool),
    Rehost(bool),
    Private(bool),
    PlayerHost(bool),
    ObserverSlots(u8),
    AutoQuit(bool),
    AutoQuitDelay(u64),
    FocusOnStart(bool),
    Speed(GameSpeed),
    RequireFull(bool),
    MapName,
    OpenPreview,
    ClosePreview,
    Host,
    Stop,
    Start,
    Slot(ghost_reforged::hosting::SlotRequest),
    Kick(ghost_reforged::hosting::KickRequest),
    /// Opens the inline ban confirmation; nothing is banned yet.
    Ban(ghost_reforged::hosting::KickRequest),
    BanReason(String),
    ConfirmBan,
    CancelBan,
    /// Options page: BattleTag typed into the manual ban box.
    BanFormTag(String),
    BanFormReason(String),
    BanFormSubmit,
    /// Ban a player from the last started game.
    BanLastPlayer(String),
    /// Ban a player of an early-ended match with the "Left the game early" reason.
    BanEarlyLeaver(String),
    /// Show or hide the host settings on the Hosting page.
    ToggleHostSettings,
    /// Show every recent kick instead of the latest three.
    ToggleKicks,
    /// Show or hide the Game View panel under the lobby.
    TogglePreview,
    /// Choose the Warcraft III replay folder.
    PickReplayFolder,
    ReplayFolderPicked(Result<Option<PathBuf>, String>),
    /// Forget the chosen replay folder and find it automatically.
    AutoReplayFolder,
    EarlyLeaveRule(super::profile::EarlyLeaveRule),
    EarlyLeaveMinutes(u32),
    EndBufferMinutes(u32),
    PollHost,
    CloseRequested(iced::window::Id),
    Browse,
    Picked(Result<Option<PathBuf>, String>),
    Save,
    SelectProfile(usize),
    NewProfile,
    PollProcess,
    ProcessObserved(process::Observation),
    /// Battle.net running / signed-in state, polled with the game process.
    BattleNetObserved(ghost_reforged::battlenet::State),
    /// Bring up (or start) the Battle.net app so the user can sign in.
    OpenBattleNet,
    /// Leave the running match and host the next lobby (click twice to confirm).
    LeaveGame,
    /// Close Warcraft III (stops hosting first; click twice during a match).
    ExitGame,
    GameExited(Result<String, String>),
    /// Status bar: Stop Hosting also closes Warcraft III.
    ExitOnStop(bool),
    /// Options: bring the launcher back after Battle.net starts the game.
    RefocusAfterLaunch(bool),
    /// Player mode, Direct Strike: where the host sits in each lobby.
    Seat(ghost_reforged::config::Seat),
    /// Left column: Recent activity or Lobby chat.
    SideTab(SideTab),
    ChatInput(String),
    ChatSend,
    PlaySounds(bool),
    /// Close dialog (X button): minimise to tray, quit, or cancel.
    CloseToTray,
    CloseQuit,
    CloseCancel,
    CloseRemember(bool),
    /// Options: forget the remembered close choice and ask again.
    CloseAskAgain,
    OpenSoundsFolder,
    ClearLog,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SideTab {
    #[default]
    Activity,
    Chat,
}

/// One lobby chat line shown in the launcher.
#[derive(Debug, Clone)]
pub struct ChatLine {
    pub time: String,
    pub sender: String,
    pub text: String,
    pub own: bool,
}

/// Chat lines kept in memory.
pub const CHAT_LINES: usize = 200;

#[derive(Debug, Clone, Copy)]
pub enum WindowAction {
    Drag,
    Minimize,
    Maximize,
    Close,
}

pub struct Form {
    pub profile: Profile,
    pub number: String,
    pub players: String,
    pub rehost_delay: String,
    pub empty_refresh: String,
    pub min_ds_level: String,
    pub min_ds_matches: String,
}

impl Form {
    pub fn from(profile: Profile) -> Self {
        Self {
            number: profile.next_number.to_string(),
            players: profile.players.to_string(),
            rehost_delay: profile.rehost_delay_seconds.to_string(),
            empty_refresh: profile.empty_rehost_seconds.to_string(),
            min_ds_level: profile.min_ds_level.to_string(),
            min_ds_matches: profile.min_ds_matches.to_string(),
            profile,
        }
    }

    pub fn read(&self) -> Result<Profile, String> {
        let mut p = self.profile.clone();
        p.next_number = self
            .number
            .parse()
            .map_err(|_| "Next number must be a positive integer.")?;
        if !p.require_full {
            p.players = self
                .players
                .parse()
                .map_err(|_| "Minimum players must be a whole number.")?;
        }
        p.rehost_delay_seconds = self
            .rehost_delay
            .parse()
            .map_err(|_| "Rehost delay must be a whole number.")?;
        p.empty_rehost_seconds = self
            .empty_refresh
            .parse()
            .map_err(|_| "Empty lobby refresh must be a whole number.")?;
        p.min_ds_level = self
            .min_ds_level
            .trim()
            .parse()
            .map_err(|_| "Minimum Direct Strike level must be a whole number (0 = off).")?;
        p.min_ds_matches = self
            .min_ds_matches
            .trim()
            .parse()
            .map_err(|_| "Minimum Direct Strike matches must be a whole number (0 = off).")?;
        p.auto_start = true;
        p.validate()?;
        Ok(p)
    }
}

/// One removed player, shown under "Recent kicks" on the Hosting page.
pub struct KickEntry {
    pub time: String,
    pub name: String,
    pub reason: String,
}

pub struct LogEntry {
    pub time: String,
    pub message: String,
    pub highlight: bool,
}

pub struct App {
    pub updates: super::updates::State,
    pub(super) data_directory: PathBuf,
    pub(super) last_reserved: Option<u32>,
    pub(super) hosting_requested: bool,
    pub(super) audio: ghost_reforged::background::Silence,
    pub hidden: bool,
    pub tray_available: bool,
    pub backdrop: super::backdrop::Backdrop,
    pub focused: bool,
    pub motion: super::motion::Motion,
    pub page: Page,
    pub form: Form,
    pub profiles: Profiles,
    pub notice: String,
    pub notice_error: bool,
    pub logs: VecDeque<LogEntry>,
    pub preview: Preview,
    pub picking: bool,
    pub store: Option<Store>,
    pub game: GameState,
    /// Last known Battle.net state; `None` until the first check.
    pub battlenet: Option<ghost_reforged::battlenet::State>,
    pub worker: Option<Worker>,
    pub lobby: Option<Snapshot>,
    /// Direct Strike lookups for players in the current lobby (`None` = not on W3Champions).
    pub ds_stats: std::collections::HashMap<String, Option<ghost_reforged::ds_report::DsStats>>,
    /// Players whose ds-report lookup failed; shown as "Lookup failed", not "Checking…".
    pub ds_failed: std::collections::HashSet<String>,
    /// The player a Ban click is waiting to confirm, with the reason typed so far.
    pub ban_prompt: Option<(ghost_reforged::hosting::KickRequest, String)>,
    /// Manual ban box on the Options page.
    pub ban_form_tag: String,
    pub ban_form_reason: String,
    /// Guests of the latest lobby snapshot, kept so a started game can be remembered.
    pub(super) last_roster: (String, Vec<String>),
    /// Players of the most recently started game.
    pub last_game: Option<super::ban_list::LastGame>,
    /// Names banned from this launcher session (lowercase), to disable repeat buttons.
    pub(super) recent_bans: std::collections::HashSet<String>,
    pub(super) ban_file: PathBuf,
    /// Newest first, at most 8, kept for the launcher session.
    pub recent_kicks: VecDeque<KickEntry>,
    /// A game started from our lobby is being played (cleared by the next lobby).
    pub in_game: bool,
    /// Host settings section on the Hosting page is expanded. Collapses when hosting
    /// starts and opens again when it stops, so the live lists get the space.
    pub settings_open: bool,
    /// Recent kicks list shows all entries instead of the latest three.
    pub kicks_expanded: bool,
    /// Game View panel under the lobby. Off by default so the lobby gets the full height.
    pub show_preview: bool,
    /// When the current match started, to tell an early end from a full game.
    pub(super) game_started_at: Option<Instant>,
    /// The host's own BattleTag in player mode, shown first in the player lists.
    pub own_name: Option<String>,
    /// Wall-clock start of the current match, to tell its replay from an older one.
    pub(super) game_started_wall: Option<std::time::SystemTime>,
    /// Chosen replay folder; `None` finds it automatically.
    pub replay_folder: Option<PathBuf>,
    /// The last match's replay could not be found; the Hosting page asks for the folder.
    pub replay_missing: bool,
    pub(super) replay_job: Option<
        std::sync::mpsc::Receiver<Result<ghost_reforged::replay::Summary, super::replays::ReplayError>>,
    >,
    pub hosting_status: String,
    pub(super) closing: Option<iced::window::Id>,
    probing: bool,
    pub(super) settings_due: Option<Instant>,
    pub(super) started: Instant,
    /// First click on Leave game / Exit Warcraft III during a match; a second click
    /// within `game_controls::CONFIRM` confirms.
    pub(super) leave_armed: Option<Instant>,
    pub(super) exit_armed: Option<Instant>,
    /// Close the game once hosting has stopped.
    pub(super) exit_after_stop: bool,
    /// Warcraft III is being closed.
    pub(super) exiting_game: bool,
    /// Bringing the launcher back after a game launch.
    pub(super) refocus: Option<super::game_controls::Refocus>,
    pub(super) side_tab: SideTab,
    /// Newest first.
    pub(super) chat_lines: VecDeque<ChatLine>,
    pub(super) chat_input: String,
    /// Chat lines that arrived while Recent activity was shown.
    pub(super) chat_unread: usize,
    pub(super) sounds: super::sounds::Sounds,
    /// What the X button does: ask (default), or a remembered choice.
    pub(super) close_choice: super::close_ui::CloseChoice,
    /// The close dialog is open for this window; `remember` is its checkbox.
    pub(super) close_prompt: Option<(iced::window::Id, bool)>,
    /// Last hosting state written to `instance.json`.
    pub(super) published_hosting: Option<bool>,
}

impl App {
    pub fn new(directory: &Path, map: Option<PathBuf>) -> Self {
        let (store, profiles, notice, notice_error) = match Store::open(directory) {
            Ok((store, profiles)) => (Some(store), profiles, String::new(), false),
            Err(error) => (None, Profiles::default(), error.to_string(), true),
        };
        let mut form = Form::from(profiles.items[profiles.selected].clone());
        if let Some(map) = map {
            if form.profile.automatic_name {
                form.profile.lobby_pattern = super::map_name::pattern(&map);
            }
            form.profile.map = Some(map);
        }
        let mut app = Self {
            updates: Default::default(),
            data_directory: directory.to_owned(),
            last_reserved: None,
            hosting_requested: false,
            audio: ghost_reforged::background::Silence::default(),
            hidden: false,
            tray_available: false,
            backdrop: super::backdrop::Backdrop::new(),
            focused: true,
            motion: super::motion::Motion::default(),
            page: Page::Hosting,
            form,
            profiles,
            notice,
            notice_error,
            logs: VecDeque::new(),
            preview: Preview::default(),
            picking: false,
            store,
            game: GameState::Checking,
            battlenet: None,
            worker: None,
            lobby: None,
            ds_stats: Default::default(),
            ds_failed: Default::default(),
            ban_prompt: None,
            ban_form_tag: String::new(),
            ban_form_reason: String::new(),
            last_roster: Default::default(),
            last_game: None,
            recent_bans: Default::default(),
            ban_file: Self::locate_ban_file(directory),
            recent_kicks: VecDeque::new(),
            in_game: false,
            settings_open: true,
            kicks_expanded: false,
            show_preview: false,
            game_started_at: None,
            own_name: None,
            game_started_wall: None,
            replay_folder: super::replays::load_folder(directory),
            replay_missing: false,
            replay_job: None,
            hosting_status: "Idle".into(),
            closing: None,
            probing: false,
            settings_due: None,
            started: Instant::now(),
            leave_armed: None,
            exit_armed: None,
            exit_after_stop: false,
            exiting_game: false,
            refocus: None,
            side_tab: SideTab::Activity,
            chat_lines: VecDeque::new(),
            chat_input: String::new(),
            chat_unread: 0,
            sounds: super::sounds::Sounds::new(directory),
            close_choice: super::close_ui::load(directory),
            close_prompt: None,
            published_hosting: None,
        };
        super::sounds::prepare(directory);
        // Nothing to fill in: start with the settings folded away.
        app.settings_open = !app.settings_complete();
        app.log("Launcher opened.");
        app
    }

    /// A map is chosen and every host field is valid, so Host would work as is.
    pub(super) fn settings_complete(&self) -> bool {
        self.store.is_some()
            && self
                .form
                .read()
                .is_ok_and(|profile| profile.map.as_deref().is_some_and(|map| map.is_file()))
    }

    pub(super) fn sound(&mut self, sound: super::sounds::Sound) {
        let enabled = self.form.profile.play_sounds;
        self.sounds.play(sound, enabled);
    }

    pub(super) fn log(&mut self, line: &str) {
        self.log_line(line, false);
    }

    pub(super) fn log_line(&mut self, line: &str, highlight: bool) {
        let seconds = self.started.elapsed().as_secs();
        super::journal::append(&self.data_directory, seconds, line);
        self.logs.push_front(LogEntry {
            time: format!("{:02}:{:02}", seconds / 60, seconds % 60),
            message: line.into(),
            highlight,
        });
        self.logs.truncate(200);
    }

    pub(super) fn notify(&mut self, result: Result<(), String>, success: &str) {
        match result {
            Ok(()) => {
                self.notice = success.into();
                self.notice_error = false;
            }
            Err(error) => {
                self.notice = error;
                self.notice_error = true;
            }
        }
    }

    pub(super) fn save(&mut self) -> Result<(), String> {
        let profile = self.form.read()?;
        let mut updated = self.profiles.clone();
        updated.items[updated.selected] = profile;
        self.store
            .as_ref()
            .ok_or("Profile storage is unavailable.")?
            .save(&updated)
            .map_err(|e| e.to_string())?;
        self.profiles = updated;
        Ok(())
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        if let Some(task) = self.update_message(&message) {
            return task;
        }
        if let Some(task) = self.replay_message(&message) {
            return task;
        }
        if let Some(task) = self.game_message(&message) {
            return task;
        }
        if let Message::Tray(event) = message {
            return self.tray_event(event);
        }
        // Keep instance.json current so a second launcher knows whether we are hosting.
        let hosting = self.worker.is_some();
        if self.published_hosting != Some(hosting) {
            self.published_hosting = Some(hosting);
            super::instance::publish(&self.data_directory, hosting);
        }
        if let Some(task) = self.close_message(&message) {
            return task;
        }
        if (self.motion.modal_visible() || self.updates.available.is_some())
            && !matches!(
                message,
                Message::ClosePreview
                    | Message::Window(_)
                    | Message::WindowFocus(_)
                    | Message::PollBackdrop
                    | Message::Picked(_)
                    | Message::Animate(_)
                    | Message::PollHost
                    | Message::PollProcess
                    | Message::ProcessObserved(_)
                    | Message::BattleNetObserved(_)
                    | Message::CloseRequested(_)
                    | Message::QuitRequested(_)
                    | Message::Stop
            )
        {
            return Task::none();
        }
        if self.picking && matches!(message, Message::SelectProfile(_) | Message::NewProfile) {
            return Task::none();
        }
        if self.worker.is_some()
            && matches!(
                message,
                Message::Edit(
                    Field::Pattern
                    | Field::Number
                    | Field::DsPatch
                    | Field::DsLevel
                    | Field::DsMatches,
                    _,
                )
                    | Message::Private(_)
                    | Message::PlayerHost(_)
                    | Message::ObserverSlots(_)
                    | Message::AutoQuit(_)
                    | Message::AutoQuitDelay(_)
                    | Message::FocusOnStart(_)
                    | Message::Speed(_)
                    | Message::MapName
                    | Message::Browse
                    | Message::SelectProfile(_)
                    | Message::NewProfile
            )
        {
            return Task::none();
        }
        match message {
            Message::Window(action) => {
                return iced::window::latest().and_then(move |id| match action {
                    WindowAction::Drag => iced::window::drag(id),
                    WindowAction::Minimize => iced::window::minimize(id, true),
                    WindowAction::Maximize => iced::window::toggle_maximize(id),
                    WindowAction::Close => Task::done(Message::CloseRequested(id)),
                });
            }
            Message::PollBackdrop => self.backdrop.poll(),
            Message::WindowFocus(focused) => self.focused = focused && !self.hidden,
            Message::Animate(now) => {
                if let Some(page) = self.motion.tick(now) {
                    self.page = page;
                }
            }
            Message::Menu(menu) => self.motion.toggle_menu(menu),
            Message::DismissMenus => self.motion.close_menus(),
            Message::OpenPreview => {
                self.preview.expand();
                if self.preview.expanded {
                    self.motion.modal.target(1.0, Instant::now());
                }
            }
            Message::ClosePreview => {
                self.preview.expanded = false;
                self.motion.modal.target(0.0, Instant::now());
            }
            Message::QuitRequested(id) => {
                self.hosting_requested = false;
                self.updates.resume = false;
                if let Some(worker) = &self.worker {
                    self.closing = Some(id);
                    worker.send(Control::Stop);
                    self.hosting_status = "Stopping…".into();
                } else {
                    return iced::window::close(id);
                }
            }
            Message::Host if self.worker.is_none() && !self.picking && self.closing.is_none() => {
                let result = self.start_hosting();
                if result.is_ok() {
                    self.settings_open = false;
                    self.start_refocus();
                    self.chat_lines.clear();
                    self.chat_unread = 0;
                    self.sound(super::sounds::Sound::HostingStarted);
                }
                self.notify(result, "");
            }
            Message::ToggleHostSettings => {
                self.motion.close_menus();
                self.settings_open = !self.settings_open;
            }
            Message::ToggleKicks => self.kicks_expanded = !self.kicks_expanded,
            Message::TogglePreview => {
                self.show_preview = !self.show_preview;
                if self.show_preview {
                    self.preview.request();
                }
            }
            Message::Stop => {
                self.ban_prompt = None;
                self.hosting_requested = false;
                self.updates.resume = false;
                if let Some(worker) = &self.worker {
                    worker.send(Control::Stop);
                }
                self.hosting_status = "Stopping…".into();
            }
            Message::Start => {
                if let Some(worker) = &self.worker {
                    worker.send(Control::Start);
                }
            }
            Message::Slot(request) => {
                self.motion.close_menus();
                if let Some(worker) = &self.worker {
                    worker.send(Control::EditSlot(request));
                }
            }
            Message::Kick(target) => {
                if let Some(worker) = &self.worker {
                    worker.send(Control::Kick(target));
                }
            }
            Message::Ban(target) => {
                let keep = self
                    .ban_prompt
                    .take()
                    .filter(|(open, _)| *open == target)
                    .map(|(_, reason)| reason);
                self.ban_prompt = Some((target, keep.unwrap_or_default()));
            }
            Message::BanReason(value) => {
                if let Some((_, reason)) = &mut self.ban_prompt {
                    *reason = value.chars().take(ghost_reforged::bans::MAX_REASON).collect();
                }
            }
            Message::CancelBan => self.ban_prompt = None,
            Message::BanFormTag(_)
            | Message::BanFormReason(_)
            | Message::BanFormSubmit
            | Message::BanLastPlayer(_)
            | Message::BanEarlyLeaver(_) => {
                self.ban_message(&message);
            }
            Message::ConfirmBan => {
                if let Some((target, reason)) = self.ban_prompt.take()
                    && let Some(worker) = &self.worker
                {
                    worker.send(Control::Ban(target, reason.trim().to_owned()));
                }
            }
            Message::PollHost => {
                self.poll_host();
                if !self.hidden {
                    self.preview.poll();
                }
                if let Some(task) = self.exit_after_stop_task() {
                    return task;
                }
                if self.worker.is_none() && self.updates.ready.is_some() {
                    return self.install_ready();
                }
                if self.worker.is_none()
                    && let Some(id) = self.closing.take()
                {
                    return iced::window::close(id);
                }
            }
            Message::Page(page) => self.motion.navigate(self.page, page),
            Message::Edit(field, value) => {
                match field {
                    Field::Title => self.form.profile.title = value,
                    Field::Pattern => {
                        self.form.profile.lobby_pattern = value;
                        self.form.profile.automatic_name = false;
                    }
                    Field::Number => self.form.number = value,
                    Field::Players => self.form.players = value,
                    Field::RehostDelay => self.form.rehost_delay = value,
                    Field::EmptyRefresh => self.form.empty_refresh = value,
                    Field::Welcome => self.form.profile.welcome.message = value,
                    Field::DsPatch => self.form.profile.ds_patch_version = value,
                    Field::DsLevel => self.form.min_ds_level = value,
                    Field::DsMatches => self.form.min_ds_matches = value,
                }
                self.queue_settings();
            }
            Message::Command(command, enabled) => {
                self.form.profile.commands.set(command, enabled);
                self.apply_settings();
            }
            Message::Welcome(enabled) => {
                self.form.profile.welcome.enabled = enabled;
                self.apply_settings();
            }
            Message::Rehost(value) => {
                self.form.profile.set_rehost(value);
                self.apply_settings();
            }
            Message::Private(value) => self.form.profile.private_game = value,
            Message::PlayerHost(value) => self.form.profile.player_host = value,
            Message::ObserverSlots(value) => {
                self.form.profile.observer_slots = value.min(ghost_reforged::config::MAX_OBSERVER_SLOTS);
            }
            Message::AutoQuit(value) => self.form.profile.auto_quit = value,
            Message::AutoQuitDelay(value) => self.form.profile.auto_quit_delay_seconds = value.min(60),
            Message::FocusOnStart(value) => self.form.profile.focus_on_start = value,
            Message::Seat(value) => {
                self.motion.close_menus();
                self.form.profile.seat = value;
                self.apply_settings();
            }
            Message::SideTab(tab) => {
                self.side_tab = tab;
                if tab == SideTab::Chat {
                    self.chat_unread = 0;
                }
            }
            Message::ChatInput(value) => {
                self.chat_input = value.chars().filter(|c| !c.is_control()).take(240).collect();
            }
            Message::ChatSend => {
                let text = self.chat_input.trim().to_owned();
                if !text.is_empty()
                    && let Some(worker) = &self.worker
                {
                    worker.send(Control::Chat(text));
                    self.chat_input.clear();
                }
            }
            Message::PlaySounds(value) => {
                self.form.profile.play_sounds = value;
                let _ = self.save();
            }
            Message::OpenSoundsFolder => {
                let folder = super::sounds::folder(&self.data_directory);
                super::sounds::prepare(&self.data_directory);
                let _ = std::process::Command::new("explorer").arg(&folder).spawn();
            }
            Message::Speed(value) => {
                self.form.profile.speed = value;
                self.motion.close_menus();
            }
            Message::RequireFull(value) => {
                self.motion.close_menus();
                self.form.profile.require_full = value;
                self.apply_settings();
            }
            Message::MapName => {
                if let Some(map) = &self.form.profile.map {
                    self.form.profile.lobby_pattern = super::map_name::pattern(map);
                    self.form.profile.automatic_name = true;
                }
            }
            Message::Browse if !self.picking => {
                self.picking = true;
                return Task::perform(super::maps::pick(), Message::Picked);
            }
            Message::Picked(result) => {
                self.picking = false;
                match result {
                    Ok(Some(path)) => {
                        if self.form.profile.automatic_name {
                            self.form.profile.lobby_pattern = super::map_name::pattern(&path);
                        }
                        self.form.profile.map = Some(path);
                        self.notify(Ok(()), "Map selected.");
                    }
                    Err(error) => self.notify(Err(error), ""),
                    Ok(None) => {}
                }
            }
            Message::Save => {
                let result = self.save();
                if result.is_ok() {
                    self.log("Profile saved.");
                }
                self.notify(result, "Saved.");
            }
            Message::SelectProfile(index) if index < self.profiles.items.len() => {
                self.motion.close_menus();
                let result = self.save().and_then(|_| {
                    let mut updated = self.profiles.clone();
                    updated.selected = index;
                    self.store
                        .as_ref()
                        .ok_or("Profile storage is unavailable.")?
                        .save(&updated)
                        .map_err(|e| e.to_string())?;
                    self.profiles = updated;
                    self.form = Form::from(self.profiles.items[index].clone());
                    Ok(())
                });
                self.notify(result, "Profile loaded.");
            }
            Message::NewProfile if self.profiles.items.len() < 30 => {
                let result = self.save().and_then(|_| {
                    let mut updated = self.profiles.clone();
                    updated.items.push(Profile {
                        title: format!("Profile {}", updated.items.len() + 1),
                        ..Profile::default()
                    });
                    updated.selected = updated.items.len() - 1;
                    self.store
                        .as_ref()
                        .ok_or("Profile storage is unavailable.")?
                        .save(&updated)
                        .map_err(|e| e.to_string())?;
                    self.profiles = updated;
                    self.form = Form::from(self.profiles.items[self.profiles.selected].clone());
                    Ok(())
                });
                self.notify(result, "Profile created.");
            }
            Message::PollProcess if !self.probing => {
                self.probing = true;
                return Task::batch([
                    Task::perform(async { process::observe() }, Message::ProcessObserved),
                    Task::perform(
                        async { ghost_reforged::battlenet::state() },
                        Message::BattleNetObserved,
                    ),
                ]);
            }
            Message::BattleNetObserved(state) => {
                if self.battlenet.is_some_and(|old| old != state) {
                    self.log(state.label());
                }
                self.battlenet = Some(state);
            }
            Message::OpenBattleNet => {
                match ghost_reforged::battlenet::find_launcher() {
                    Some(path) => {
                        let result = std::process::Command::new(path)
                            .spawn()
                            .map(|_| ())
                            .map_err(|_| "Battle.net could not be started.".to_string());
                        self.notify(result, "Battle.net opened. Sign in; hosting continues automatically.");
                    }
                    None => self.notify(Err("Battle.net was not found.".into()), ""),
                }
            }
            Message::ProcessObserved(game) => {
                if game.pid.is_some() || game.state == GameState::Stopped {
                    self.audio.target(if self.form.profile.player_host {
                        None
                    } else {
                        game.pid
                    });
                }
                if let Some(error) = self.audio.error() {
                    self.log(&error);
                    self.notify(Err(error), "");
                }
                self.probing = false;
                if self.worker.is_none() {
                    self.preview.target(game.pid);
                    if self.game != game.state {
                        self.preview.request();
                    }
                }
                // The game closed while hosting: the bot will start it again through
                // Battle.net, so bring the launcher back afterwards too.
                if self.worker.is_some()
                    && super::game_controls::game_running(self.game)
                    && game.state == GameState::Stopped
                    && !self.exiting_game
                {
                    self.game = game.state;
                    self.start_refocus();
                }
                self.game = game.state;
            }
            Message::ClearLog => self.logs.clear(),
            _ => {}
        }
        Task::none()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            super::tray::subscription().map(Message::Tray),
            if let Some(interval) = self
                .backdrop
                .interval(self.focused && !self.hidden && !self.motion.modal_visible())
            {
                iced::time::every(interval).map(|_| Message::PollBackdrop)
            } else {
                Subscription::none()
            },
            iced::event::listen_with(|event, _, _| match event {
                iced::Event::Window(iced::window::Event::Focused) => {
                    Some(Message::WindowFocus(true))
                }
                iced::Event::Window(iced::window::Event::Unfocused) => {
                    Some(Message::WindowFocus(false))
                }
                _ => None,
            }),
            if !self.hidden && self.motion.active() {
                iced::time::every(Duration::from_millis(16)).map(Message::Animate)
            } else {
                Subscription::none()
            },
            iced::time::every(Duration::from_secs(3)).map(|_| Message::PollProcess),
            if self.worker.is_some()
                || (!self.hidden && self.preview.needs_poll())
                || self.settings_due.is_some()
                || self.replay_job.is_some()
            {
                iced::time::every(Duration::from_millis(250)).map(|_| Message::PollHost)
            } else {
                Subscription::none()
            },
            if self.preview.expanded {
                iced::event::listen_with(|event, _, _| match event {
                    iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                        key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                        ..
                    }) => Some(Message::ClosePreview),
                    _ => None,
                })
            } else {
                Subscription::none()
            },
            iced::window::close_requests().map(Message::CloseRequested),
        ])
    }
}
