use super::{
    app::{App, Field, Message},
    art::{self, Icon},
    backdrop_view as bg,
    effects,
    frame_art::Frame,
    motion::Menu,
    palette as p,
    select::select,
    widgets::{action, field, field_enabled, heading, label, muted},
};
use ghost_reforged::ds_report::{self, DsStats, Lookup};
use iced::{
    Alignment, Element, Fill, Length,
    widget::{button, column, container, row, scrollable, space, text, text_input},
};

pub fn view(app: &App) -> Element<'_, Message> {
    let idle = app.worker.is_none();
    let editable = idle && !app.picking && app.store.is_some();
    let selector: Element<'_, Message> = if editable {
        select(
            app,
            Menu::Profile,
            app.form.profile.title.clone(),
            app.profiles
                .items
                .iter()
                .enumerate()
                .map(|(index, p)| (p.title.clone(), Message::SelectProfile(index))),
        )
    } else {
        effects::skin(
            container(text(&app.form.profile.title).size(16))
                .padding([8, 14])
                .width(Fill)
                .style(p::panel),
            Frame::Input,
        )
    };
    let map = effects::skin(
        container(text(app.form.profile.map_label()).size(15))
            .padding([10, 14])
            .width(Fill)
            .style(p::panel),
        Frame::Input,
    );
    let threshold = select(
        app,
        Menu::StartRule,
        if app.form.profile.require_full {
            "Full"
        } else {
            "Minimum players"
        }
        .into(),
        [
            ("Full".into(), Message::RequireFull(true)),
            ("Minimum players".into(), Message::RequireFull(false)),
        ],
    );
    let mut start_rule = column![label("Auto start", Icon::Power), threshold].spacing(2);
    if !app.form.profile.require_full {
        start_rule = start_rule.push(space().height(3)).push(field(
            "Minimum players",
            &app.form.players,
            Field::Players,
        ));
    }
    let controls = if idle {
        row![action("Host", Message::Host, editable)]
    } else {
        let stopping = app.hosting_status == "Stopping…";
        row![
            action(
                "Force start",
                Message::Start,
                !stopping && app.lobby.as_ref().is_some_and(|l| l.can_start)
            ),
            action("Stop Hosting", Message::Stop, !stopping)
        ]
    }
    .spacing(12);
    let settings: Element<'_, Message> = if app.settings_open {
        column![
            column![
                // The toggle shares the Profile label row, so the expanded layout
                // keeps its original height and positions.
                row![
                    container(label("Profile", Icon::User)).width(Fill),
                    toggle("Hide host settings ▴"),
                ]
                .align_y(Alignment::Center),
                row![
                    selector,
                    action(
                        "New",
                        Message::NewProfile,
                        editable && app.profiles.items.len() < 30
                    )
                ]
                .spacing(9)
                .align_y(Alignment::Center)
            ]
            .spacing(2),
            column![
                label("Map", Icon::Map),
                row![map, action("Browse", Message::Browse, idle && !app.picking)]
                    .spacing(9)
                    .align_y(Alignment::Center)
            ]
            .spacing(2),
            row![
                field_enabled(
                    "Game name",
                    &app.form.profile.lobby_pattern,
                    Field::Pattern,
                    idle
                ),
                column![
                    space().height(20),
                    action(
                        "Use map name",
                        Message::MapName,
                        editable && app.form.profile.map.is_some()
                    )
                ]
            ]
            .spacing(8)
            .align_y(Alignment::End),
            row![
                field_enabled("Next number", &app.form.number, Field::Number, idle),
                start_rule
            ]
            .spacing(24),
            ds_filter(app),
        ]
        .spacing(12)
        .into()
    } else {
        column![
            row![
                text("Host settings").size(16).color(p::GOLD).width(Fill),
                toggle("Edit ▾"),
            ]
            .align_y(Alignment::Center),
            settings_grid(app),
        ]
        .spacing(6)
        .into()
    };
    let inputs = column![
        settings,
        iced::widget::rule::horizontal(1),
        current_game(app),
        recent_kicks(app),
        iced::widget::rule::horizontal(1),
        recent_activity(app),
    ]
    .spacing(12);
    let left = effects::frame(
        container(
            column![
                heading("Auto Host", Icon::Tower),
                container(
                    scrollable(
                        container(inputs)
                            .padding(iced::Padding {
                                right: 10.0,
                                ..Default::default()
                            })
                            .width(Fill)
                    )
                    .style(p::scrollbar)
                )
                .padding(iced::Padding {
                    top: 0.0,
                    left: 24.0,
                    right: 24.0,
                    bottom: 22.0
                })
                .height(Fill),
            ]
            .height(Fill),
        )
        .height(Fill)
        .style(p::panel),
    );
    // Game View is optional: the lobby gets the full height unless it is switched on.
    let right: Element<'_, Message> = if app.show_preview {
        column![lobby(app), super::preview_view::thumbnail(app)]
            .spacing(bg::PREVIEW_GAP)
            .height(Fill)
            .into()
    } else {
        lobby(app)
    };
    column![
        status_bar(app, controls),
        row![
            container(left).width(Length::FillPortion(bg::SETTINGS_PORTION)).height(Fill),
            container(right).width(Length::FillPortion(bg::LOBBY_PORTION)).height(Fill)
        ]
        .spacing(bg::COLUMNS_GAP)
        .height(Fill)
    ]
    .spacing(bg::ROW_GAP)
    .into()
}

