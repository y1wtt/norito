//! Editor key bindings. Shortcuts that need the application go out as events.
use crate::{EditorEvent, Formatting};
use iced::keyboard::{Key, key::Named};
use iced::widget::text_editor::{Binding, KeyPress, Status};

pub(crate) fn binding(key: KeyPress) -> Option<Binding<EditorEvent>> {
    if matches!(key.status, Status::Focused { .. })
        && let Some(event) = shortcut(&key)
    {
        return Some(Binding::Custom(event));
    }
    Binding::from_key_press(key)
}

fn shortcut(key: &KeyPress) -> Option<EditorEvent> {
    let m = key.modifiers;
    if key.key == Key::Named(Named::ContextMenu) || (key.key == Key::Named(Named::F10) && m.shift())
    {
        return Some(EditorEvent::ContextMenu);
    }
    if !m.command() || m.alt() {
        return None;
    }
    if key.key == Key::Named(Named::Enter) {
        return Some(EditorEvent::FollowLink);
    }
    let format = |f| Some(EditorEvent::Format(f));
    match key
        .key
        .to_latin(key.physical_key)
        .map(|c| c.to_ascii_lowercase())?
    {
        'z' if m.shift() => Some(EditorEvent::Redo),
        'z' => Some(EditorEvent::Undo),
        'y' => Some(EditorEvent::Redo),
        'q' if m.shift() => format(Formatting::Quote),
        't' if m.shift() => Some(EditorEvent::InsertTable),
        'x' if m.shift() => format(Formatting::Strike),
        'b' => format(Formatting::Bold),
        'i' => format(Formatting::Italic),
        'e' => format(Formatting::Code),
        n @ '0'..='6' => format(Formatting::Heading(n as u8 - b'0')),
        _ => None,
    }
}
