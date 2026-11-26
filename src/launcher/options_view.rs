use super::{
    app::{App, Field, Message},
    art, effects,
    motion::Menu,
    palette as p,
    profile::GameSpeed,
    select::select,
    view::{field, muted},
};
use ghost_reforged::commands::ChatCommand;
use iced::{
    Alignment, Element, Fill,
    widget::{column, container, row, rule, scrollable, text, toggler},
};

pub fn view<'a>(app: &'a App) -> Element<'a, Message> {
    let profile = &app.form.profile;
    let editable = app.store.is_some();
    let mut commands: iced::widget::Column<'a, Message> =
        column![text("Chat commands").size(20)].spacing(18);
    for command in ChatCommand::ALL {
        let toggle = toggler(profile.commands.enabled(command))
            .label(command.name())
            .size(21)
            .text_size(14);
        let toggle = if editable {
            toggle.on_toggle(move |enabled| Message::Command(command, enabled))
        } else {
            toggle
        };
        commands = commands.push(column![toggle, muted(command.description())].spacing(6));
    }
    let mut settings: iced::widget::Column<'a, Message> = column![
        muted(format!("Profile · {}", profile.title)),
        rule::horizontal(1),
        super::ban_list::section(app),
        rule::horizontal(1),
        text("Welcome message").size(20),
        toggler(profile.welcome.enabled)
            .label("Welcome joining players")
            .size(21)
            .text_size(14)
            .on_toggle(Message::Welcome),
        field("Message", &profile.welcome.message, Field::Welcome),
        muted("Use #{playername} for the joining player's name."),
        rule::horizontal(1),
        commands,
        rule::horizontal(1),
        text("Hosting").size(20),
        toggler(profile.rehost)
            .label(if profile.player_host {
                "Host the next lobby after each match"
            } else {
                "Create next lobby after countdown"
            })
            .size(21)
            .text_size(14)
            .on_toggle(Message::Rehost),
        if app.worker.is_none() {
            column![
                toggler(profile.player_host)
                    .label("Play as host (visible game)")
                    .size(21)
                    .text_size(14)
                    .on_toggle(Message::PlayerHost),
                if profile.player_host {
                    Element::from(column![
                        observer_stepper(profile.observer_slots),
                        toggler(profile.auto_quit)
                            .label("Press Quit on the Direct Strike end panel")
                            .size(21)
                            .text_size(14)
                            .on_toggle(Message::AutoQuit),
                        delay_stepper(profile.auto_quit_delay_seconds),
                        toggler(profile.leave_if_quit_fails)
                            .label("If Quit cannot be pressed, leave the game (may count as a leave)")
                            .size(21)
                            .text_size(14)
                            .on_toggle(Message::LeaveIfQuitFails),
                        muted(
                            "The bot watches the game window for the VICTORY/DEFEAT panel and clicks \
                             its Quit button, so the result is recorded normally and the next lobby \
                             is hosted. Your mouse may move briefly if the first click is ignored."
                        ),
                    ]
                    .spacing(12))
                } else {
                    column![].into()
                },
                toggler(profile.private_game)
                    .label("Private lobby")
                    .size(21)
                    .text_size(14)
                    .on_toggle(Message::Private),
                column![
                    muted("Game speed"),
                    select(
                        app,
                        Menu::Speed,
                        profile.speed.to_string(),
                        GameSpeed::ALL
                            .into_iter()
                            .map(|speed| (speed.to_string(), Message::Speed(speed)))
                    )
                ]
                .spacing(7),
            ]
            .spacing(16)
        } else {
            column![
                muted(format!(
                    "{} lobby · {} speed",
                    if profile.private_game {
                        "Private"
                    } else {
                        "Public"
                    },
                    profile.speed
                )),
                muted("Map, game name, visibility and speed are set when the lobby is created.")
            ]
            .spacing(7)
        },
        row![
            field(
                if profile.player_host {
                    "Results screen time (s)"
                } else {
                    "Rehost delay (s)"
                },
                &app.form.rehost_delay,
                Field::RehostDelay
            ),
            field(
                "Empty lobby refresh (s, 0 = off)",
                &app.form.empty_refresh,
                Field::EmptyRefresh
            )
        ]
        .spacing(12),
        muted(if profile.player_host {
            "Play as host: when your match ends, the results stay open for this many seconds, \
             then the bot closes them and hosts the next lobby. Click Back sooner to skip the wait."
        } else {
            "The bot leaves each started game and creates the next lobby after this delay."
        }),
        rule::horizontal(1),
        text("Direct Strike filter").size(20),
        muted(
            "Players with a BattleTag are looked up on ds-report.com when they join. \
             Players who are not found, or are below a minimum, are kicked (never banned). \
             0 turns a minimum off. The minimums apply immediately, also while hosting \
             (they are on the Hosting page too); the patch version applies when hosting starts."
        ),
        super::widgets::field_enabled(
            "Patch version",
            &profile.ds_patch_version,
            Field::DsPatch,
            app.worker.is_none()
        ),
        row![
            super::widgets::field_enabled(
                "Minimum level (0 = off)",
                &app.form.min_ds_level,
                Field::DsLevel,
                true
            ),
            super::widgets::field_enabled(
                "Minimum matches (0 = off)",
                &app.form.min_ds_matches,
                Field::DsMatches,
                true
            )
        ]
        .spacing(12),
    ]
    .spacing(16)
    .max_width(760);

    if !app.notice.is_empty() {
        settings = settings.push(text(&app.notice).size(13).color(if app.notice_error {
            p::ERROR
        } else {
            p::MUTED
        }));
    }
    effects::frame(
        container(
            column![
                super::widgets::heading("Options", art::Icon::Gear),
                container(
                    scrollable(container(settings).padding(iced::Padding {
                        right: 16.0,
                        ..Default::default()
                    }))
                    .style(p::scrollbar)
                )
                .padding(24)
                .height(Fill)
            ]
            .height(Fill),
        )
        .height(Fill)
        .style(p::panel),
    )
}

/// Observer slots left open in a player-host lobby; the map's other observer slots are closed.
fn observer_stepper<'a>(count: u8) -> Element<'a, Message> {
    let max = ghost_reforged::config::MAX_OBSERVER_SLOTS;
    column![
        row![
            super::widgets::action("−", Message::ObserverSlots(count.saturating_sub(1)), count > 0),
            text(count.to_string()).size(16).width(28).center(),
            super::widgets::action("+", Message::ObserverSlots((count + 1).min(max)), count < max),
            muted("Observer slots (0 = none)"),
        ]
        .spacing(9)
        .align_y(Alignment::Center),
    ]
    .into()
}

/// Seconds the end panel stays up before Quit is pressed (0..=60, steps of 5).
fn delay_stepper<'a>(seconds: u64) -> Element<'a, Message> {
    row![
        super::widgets::action("−", Message::AutoQuitDelay(seconds.saturating_sub(5)), seconds > 0),
        text(format!("{seconds} s")).size(16).width(44).center(),
        super::widgets::action("+", Message::AutoQuitDelay((seconds + 5).min(60)), seconds < 60),
        muted("Wait before pressing Quit"),
    ]
    .spacing(9)
    .align_y(Alignment::Center)
    .into()
}