/// Full-width bar above both columns: what is being hosted, its state, and the main
/// buttons, so the essentials are visible without reading either panel.
fn status_bar<'a>(app: &'a App, controls: iced::widget::Row<'a, Message>) -> Element<'a, Message> {
    let idle = app.worker.is_none();
    let (title, detail) = if let Some(lobby) = &app.lobby {
        (
            lobby.name.clone(),
            format!("{} / {} players", lobby.humans, lobby.player_capacity()),
        )
    } else if app.in_game {
        (
            app.last_game
                .as_ref()
                .map(|game| game.lobby.clone())
                .unwrap_or_else(|| "Match".into()),
            "In game".into(),
        )
    } else {
        let number = app
            .form
            .number
            .trim()
            .parse()
            .unwrap_or(app.form.profile.next_number);
        (
            app.form.profile.lobby_name(number),
            if idle { "Next game" } else { "Preparing" }.into(),
        )
    };
    let mut info = column![
        row![
            text(title).font(art::LABEL).size(24).color(p::GOLD),
            muted(detail)
        ]
        .spacing(14)
        .align_y(Alignment::Center),
        status_line(app),
    ]
    .spacing(4);
    if !app.notice.is_empty() {
        info = info.push(text(&app.notice).size(13).color(if app.notice_error {
            p::ERROR
        } else {
            p::MUTED
        }));
    }
    let preview = effects::hover(
        button(
            row![
                art::icon(Icon::Eye, 22.0),
                text(if app.show_preview { "Hide view" } else { "Game view" })
                    .size(14)
                    .color(p::GOLD)
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .padding([6, 10])
        .style(p::inset)
        .on_press(Message::TogglePreview),
        true,
    );
    effects::frame(
        container(
            row![container(info).width(Fill), preview, controls.spacing(12)]
                .spacing(14)
                .align_y(Alignment::Center),
        )
        .padding([10, 22])
        .width(Fill)
        .height(bg::STATUS_HEIGHT)
        .center_y(bg::STATUS_HEIGHT)
        .style(p::panel),
    )
}

fn lobby(app: &App) -> Element<'_, Message> {
    // While a match runs there is no lobby; show who is playing in the same panel.
    if app.in_game
        && app.lobby.is_none()
        && let Some(game) = app.last_game.as_ref().filter(|g| !g.players.is_empty())
    {
        return game_panel(app, game);
    }
    let mut players = column![].spacing(0).width(Fill);
    let (name, count) = if let Some(lobby) = &app.lobby {
        for slot in &lobby.slots {
            if slot.own {
                continue;
            }
            let name = if slot.status == 0 {
                "Open slot".into()
            } else if slot.status == 1 {
                "Closed slot".into()
            } else if slot.computer {
                "Computer".into()
            } else if slot.name.is_empty() {
                format!("Player {}", slot.number + 1)
            } else {
                slot.name.clone()
            };
            // Levels are shown in the Players section, not squeezed beside the name.
            let name = text(name).size(15).width(Fill);
            let mut item = row![muted(format!("{:02}", slot.number + 1)).width(30), name]
            .spacing(8)
            .align_y(Alignment::Center);
            if slot.status == 2 && !slot.computer && !slot.name.is_empty() {
                item = item.push(level_badge(app, &slot.name));
            }
            if slot.observer {
                item = item.push(muted("Observer"));
            }
            let choices = lobby.slot_actions(slot.number);
            if !choices.is_empty() {
                item = item.push(
                    container(select(
                        app,
                        Menu::Slot(slot.number),
                        "Slot".into(),
                        choices.into_iter().map(|(action, request)| {
                            (action.label().into(), Message::Slot(request))
                        }),
                    ))
                    .width(88),
                );
            }
            if slot.status == 2 && !slot.computer {
                let target = lobby.kick_target(slot.number);
                item = item.push(super::widgets::optional_action(
                    "Kick",
                    target.clone().map(Message::Kick),
                ));
                item = item.push(super::widgets::optional_action(
                    "Ban",
                    target.map(Message::Ban),
                ));
            }
            let mut entry = column![item].spacing(8);
            if let Some((pending, reason)) = &app.ban_prompt
                && slot.status == 2
                && lobby.kick_target(slot.number).as_ref() == Some(pending)
            {
                entry = entry.push(ban_prompt(pending.name(), reason));
            }
            players = players.push(container(entry).padding([9, 0]));
        }
        (
            lobby.name.clone(),
            format!("{} / {}", lobby.humans, lobby.player_capacity()),
        )
    } else {
        let capacity = app.form.players.parse::<u8>().unwrap_or(4).clamp(1, 23);
        for index in 0..capacity {
            players = players.push(
                container(
                    row![
                        muted(format!("{:02}", index + 1)).width(30),
                        text("Open slot").font(art::LABEL).size(15).color(p::MUTED)
                    ]
                    .spacing(8),
                )
                .padding([8, 0]),
            );
        }
        (
            app.form
                .profile
                .lobby_name(app.form.number.parse().unwrap_or(1)),
            format!("0 / {capacity}"),
        )
    };
    let own = app
        .lobby
        .as_ref()
        .and_then(|l| l.slots.iter().find(|s| s.own));
    let observer_name = own.map(|s| s.name.as_str()).unwrap_or("");
    let own_role = host_role(own, app.form.profile.player_host);
    let contents = column![
        row![
            text(name)
                .font(art::LABEL)
                .size(21)
                .color(p::GOLD)
                .width(Fill),
            muted(count)
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        iced::widget::rule::horizontal(1),
        scrollable(container(players).padding(iced::Padding {
            right: 10.0,
            ..Default::default()
        }))
        .style(p::scrollbar)
        .height(Fill),
        iced::widget::rule::horizontal(1),
        row![
            art::ghost(30.0),
            text(observer_name).size(16).width(Fill),
            row![
                level_badge(app, observer_name),
                muted(own_role)
            ]
            .spacing(10)
            .align_y(Alignment::Center)
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    ]
    .spacing(9)
    .height(Fill);
    let mut body = column![
        heading(
            if app.lobby.is_some() {
                "Lobby"
            } else {
                "Lobby Preview"
            },
            Icon::Swords
        ),
        container(contents)
            .padding(iced::Padding {
                top: 8.0,
                right: 24.0,
                bottom: 22.0,
                left: 24.0
            })
            .height(Fill)
    ]
    .height(Fill);
    if app.worker.is_some() && app.lobby.is_none() {
        body = body.push(container(muted(&app.hosting_status)).padding([0, 24]));
    }
    effects::frame(container(body).height(Fill).style(p::panel))
}

/// How the host's own slot is labelled: the live lobby wins over the profile setting.
fn host_role(own: Option<&ghost_reforged::hosting::Slot>, player_host: bool) -> String {
    match own {
        Some(slot) if slot.observer => "Observer".into(),
        Some(slot) => format!("Host · slot {}", slot.number + 1),
        None if player_host => "Host".into(),
        None => "Observer".into(),
    }
}

/// A small text link that shows or hides the host settings. Kept no taller than a label.
fn toggle<'a>(caption: &'static str) -> Element<'a, Message> {
    effects::hover(
        button(text(caption).size(14).color(p::GOLD))
            .padding([0, 4])
            .style(p::inset)
            .on_press(Message::ToggleHostSettings),
        true,
    )
}

/// Labelled rows shown while the host settings are folded: (label, value).
pub(super) fn settings_rows(app: &App) -> Vec<(&'static str, String)> {
    let profile = &app.form.profile;
    let number = app.form.number.trim().parse().unwrap_or(profile.next_number);
    let start = if profile.require_full {
        "Full".to_string()
    } else {
        format!("{} players", app.form.players.trim())
    };
    let (level, matches) = filter_minimums(app);
    let filter = match (level, matches) {
        (0, 0) => "off".to_string(),
        (level, 0) => format!("level {level}+"),
        (0, matches) => format!("{matches}+ matches"),
        (level, matches) => format!("level {level}+ · {matches}+ matches"),
    };
    let rehost = if profile.rehosts() { "on" } else { "OFF (stops after 1 game)" }.to_string();
    let mut rows = vec![
        ("Map", profile.map_label()),
        ("Next game", profile.lobby_name(number)),
        ("Auto start", start),
        ("DS filter", filter),
        ("Rehost", rehost),
    ];
    if profile.player_host {
        rows.push((
            "Auto-quit",
            if profile.auto_quit {
                format!("on · {} s", profile.auto_quit_delay_seconds)
            } else {
                "off".into()
            },
        ));
    }
    rows
}

/// The folded settings as two columns of label/value pairs.
fn settings_grid(app: &App) -> Element<'_, Message> {
    let rows = settings_rows(app);
    let cell = |(label, value): &(&'static str, String)| {
        row![
            muted(*label).width(88),
            text(value.clone()).size(14).width(Fill)
        ]
        .spacing(8)
        .align_y(Alignment::Center)
    };
    // Map and game name are long: full width. The short values pair up.
    let mut grid = column![].spacing(4);
    let (wide, short) = rows.split_at(2.min(rows.len()));
    for item in wide {
        grid = grid.push(cell(item));
    }
    for pair in short.chunks(2) {
        let mut line = row![].spacing(16);
        for item in pair {
            line = line.push(container(cell(item)).width(Fill));
        }
        if pair.len() == 1 {
            line = line.push(space().width(Fill));
        }
        grid = grid.push(line);
    }
    grid.into()
}

