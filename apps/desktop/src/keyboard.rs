//! Window-level input: shortcuts, IME state, modifiers and dropped files.
use crate::{EditAction, Message, features::Event as Feature};
use iced::{
    Event, Subscription,
    advanced::input_method,
    event::Status,
    keyboard::{self, Key, Modifiers, key::Named},
    window,
};
use norito_editor_widget::{EditorEvent, Formatting, TableAction};
use norito_viewmodel::{Action, Command};

pub fn listen() -> Subscription<Message> {
    iced::event::listen_with(|event, status, _| message(event, status))
}

fn message(event: Event, status: Status) -> Option<Message> {
    match event {
        Event::Window(window::Event::FileDropped(path)) => Some(Message::Dropped(path)),
        Event::Window(window::Event::Unfocused) => Some(Message::Composing(false)),
        Event::InputMethod(input_method::Event::Preedit(text, _)) => {
            Some(Message::Composing(!text.is_empty()))
        }
        Event::InputMethod(input_method::Event::Closed | input_method::Event::Commit(_)) => {
            Some(Message::Composing(false))
        }
        Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
            Some(Message::Modifiers(modifiers))
        }
        Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            modifiers,
            repeat,
            ..
        }) => key_press(key, modifiers, repeat, status),
        _ => None,
    }
}

fn key_press(key: Key, modifiers: Modifiers, repeat: bool, status: Status) -> Option<Message> {
    if key == Key::Named(Named::Escape) {
        return Some(Message::Escape);
    }
    let ignored = status == Status::Ignored;
    // Keys the focused widget left alone move between table cells.
    if ignored && key == Key::Named(Named::Tab) {
        return Some(table(TableAction::Move(modifiers.shift())));
    }
    if ignored && key == Key::Named(Named::Enter) && modifiers.command() {
        return Some(table(TableAction::Row(1)));
    }
    let Key::Character(key) = key else {
        return None;
    };
    if !modifiers.command() {
        return None;
    }
    let key = key.to_lowercase();
    // Editing shortcuts belong to the focused widget; only unhandled ones reach the app.
    if let Some(message) = edit_shortcut(&key, modifiers) {
        return ignored.then(|| Message::ContextShortcut(Box::new(message)));
    }
    let shift = modifiers.shift();
    Some(match key.as_str() {
        "f" | "h" => Message::Feature(Feature::Find),
        "/" if !repeat && !modifiers.alt() && !shift => Message::ToggleMode,
        "s" => Message::Command(Command::Save(shift)),
        "o" if shift => Message::Command(Command::OpenFolder),
        "o" => Message::Command(Command::Navigate(Action::Open)),
        "n" if shift => Message::Command(Command::NewDirectory),
        "n" => Message::Command(Command::Navigate(Action::New)),
        _ => return None,
    })
}

fn edit_shortcut(key: &str, modifiers: Modifiers) -> Option<Message> {
    Some(match key {
        "x" => Message::EditMenu(EditAction::Cut),
        "c" => Message::EditMenu(EditAction::Copy),
        "v" => Message::EditMenu(EditAction::Paste),
        "a" => Message::EditMenu(EditAction::SelectAll),
        "z" if modifiers.shift() => Message::Command(Command::Redo),
        "z" => Message::Command(Command::Undo),
        "y" => Message::Command(Command::Redo),
        "b" => Message::Format(Formatting::Bold),
        "i" => Message::Format(Formatting::Italic),
        "e" => Message::Format(Formatting::Code),
        _ => return None,
    })
}

fn table(action: TableAction) -> Message {
    Message::Editor(EditorEvent::Table(action))
}
