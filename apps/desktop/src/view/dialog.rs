//! The unsaved-changes dialog shown while navigation waits for an answer.
use super::controls::panel;
use crate::{App, Message, theme};
use iced::{
    Element, Length,
    widget::{column, container, opaque, row, stack, text},
};
use norito_viewmodel::Command;

impl App {
    pub(super) fn with_save_dialog<'a>(
        &'a self,
        base: Element<'a, Message>,
    ) -> Element<'a, Message> {
        if self.vm.pending.is_none() {
            return base;
        }
        let c = theme::colors(self.light);
        let ready = !self.vm.busy();
        let status = &self.vm.status;
        let dialog = container(
            column![
                text("変更を保存しますか？").size(20),
                text(self.vm.session.name()).size(14).color(c.muted),
                text(if status.error {
                    status.message.as_str()
                } else {
                    ""
                })
                .size(13)
                .color(c.accent),
                row![
                    self.control(
                        "キャンセル",
                        Message::Command(Command::Cancel),
                        ready,
                        false
                    ),
                    self.control(
                        "保存しない",
                        Message::Command(Command::ConfirmDiscard),
                        ready,
                        false
                    ),
                    self.control("保存", Message::Command(Command::ConfirmSave), ready, true)
                ]
                .spacing(12)
            ]
            .spacing(20),
        )
        .padding(28)
        .style(panel(c));
        stack![
            base,
            opaque(
                container(dialog)
                    .center_x(Length::Fill)
                    .center_y(Length::Fill)
                    .style(|_| container::Style {
                        background: Some(iced::Color::from_rgba(0.05, 0.04, 0.03, 0.8).into()),
                        ..Default::default()
                    })
            )
        ]
        .into()
    }
}