/// Coloured dot and the bot's current state, next to Host / Stop.
fn status_line(app: &App) -> Element<'_, Message> {
    let (color, label) = if app.worker.is_none() {
        (p::MUTED, app.hosting_status.as_str())
    } else if app.in_game {
        (p::GOLD, "In game")
    } else if app.lobby.is_some() {
        (p::ACCENT, app.hosting_status.as_str())
    } else {
        (iced::Color::from_rgb8(96, 150, 235), app.hosting_status.as_str())
    };
    let dot = container(space())
        .width(10)
        .height(10)
        .style(move |_| container::Style {
            background: Some(color.into()),
            border: iced::Border {
                radius: 5.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });
    row![dot, text(label.to_string()).size(14).color(color).width(Fill)]
        .spacing(8)
        .align_y(Alignment::Center)
        .width(Fill)
        .into()
}

#[cfg_attr(not(test), allow(dead_code))]
/// One line describing the collapsed settings: game name, auto start and filter.
pub(super) fn settings_summary(app: &App) -> String {
    let number = app.form.number.trim().parse().unwrap_or(app.form.profile.next_number);
    let start = if app.form.profile.require_full {
        "Auto start: Full".to_string()
    } else {
        format!("Auto start: {} players", app.form.players.trim())
    };
    let (level, matches) = filter_minimums(app);
    let filter = match (level, matches) {
        (0, 0) => "Filter off".to_string(),
        (level, 0) => format!("Min level {level}"),
        (0, matches) => format!("Min {matches} matches"),
        (level, matches) => format!("Min level {level} · {matches} matches"),
    };
    let after = if app.form.profile.rehosts() {
        "Rehost: on"
    } else {
        "Rehost: OFF (stops after 1 game)"
    };
    format!(
        "{} · {start} · {filter} · {after}",
        app.form.profile.lobby_name(number)
    )
}

