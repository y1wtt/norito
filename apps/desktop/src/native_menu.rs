//! macOS main-thread NSMenu adapter. Document commands still go through MVVM.
use super::{EditAction, Message, menu::MenuAction};
use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
fn key(code: Code, shift: bool) -> Option<Accelerator> {
    Some(Accelerator::new(
        Some(
            Modifiers::SUPER
                | if shift {
                    Modifiers::SHIFT
                } else {
                    Modifiers::empty()
                },
        ),
        code,
    ))
}
fn accelerator(action: MenuAction) -> Option<Accelerator> {
    match action {
        MenuAction::New => key(Code::KeyN, false),
        MenuAction::NewDirectory => key(Code::KeyN, true),
        MenuAction::Open => key(Code::KeyO, false),
        MenuAction::Folder => key(Code::KeyO, true),
        MenuAction::Save => key(Code::KeyS, false),
        MenuAction::SaveAs => key(Code::KeyS, true),
        MenuAction::Bold => key(Code::KeyB, false),
        MenuAction::Italic => key(Code::KeyI, false),
        MenuAction::Code => key(Code::KeyE, false),
        MenuAction::Quote => key(Code::KeyQ, true),
        MenuAction::Table => key(Code::KeyT, true),
        MenuAction::Strike => key(Code::KeyX, true),
        MenuAction::Find => key(Code::KeyF, false),
        MenuAction::Paragraph => key(Code::Digit0, false),
        MenuAction::H1 => key(Code::Digit1, false),
        MenuAction::H2 => key(Code::Digit2, false),
        MenuAction::H3 => key(Code::Digit3, false),
        _ => None,
    }
}
thread_local! { static MENU: std::cell::RefCell<Option<Menu>> = const { std::cell::RefCell::new(None) }; }
pub fn install() {
    let root = Menu::new();
    let app = Submenu::new("norito", true);
    let _ = app.append(&PredefinedMenuItem::about(Some("noritoについて"), None));
    let _ = app.append(&PredefinedMenuItem::hide(None));
    let _ = app.append(&MenuItem::with_id(
        "quit",
        "noritoを終了",
        true,
        key(Code::KeyQ, false),
    ));
    let _ = root.append(&app);
    for (label, actions) in [
        ("ファイル", MenuAction::FILE),
        ("書式", MenuAction::FORMAT),
        ("挿入", MenuAction::INSERT),
        ("表示", MenuAction::VIEW),
    ] {
        let menu = Submenu::new(label, true);
        for action in actions {
            if *action == MenuAction::DisplayMode {
                let modes = Submenu::new("表示モード", true);
                for mode in MenuAction::MODES {
                    let _ = modes.append(&MenuItem::with_id(
                        format!("{mode:?}"),
                        mode.to_string(),
                        true,
                        accelerator(*mode),
                    ));
                }
                let _ = menu.append(&modes);
                continue;
            }
            let _ = menu.append(&MenuItem::with_id(
                format!("{action:?}"),
                action.to_string().split("  ").next().unwrap_or_default(),
                true,
                accelerator(*action),
            ));
        }
        let _ = root.append(&menu);
    }
    let edit = Submenu::new("編集", true);
    for (id, name) in [
        ("undo", "元に戻す"),
        ("redo", "やり直す"),
        ("cut", "切り取り"),
        ("copy", "コピー"),
        ("paste", "貼り付け"),
        ("select", "すべて選択"),
    ] {
        let shortcut = match id {
            "undo" => key(Code::KeyZ, false),
            "redo" => key(Code::KeyZ, true),
            "cut" => key(Code::KeyX, false),
            "copy" => key(Code::KeyC, false),
            "paste" => key(Code::KeyV, false),
            "select" => key(Code::KeyA, false),
            _ => None,
        };
        let _ = edit.append(&MenuItem::with_id(id, name, true, shortcut));
    }
    let _ = root.insert(&edit, 2);
    root.init_for_nsapp();
    MENU.with(|menu| *menu.borrow_mut() = Some(root));
}
pub fn poll() -> Vec<Message> {
    let mut messages = Vec::new();
    while let Ok(event) = MenuEvent::receiver().try_recv() {
        let id = event.id.as_ref();
        let message = match id {
            "quit" => Some(Message::Command(norito_viewmodel::Command::Navigate(
                norito_viewmodel::Action::Exit,
            ))),
            "undo" => Some(Message::Command(norito_viewmodel::Command::Undo)),
            "redo" => Some(Message::Command(norito_viewmodel::Command::Redo)),
            "cut" => Some(Message::EditMenu(EditAction::Cut)),
            "copy" => Some(Message::EditMenu(EditAction::Copy)),
            "paste" => Some(Message::EditMenu(EditAction::Paste)),
            "select" => Some(Message::EditMenu(EditAction::SelectAll)),
            _ => [
                MenuAction::FILE,
                MenuAction::FORMAT,
                MenuAction::INSERT,
                MenuAction::VIEW,
                MenuAction::MODES,
            ]
            .into_iter()
            .flatten()
            .find(|action| format!("{action:?}") == id)
            .map(|action| Message::Menu(*action)),
        };
        if let Some(message) = message {
            messages.push(message);
        }
    }
    messages
}
