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
        'c' => Some(EditorEvent::Copy),
        'x' => Some(EditorEvent::Cut),
        'v' => Some(EditorEvent::Paste),
        'b' => format(Formatting::Bold),
        'i' => format(Formatting::Italic),
        'e' => format(Formatting::Code),
        n @ '0'..='6' => format(Formatting::Heading(n as u8 - b'0')),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::keyboard::{
        Modifiers,
        key::{NativeCode, Physical},
    };

    fn press(key: Key, modifiers: Modifiers, focused: bool) -> KeyPress {
        KeyPress {
            modified_key: key.clone(),
            key,
            physical_key: Physical::Unidentified(NativeCode::Unidentified),
            modifiers,
            text: None,
            status: if focused {
                Status::Focused { is_hovered: false }
            } else {
                Status::Active
            },
        }
    }

    #[test]
    fn every_editor_shortcut_routes_only_while_focused() {
        let command = Modifiers::COMMAND;
        for (key, shift, expected) in [
            ("c", false, "Copy"),
            ("x", false, "Cut"),
            ("v", false, "Paste"),
            ("z", false, "Undo"),
            ("z", true, "Redo"),
            ("y", false, "Redo"),
            ("b", false, "Format(Bold)"),
            ("i", false, "Format(Italic)"),
            ("e", false, "Format(Code)"),
            ("x", true, "Format(Strike)"),
            ("q", true, "Format(Quote)"),
            ("t", true, "InsertTable"),
            ("0", false, "Format(Heading(0))"),
            ("1", false, "Format(Heading(1))"),
            ("2", false, "Format(Heading(2))"),
            ("3", false, "Format(Heading(3))"),
            ("4", false, "Format(Heading(4))"),
            ("5", false, "Format(Heading(5))"),
            ("6", false, "Format(Heading(6))"),
        ] {
            let modifiers = command
                | if shift {
                    Modifiers::SHIFT
                } else {
                    Modifiers::empty()
                };
            let Some(Binding::Custom(event)) =
                binding(press(Key::Character(key.into()), modifiers, true))
            else {
                panic!("missing {key}");
            };
            assert_eq!(format!("{event:?}"), expected);
            assert!(!matches!(
                binding(press(Key::Character(key.into()), modifiers, false)),
                Some(Binding::Custom(_))
            ));
            assert!(!matches!(
                binding(press(
                    Key::Character(key.into()),
                    modifiers | Modifiers::ALT,
                    true
                )),
                Some(Binding::Custom(_))
            ));
        }
        assert!(matches!(
            binding(press(Key::Named(Named::F10), Modifiers::SHIFT, true)),
            Some(Binding::Custom(EditorEvent::ContextMenu))
        ));
        assert!(matches!(
            binding(press(Key::Named(Named::Enter), command, true)),
            Some(Binding::Custom(EditorEvent::FollowLink))
        ));
    }
}