/// One row of the Players section: (level, matches, status, colour).
pub(super) fn player_row(
    name: &str,
    lookup: Option<Option<DsStats>>,
    min_level: u32,
    min_matches: u32,
) -> (String, String, &'static str, iced::Color) {
    let filter_on = min_level > 0 || min_matches > 0;
    let dash = || ("—".to_string(), "—".to_string());
    match lookup {
        Some(Some(stats)) => {
            let status = if ds_report::rejects(min_level, min_matches, Lookup::Found(stats)) {
                ("Below minimum", p::ERROR)
            } else if filter_on {
                ("Passed", p::ACCENT)
            } else {
                ("Found", p::ACCENT)
            };
            (stats.level.to_string(), stats.matches.to_string(), status.0, status.1)
        }
        Some(None) => {
            let (level, matches) = dash();
            let color = if filter_on { p::ERROR } else { p::MUTED };
            (level, matches, "Not on W3C", color)
        }
        None if !name.contains('#') => {
            let (level, matches) = dash();
            (level, matches, "No BattleTag", p::MUTED)
        }
        None => {
            let (level, matches) = dash();
            (level, matches, "Checking…", p::MUTED)
        }
    }
}

fn filter_minimums(app: &App) -> (u32, u32) {
    (
        app.form.min_ds_level.trim().parse().unwrap_or(0),
        app.form.min_ds_matches.trim().parse().unwrap_or(0),
    )
}

