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
            let stats = (slot.status == 2 && !slot.computer)
                .then(|| app.ds_stats.get(&slot.name))
                .flatten();
            let name: Element<'_, Message> = match stats {
                Some(stats) => row![
                    text(name).size(15),
                    muted(ghost_reforged::ds_report::DsStats::badge(*stats))
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .width(Fill)
                .into(),
                None => text(name).size(15).width(Fill).into(),
            };
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
    let observer_name = app
        .lobby
        .as_ref()
        .and_then(|l| l.slots.iter().find(|s| s.own))
        .map(|s| s.name.as_str())
        .unwrap_or("");
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
            muted("Observer")
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
