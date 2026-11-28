//! The X button: asks "Minimise to tray or quit?" (with "Remember my choice"),
//! like the W3Champions launcher, instead of always hiding to the tray.
use super::{
    app::{App, Message},
    palette as p,
};
use iced::{
    Color, Element, Fill, Task,
    widget::{column, container, opaque, row, space, stack, text, toggler},
};
use std::path::Path;

const FILE: &str = "close_choice.txt";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CloseChoice {
    #[default]
    Ask,
    Tray,
    Quit,
}

impl CloseChoice {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ask => "Ask every time",
            Self::Tray => "Minimise to tray",
            Self::Quit => "Quit",
        }
    }
}

pub fn load(directory: &Path) -> CloseChoice {
    match std::fs::read_to_string(directory.join(FILE)).unwrap_or_default().trim() {
        "tray" => CloseChoice::Tray,
        "quit" => CloseChoice::Quit,
        _ => CloseChoice::Ask,
    }
}

fn save(directory: &Path, choice: CloseChoice) {
    let path = directory.join(FILE);
    let _ = match choice {
        CloseChoice::Ask => std::fs::remove_file(path),
        CloseChoice::Tray => std::fs::write(path, "tray"),
        CloseChoice::Quit => std::fs::write(path, "quit"),
    };
}

impl App {
    pub(super) fn close_message(&mut self, message: &Message) -> Option<Task<Message>> {
        match message {
            Message::CloseRequested(id) => Some(match self.close_choice {
                CloseChoice::Tray => self.hide_to_tray(*id),
                CloseChoice::Quit => Task::done(Message::QuitRequested(*id)),
                CloseChoice::Ask => {
                    self.close_prompt = Some((*id, false));
                    Task::none()
                }
            }),
            Message::CloseRemember(value) => {
                if let Some((_, remember)) = &mut self.close_prompt {
                    *remember = *value;
                }
                Some(Task::none())
            }
            Message::CloseCancel => {
                self.close_prompt = None;
                Some(Task::none())
            }
            Message::CloseToTray | Message::CloseQuit => {
                let Some((id, remember)) = self.close_prompt.take() else {
                    return Some(Task::none());
                };
                let choice = if matches!(message, Message::CloseToTray) {
                    CloseChoice::Tray
                } else {
                    CloseChoice::Quit
                };
                if remember {
                    self.close_choice = choice;
                    save(&self.data_directory, choice);
                }
                Some(match choice {
                    CloseChoice::Tray => self.hide_to_tray(id),
                    _ => Task::done(Message::QuitRequested(id)),
                })
            }
            Message::CloseAskAgain => {
                self.close_choice = CloseChoice::Ask;
                save(&self.data_directory, CloseChoice::Ask);
                Some(Task::none())
            }
            _ => None,
        }
    }
}

pub fn modal<'a>(app: &'a App, base: Element<'a, Message>) -> Element<'a, Message> {
    let Some((_, remember)) = app.close_prompt else {
        return base;
    };
    let hosting = app.worker.is_some();
    let detail = if hosting {
        "Hosting is running. Minimise keeps it hosting in the system tray; Quit stops hosting \
         and closes GHost Reforged."
    } else {
        "Minimise keeps GHost Reforged in the system tray (open it again from the tray icon)."
    };
    let tray_label = if app.tray_available { "Minimise to tray" } else { "Minimise (no tray)" };
    let content = column![
        text("Close GHost Reforged?").size(23).color(p::GOLD),
        text(detail).size(15),
        toggler(remember)
            .label("Remember my choice (change it in Options)")
            .size(21)
            .text_size(14)
            .on_toggle(Message::CloseRemember),
        row![
            space().width(Fill),
            super::widgets::action("Cancel", Message::CloseCancel, true),
            super::widgets::action(tray_label, Message::CloseToTray, true),
            super::widgets::action(
                if hosting { "Stop and quit" } else { "Quit" },
                Message::CloseQuit,
                true
            ),
        ]
        .spacing(12),
    ]
    .spacing(16);
    stack![
        base,
        opaque(
            container(space())
                .width(Fill)
                .height(Fill)
                .style(|_| container::Style {
                    background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.75).into()),
                    ..Default::default()
                })
        ),
        container(opaque(container(content).padding(26).width(520).style(p::panel)))
            .center_x(Fill)
            .center_y(Fill),
    ]
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::window;

    #[test]
    fn the_x_button_asks_and_a_remembered_choice_is_used_next_time() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::new(dir.path(), None);
        app.tray_available = true;
        let _ = app.update(Message::CloseRequested(window::Id::unique()));
        assert!(app.close_prompt.is_some(), "asks first");
        assert!(!app.hidden);
        let _ = app.update(Message::CloseCancel);
        assert!(app.close_prompt.is_none() && !app.hidden);
        let _ = app.update(Message::CloseRequested(window::Id::unique()));
        let _ = app.update(Message::CloseRemember(true));
        let _ = app.update(Message::CloseToTray);
        assert!(app.hidden);
        assert_eq!(load(dir.path()), CloseChoice::Tray);
        let _ = app.update(Message::CloseAskAgain);
        assert_eq!(load(dir.path()), CloseChoice::Ask);
    }
}