/// Live Direct Strike filter: editable while hosting, applied about half a second after typing.
fn ds_filter(app: &App) -> Element<'_, Message> {
    let (level, matches) = filter_minimums(app);
    let summary = match (level, matches) {
        (0, 0) => "Filter off".to_string(),
        (level, 0) => format!("Kicking below level {level}"),
        (0, matches) => format!("Kicking below {matches} matches"),
        (level, matches) => format!("Kicking below level {level} or {matches} matches"),
    };
    column![
        row![
            text("Direct Strike filter").size(16).color(p::GOLD).width(Fill),
            muted(summary)
        ]
        .align_y(Alignment::Center),
        row![
            field("Min level (0 = off)", &app.form.min_ds_level, Field::DsLevel),
            field("Min matches (0 = off)", &app.form.min_ds_matches, Field::DsMatches),
        ]
        .spacing(24),
        muted(if app.worker.is_some() {
            "Live: changes apply to the current lobby, including players already in it."
        } else {
            "Changes apply straight away, also while hosting."
        }),
    ]
    .spacing(6)
    .into()
}

fn stats_table<'a>(
    app: &'a App,
    names: impl Iterator<Item = String>,
    ban: bool,
) -> iced::widget::Column<'a, Message> {
    let (min_level, min_matches) = filter_minimums(app);
    let mut header = row![
        muted("Player").width(Fill),
        muted("Level").width(52),
        muted("Matches").width(66),
        muted("Status").width(112),
    ]
    .spacing(8);
    if ban {
        header = header.push(space().width(78));
    }
    let mut table = column![header].spacing(6);
    // The host comes first, so a working lookup is visible even with nobody else there.
    let own = app.own_name.clone().filter(|_| app.form.profile.player_host);
    let names: Vec<String> = own.iter().cloned().chain(names.filter(|n| Some(n) != own.as_ref())).collect();
    for name in names {
        let you = own.as_ref() == Some(&name);
        let (level, matches, status, color) =
            player_row(&name, app.ds_stats.get(&name).copied(), min_level, min_matches);
        let (status, color) = if status == "Checking…" && app.ds_failed.contains(&name) {
            ("Lookup failed", p::GOLD)
        } else {
            (status, color)
        };
        let status = if you && !matches!(status, "Checking…" | "No BattleTag" | "Lookup failed") {
            if status == "Not on W3C" { "You · not found" } else { "You" }
        } else {
            status
        };
        let shown = if you { format!("{name} (you)") } else { name.clone() };
        let mut line = row![
            text(shown).size(14).width(Fill),
            text(level).size(14).width(52),
            text(matches).size(14).width(66),
            text(status).size(14).color(color).width(112),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        if ban && you {
            line = line.push(space().width(78));
        } else if ban {
            let banned = app.is_recently_banned(&name);
            line = line.push(
                container(action(
                    if banned { "Banned" } else { "Ban" },
                    Message::BanLastPlayer(name),
                    !banned,
                ))
                .width(78),
            );
        }
        table = table.push(line);
    }
    table
}

/// Everyone in the lobby with their Direct Strike level and filter verdict.
#[allow(dead_code)]
fn players(app: &App) -> Element<'_, Message> {
    let guests: Vec<String> = app
        .lobby
        .iter()
        .flat_map(|lobby| lobby.slots.iter())
        .filter(|s| s.status == 2 && !s.computer && !s.own)
        .map(|s| {
            if s.name.is_empty() {
                format!("Player {}", s.number + 1)
            } else {
                s.name.clone()
            }
        })
        .collect();
    let count = guests.len() + usize::from(app.form.profile.player_host && app.own_name.is_some() && app.lobby.is_some());
    let mut section = column![
        text(if count > 0 { format!("Players ({count})") } else { "Players".into() })
            .size(16)
            .color(p::GOLD)
    ]
    .spacing(6);
    if guests.is_empty() && (app.own_name.is_none() || !app.form.profile.player_host || app.lobby.is_none()) {
        return section
            .push(muted(if app.in_game {
                "The game is running; its players are in the Current Game panel."
            } else {
                "No players in the lobby yet. Levels come from ds-report.com."
            }))
            .into();
    }
    section = section.push(stats_table(app, guests.into_iter(), false));
    section.into()
}

