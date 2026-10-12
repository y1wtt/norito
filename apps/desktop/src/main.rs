//! Desktop host: composition root, Iced application, and the screen.
//!
//! - `app`: state and the update loop; `edit`, `drops` and `features` handle its messages.
//! - `services` + `host`: adapters behind ports, and effects executed on workers.
//! - `cli`, `keyboard`, `menu`, `screenshot`, `window_chrome`: inputs and outputs of the shell.
//! - `view` + `theme` + `icons`: drawing.
mod app;
mod cli;
mod drops;
mod edit;
mod features;
mod host;
mod icons;
mod keyboard;
mod menu;
#[cfg(target_os = "macos")]
mod native_menu;
mod screenshot;
mod services;
mod theme;
mod view;
mod window_chrome;

#[cfg(test)]
mod tests;

pub(crate) use app::{App, EditAction, Message};

fn main() -> iced::Result {
    iced::application(App::boot, App::update, App::view)
        .title(App::title)
        .theme(|app: &App| theme::theme(app.light))
        .subscription(App::subscription)
        .default_font(iced::Font::with_name("Yu Gothic UI"))
        .window(iced::window::Settings {
            decorations: !cfg!(target_os = "windows"),
            min_size: Some(iced::Size::new(760.0, 520.0)),
            ..Default::default()
        })
        .window_size((1280, 820))
        .centered()
        .exit_on_close_request(false)
        .run()
}
