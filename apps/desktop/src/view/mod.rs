//! Screen layout. Each region is drawn by its own module: `menu_bar` (title row
//! and dropdowns), `sidebar` (folder tree), `page` (editor and its overlays),
//! `dialog` (unsaved changes) and `controls` (shared buttons and icons).
pub(crate) mod controls;
mod dialog;
mod menu_bar;
mod page;
mod sidebar;

use crate::{App, Message, features, icons, theme, window_chrome};
use iced::{
    Element, Length, Padding,
    widget::{column, container, row, space, stack, text},
};
use norito_editor_widget::Mode;
pub use sidebar::Chevrons;

impl App {
    pub fn view(&self) -> Element<'_, Message> {
        let c = theme::colors(self.light);
        let mut body = row![].height(Length::Fill);
        if self.sidebar {
            body = body.push(self.sidebar_view());
        }
        body = body.push(self.page_view());
        if self.features.outline {
            body = body.push(self.outline_view());
        }
        let body = stack![
            body,
            container(self.icon_control(
                icons::OUTLINE,
                "目次",
                Message::Feature(features::Event::ToggleOutline),
                true,
                self.features.outline,
            ))
            .align_right(Length::Fill)
            .padding(Padding {
                top: 4.0,
                right: 2.0,
                ..Default::default()
            })
        ];
        // Only progress and errors are worth a status line.
        let status = &self.vm.status;
        let footer = row![
            self.icon_control(
                icons::CODE,
                "表示モードを切り替え  Ctrl+/",
                Message::ToggleMode,
                !self.composing,
                self.mode == Mode::Source
            ),
            text(if status.error || self.vm.busy() {
                status.message.as_str()
            } else {
                ""
            })
            .size(12)
            .color(if status.error { c.accent } else { c.muted }),
            space::horizontal(),
            text(format!("{} 文字", self.editor.character_count()))
                .size(11)
                .color(c.muted)
        ]
        .spacing(16)
        .align_y(iced::Alignment::Center);
        let base: Element<'_, Message> = column![
            self.header_view(),
            self.search_view(),
            body,
            container(footer).padding([8, 24])
        ]
        .height(Length::Fill)
        .into();
        let base = if cfg!(target_os = "windows") {
            window_chrome::resize_frame(base)
        } else {
            base
        };
        self.with_save_dialog(self.with_dropdown(base))
    }
}