/// The right-hand panel during a match: the game's players with level and a Ban button.
fn game_panel<'a>(app: &'a App, game: &'a super::ban_list::LastGame) -> Element<'a, Message> {
    let contents = column![
        row![
            text(&game.lobby)
                .font(art::LABEL)
                .size(21)
                .color(p::GOLD)
                .width(Fill),
            muted(format!("{} other players", game.players.len()))
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        iced::widget::rule::horizontal(1),
        scrollable(
            container(stats_table(app, game.players.iter().cloned(), true)).padding(iced::Padding {
                right: 10.0,
                ..Default::default()
            })
        )
        .style(p::scrollbar)
        .height(Fill),
        iced::widget::rule::horizontal(1),
        muted("Bans are saved now; banned players are removed with a native ban when they join a later lobby."),
    ]
    .spacing(9)
    .height(Fill);
    effects::frame(
        container(
            column![
                heading("Current Game", Icon::Swords),
                container(contents)
                    .padding(iced::Padding {
                        top: 8.0,
                        right: 24.0,
                        bottom: 22.0,
                        left: 24.0
                    })
                    .height(Fill)
            ]
            .height(Fill),
        )
        .height(Fill)
        .style(p::panel),
    )
}

/// Players of the last game, with a Ban button each. During a match they are shown in the
/// Current Game panel on the right instead. After an early end, offers "left early" bans.
fn current_game(app: &App) -> Element<'_, Message> {
    let Some(game) = app
        .last_game
        .as_ref()
        .filter(|g| !g.players.is_empty() && !app.in_game)
    else {
        return space().height(0).into();
    };
    let title = "Last game";
    let reason = effects::skin(
        text_input("Ban reason (optional)", &app.ban_form_reason)
            .on_input(Message::BanFormReason)
            .padding([6, 12])
            .size(14)
            .style(p::input),
        Frame::Input,
    );
    let mut section = column![
        row![
            text(title).size(16).color(p::GOLD).width(Fill),
            muted(&game.lobby)
        ]
        .align_y(Alignment::Center),
    ]
    .spacing(6);
    if let Some(summary) = &game.replay {
        section = section.push(replay_leavers(app, game, summary));
    } else if let Some(seconds) = game.ended_early {
        section = section.push(early_leavers(app, game, seconds));
    }
    if app.replay_missing && game.replay.is_none() {
        section = section.push(replay_folder_prompt());
    }
    section
        .push(stats_table(app, game.players.iter().cloned(), true))
        .push(reason)
        .push(muted("Bans are saved to the ban list; banned players are removed with a native ban when they join a later lobby."))
        .into()
}

/// Early leavers read from the match replay, with what the ban rule did about them.
fn replay_leavers<'a>(
    app: &App,
    game: &'a super::ban_list::LastGame,
    summary: &'a ghost_reforged::replay::Summary,
) -> Element<'a, Message> {
    use ghost_reforged::replay::clock;
    let leavers = summary.early_leavers(super::replays::END_GRACE_MS);
    let own = app.own_name.as_deref();
    let mut list = column![muted(format!(
        "Match length {} · from the replay",
        clock(summary.length_ms)
    ))]
    .spacing(6);
    if leavers.is_empty() {
        list = list.push(muted("Nobody left early."));
    }
    for leave in leavers {
        let you = own.is_some_and(|own| own.eq_ignore_ascii_case(&leave.name));
        let before_end = clock(summary.length_ms.saturating_sub(leave.at_ms));
        let mut line = row![
            text(format!("{} left at {} ({before_end} before the end)", leave.name, clock(leave.at_ms)))
                .size(14)
                .color(if leave.at_ms < super::ban_list::EARLY_END_SECONDS * 1000 {
                    p::ERROR
                } else {
                    p::TEXT
                })
                .width(Fill)
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        if you {
            line = line.push(muted("You"));
        } else if game.auto_banned.iter().any(|name| name == &leave.name) {
            line = line.push(text("Banned automatically").size(13).color(p::GOLD));
        } else {
            let banned = app.is_recently_banned(&leave.name);
            line = line.push(action(
                if banned { "Banned" } else { "Ban" },
                Message::BanEarlyLeaver(leave.name.clone()),
                !banned,
            ));
        }
        list = list.push(line);
    }
    container(list).padding(10).style(p::panel).into()
}

