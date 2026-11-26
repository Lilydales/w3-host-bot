use super::{
    app::{App, Field, Message},
    art::{self, Icon},
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
    widget::{column, container, row, scrollable, space, text, text_input},
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
    let inputs = column![
        column![
            label("Profile", Icon::User),
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
        players(app),
        current_game(app),
        recent_kicks(app),
    ]
    .spacing(12);
    let mut footer = column![controls].spacing(9);
    if !app.notice.is_empty() {
        footer = footer.push(text(&app.notice).size(14).color(if app.notice_error {
            p::ERROR
        } else {
            p::MUTED
        }));
    }
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
                    bottom: 6.0
                })
                .height(Fill),
                container(footer).height(136).padding(iced::Padding {
                    top: 10.0,
                    left: 24.0,
                    right: 24.0,
                    bottom: 25.0
                })
            ]
            .height(Fill),
        )
        .height(Fill)
        .style(p::panel),
    );
    row![
        container(left).width(Length::FillPortion(3)).height(Fill),
        column![lobby(app), super::preview_view::thumbnail(app)]
            .spacing(super::backdrop_view::PREVIEW_GAP)
            .height(Fill)
            .width(Length::FillPortion(2))
    ]
    .spacing(super::backdrop_view::COLUMNS_GAP)
    .into()
}

fn lobby(app: &App) -> Element<'_, Message> {
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
            muted(own_role)
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
    for name in names {
        let (level, matches, status, color) =
            player_row(&name, app.ds_stats.get(&name).copied(), min_level, min_matches);
        let mut line = row![
            text(name.clone()).size(14).width(Fill),
            text(level).size(14).width(52),
            text(matches).size(14).width(66),
            text(status).size(14).color(color).width(112),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        if ban {
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
    let mut section = column![text("Players").size(16).color(p::GOLD)].spacing(6);
    if guests.is_empty() {
        return section
            .push(muted(if app.in_game {
                "The game is running; its players are listed below."
            } else {
                "No players in the lobby yet. Levels come from ds-report.com."
            }))
            .into();
    }
    section = section.push(stats_table(app, guests.into_iter(), false));
    section.into()
}

/// Players of the game being played (or the last one), with a Ban button each.
/// The lobby is gone once a game starts, so this is the place to ban during or after a match.
fn current_game(app: &App) -> Element<'_, Message> {
    let Some(game) = app.last_game.as_ref().filter(|g| !g.players.is_empty()) else {
        return space().height(0).into();
    };
    let title = if app.in_game { "In this game" } else { "Last game" };
    let reason = effects::skin(
        text_input("Ban reason (optional)", &app.ban_form_reason)
            .on_input(Message::BanFormReason)
            .padding([6, 12])
            .size(14)
            .style(p::input),
        Frame::Input,
    );
    column![
        row![
            text(title).size(16).color(p::GOLD).width(Fill),
            muted(&game.lobby)
        ]
        .align_y(Alignment::Center),
        stats_table(app, game.players.iter().cloned(), true),
        reason,
        muted("Bans are saved to the ban list; banned players are removed when they join a later lobby."),
    ]
    .spacing(6)
    .into()
}

/// Why players were removed, newest first. Fills the space under the hosting form.
fn recent_kicks(app: &App) -> Element<'_, Message> {
    let mut list = column![text("Recent kicks").size(16).color(p::GOLD)].spacing(6);
    if app.recent_kicks.is_empty() {
        list = list.push(muted("Nobody has been kicked yet. Reasons appear here and in lobby chat."));
    }
    for entry in &app.recent_kicks {
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