/// Shown when the replay of the last match could not be found.
fn replay_folder_prompt<'a>() -> Element<'a, Message> {
    container(
        column![
            text("LastReplay.w3g was not found, so early leavers could not be detected.")
                .size(14)
                .color(p::GOLD),
            muted("Usually Documents\\Warcraft III\\BattleNet\\<number>\\Replay."),
            row![
                action("Choose Replay folder", Message::PickReplayFolder, true),
                action("Find automatically", Message::AutoReplayFolder, true),
            ]
            .spacing(9),
        ]
        .spacing(6),
    )
    .padding(10)
    .style(p::panel)
    .into()
}

/// After an early end: who left cannot be read from the game, so the host picks them.
fn early_leavers<'a>(app: &App, game: &'a super::ban_list::LastGame, seconds: u64) -> Element<'a, Message> {
    let mut buttons = row![].spacing(8);
    for name in &game.players {
        let banned = app.is_recently_banned(name);
        buttons = buttons.push(action(
            if banned { "Banned" } else { "Left early" },
            Message::BanEarlyLeaver(name.clone()),
            !banned,
        ));
    }
    container(
        column![
            text(format!(
                "Match ended early ({}:{:02}): someone left before 7 minutes.",
                seconds / 60,
                seconds % 60
            ))
            .size(14)
            .color(p::ERROR),
            muted("Click whoever left to ban them with the reason \"Left the game early\"."),
            scrollable(buttons).direction(iced::widget::scrollable::Direction::Horizontal(
                Default::default()
            )),
        ]
        .spacing(6),
    )
    .padding(10)
    .style(p::panel)
    .into()
}

/// "Lv 804 · 107" for a player, coloured by the live DS filter. Empty while unknown.
fn level_badge<'a>(app: &App, name: &str) -> Element<'a, Message> {
    if name.trim().is_empty() {
        return space().width(0).into();
    }
    let (min_level, min_matches) = filter_minimums(app);
    let (level, matches, status, color) =
        player_row(name, app.ds_stats.get(name).copied(), min_level, min_matches);
    let label = match status {
        "Checking…" if app.ds_failed.contains(name) => "Lookup failed".to_string(),
        "Checking…" => "Lv …".to_string(),
        "No BattleTag" => String::new(),
        "Not on W3C" => "Not on W3C".to_string(),
        _ => format!("Lv {level} · {matches}"),
    };
    let color = if app.ds_failed.contains(name) && status == "Checking…" {
        p::GOLD
    } else if status == "Found" {
        p::MUTED
    } else {
        color
    };
    text(label).size(13).color(color).into()
}

/// The newest Activity lines, so status changes are visible without leaving the page.
const ACTIVITY_LINES: usize = 10;

fn recent_activity(app: &App) -> Element<'_, Message> {
    let mut list = column![
        row![
            text("Recent activity").size(16).color(p::GOLD).width(Fill),
            effects::hover(
                button(text("Open log ▸").size(14).color(p::GOLD))
                    .padding([0, 4])
                    .style(p::inset)
                    .on_press(Message::Page(super::app::Page::Activity)),
                true,
            )
        ]
        .align_y(Alignment::Center)
    ]
    .spacing(4);
    if app.logs.is_empty() {
        list = list.push(muted("Nothing yet."));
    }
    for entry in app.logs.iter().take(ACTIVITY_LINES) {
        list = list.push(
            row![
                muted(&entry.time).width(48),
                text(&entry.message)
                    .size(13)
                    .color(if entry.highlight { p::GOLD } else { p::TEXT })
                    .width(Fill)
            ]
            .spacing(8),
        );
    }
    list.into()
}

/// Recent kicks shown before "Show all".
const KICKS_FOLDED: usize = 3;

/// Why players were removed, newest first. Fills the space under the hosting form.
fn recent_kicks(app: &App) -> Element<'_, Message> {
    let total = app.recent_kicks.len();
    let mut header = row![
        text(if total > 0 { format!("Recent kicks ({total})") } else { "Recent kicks".into() })
            .size(16)
            .color(p::GOLD)
            .width(Fill)
    ]
    .align_y(Alignment::Center);
    if total > KICKS_FOLDED {
        header = header.push(effects::hover(
            button(
                text(if app.kicks_expanded { "Show less ▴" } else { "Show all ▾" })
                    .size(14)
                    .color(p::GOLD),
            )
            .padding([0, 4])
            .style(p::inset)
            .on_press(Message::ToggleKicks),
            true,
        ));
    }
    let mut list = column![header].spacing(6);
    if total == 0 {
        list = list.push(muted("Nobody has been kicked yet. Reasons appear here and in lobby chat."));
    }
    let shown = if app.kicks_expanded { total } else { total.min(KICKS_FOLDED) };
    for entry in app.recent_kicks.iter().take(shown) {
        list = list.push(
            row![
                muted(&entry.time).width(48),
                column![text(&entry.name).size(14), muted(&entry.reason)].spacing(2)
            ]
            .spacing(8),
        );
    }
    list.into()
}

/// Inline confirmation shown under a player's row after clicking "Ban".
fn ban_prompt<'a>(player: &'a str, reason: &'a str) -> Element<'a, Message> {
    let input = text_input("Reason (optional)", reason)
        .on_input(Message::BanReason)
        .on_submit(Message::ConfirmBan)
        .padding([8, 14])
        .size(15)
        .style(p::input);
    column![
        muted(format!("Ban {player}? They will be removed and blocked from future lobbies.")),
        effects::skin(input, Frame::Input),
        row![
            super::widgets::action("Confirm Ban", Message::ConfirmBan, true),
            super::widgets::action("Cancel", Message::CancelBan, true),
        ]
        .spacing(9),
    ]
    .spacing(6)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapsed_settings_summarise_name_start_rule_and_filter() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = App::new(directory.path(), None);
        app.form.profile.lobby_pattern = "Mix Mode DS #{n}".into();
        app.form.number = "5".into();
        app.form.profile.require_full = true;
        app.form.profile.rehost = true;
        assert_eq!(
            settings_summary(&app),
            "Mix Mode DS #5 · Auto start: Full · Filter off · Rehost: on"
        );
        app.form.profile.require_full = false;
        app.form.players = "6".into();
        app.form.min_ds_level = "10".into();
        app.form.min_ds_matches = "100".into();
        assert_eq!(
            settings_summary(&app),
            "Mix Mode DS #5 · Auto start: 6 players · Min level 10 · 100 matches · Rehost: on"
        );
        app.form.profile.set_rehost(false);
        assert!(settings_summary(&app).ends_with("Rehost: OFF (stops after 1 game)"));
    }

    #[test]
    fn folded_settings_grid_lists_every_setting_and_auto_quit_only_for_player_hosts() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = App::new(directory.path(), None);
        app.form.profile.lobby_pattern = "Mix Mode DS #{n}".into();
        app.form.number = "8".into();
        app.form.min_ds_level = "10".into();
        let rows = settings_rows(&app);
        let get = |rows: &[(&str, String)], key: &str| rows.iter().find(|r| r.0 == key).map(|r| r.1.clone());
        assert_eq!(get(&rows, "Next game").as_deref(), Some("Mix Mode DS #8"));
        assert_eq!(get(&rows, "DS filter").as_deref(), Some("level 10+"));
        assert!(get(&rows, "Auto-quit").is_none());
        app.form.profile.player_host = true;
        let rows = settings_rows(&app);
        assert_eq!(get(&rows, "Auto-quit").as_deref(), Some("on · 5 s"));
        assert_eq!(get(&rows, "Rehost").as_deref(), Some("on"));
    }

    #[test]
    fn player_rows_show_level_matches_and_filter_status() {
        let found = Some(Some(DsStats { level: 14, matches: 230 }));
        let low = Some(Some(DsStats { level: 3, matches: 12 }));
        assert_eq!(player_row("A#1", found, 10, 0).2, "Passed");
        assert_eq!(player_row("A#1", found, 0, 0).2, "Found");
        assert_eq!(player_row("A#1", found, 0, 0).0, "14");
        assert_eq!(player_row("A#1", found, 0, 0).1, "230");
        assert_eq!(player_row("A#1", low, 10, 100).2, "Below minimum");
        assert_eq!(player_row("A#1", Some(None), 10, 0).2, "Not on W3C");
        assert_eq!(player_row("A#1", None, 10, 0).2, "Checking…");
        assert_eq!(player_row("NoTag", None, 10, 0).2, "No BattleTag");
    }
}
