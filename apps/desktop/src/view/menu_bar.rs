//! The title row (brand, menus, document name, window buttons) and the open dropdown.
use super::controls::panel;
use crate::{
    App, Message, menu::MenuAction, menu::MenuBar, theme, window_chrome::Action as Window,
};
use iced::{
    Element, Length, Padding,
    widget::{button, column, container, mouse_area, opaque, row, space, stack, text},
};
use norito_editor_widget::Mode;
use norito_viewmodel::{Action, Command};

/// The title row height; dropdowns open below it.
const HEIGHT: f32 = 36.0;
const BRAND_WIDTH: f32 = 64.0;

/// Left edge of the title of menu `index`.
fn menu_x(index: usize) -> f32 {
    BRAND_WIDTH
        + if index == 0 {
            0.0
        } else {
            76.0 + (index - 1) as f32 * 52.0
        }
}

impl App {
    pub(super) fn header_view(&self) -> Element<'_, Message> {
        let c = theme::colors(self.light);
        let title = format!(
            "{}{}",
            self.vm.session.name(),
            if self.vm.session.document.dirty() {
                " •"
            } else {
                ""
            }
        );
        let title = mouse_area(
            container(text(title).size(13).color(c.muted))
                .center_x(Length::Fill)
                .height(HEIGHT)
                .center_y(HEIGHT),
        )
        .on_press(Message::Window(Window::Drag))
        .on_double_click(Message::Window(Window::Maximize));
        let brand = container(text("norito").size(13).color(c.text))
            .width(BRAND_WIDTH)
            .padding([9, 16]);
        let mut header = row![brand].align_y(iced::Alignment::Center);
        // macOS shows these menus in the system menu bar instead.
        if !cfg!(target_os = "macos") {
            let mut menus = row![].spacing(4).align_y(iced::Alignment::Center);
            for (index, (label, _)) in MenuBar::MENUS.iter().enumerate() {
                menus = menus.push(self.menu_title(index, label));
            }
            header = header.push(menus);
        }
        header = header.push(title);
        if cfg!(target_os = "windows") {
            header = header
                .push(self.control("—", Message::Window(Window::Minimize), true, false))
                .push(self.control("□", Message::Window(Window::Maximize), true, false))
                .push(self.control(
                    "×",
                    Message::Command(Command::Navigate(Action::Exit)),
                    true,
                    false,
                ));
        }
        header.into()
    }
    fn menu_title<'a>(&self, index: usize, label: &'a str) -> Element<'a, Message> {
        let c = theme::colors(self.light);
        let selected = self.menu.open == Some(index);
        let control = button(
            text(label)
                .size(12)
                .width(Length::Fill)
                .align_x(iced::alignment::Horizontal::Center),
        )
        .width(if index == 0 { 72 } else { 48 })
        .padding([5, 6])
        .on_press(Message::MenuToggle(index))
        .style(move |_, status| theme::control(c, selected, status));
        let area = mouse_area(control);
        if self.menu.open.is_some() {
            area.on_enter(Message::MenuHover(index)).into()
        } else {
            area.into()
        }
    }
    /// `base` with the open dropdown, and the display-mode submenu, over it.
    pub(super) fn with_dropdown<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        let Some(index) = self.menu.open else {
            return base;
        };
        let c = theme::colors(self.light);
        let mut items = column![].spacing(2);
        for action in MenuBar::MENUS[index].1 {
            let submenu = *action == MenuAction::DisplayMode;
            let mut label = row![text(action.to_string()).size(12)].width(Length::Fill);
            if submenu {
                label = label.push(space::horizontal()).push(text("›").size(12));
            }
            items = items.push(
                mouse_area(
                    button(label)
                        .width(Length::Fill)
                        .padding([7, 12])
                        .on_press(Message::Menu(*action))
                        .style(move |_, status| theme::control(c, false, status)),
                )
                .on_enter(Message::ModeMenu(submenu)),
            );
        }
        let mut panels = row![container(items).width(240).padding(4).style(panel(c))];
        if MenuBar::MENUS[index].1.contains(&MenuAction::DisplayMode) && self.menu.modes {
            let mut modes = column![].spacing(2);
            for action in MenuAction::MODES {
                let selected = matches!(
                    (action, self.mode),
                    (MenuAction::Rich, Mode::Rich) | (MenuAction::Source, Mode::Source)
                );
                modes = modes.push(
                    button(text(action.to_string()).size(12))
                        .width(Length::Fill)
                        .padding([7, 12])
                        .on_press(Message::Menu(*action))
                        .style(move |_, status| theme::control(c, selected, status)),
                );
            }
            panels = panels.push(container(modes).width(160).padding(4).style(panel(c)));
        }
        let below = Padding {
            top: HEIGHT,
            ..Default::default()
        };
        let dismiss = container(
            mouse_area(opaque(
                container(space()).width(Length::Fill).height(Length::Fill),
            ))
            .on_press(Message::MenuClose)
            .on_right_press(Message::MenuClose),
        )
        .padding(below);
        stack![
            base,
            dismiss,
            container(opaque(panels)).padding(Padding {
                left: menu_x(index),
                ..below
            })
        ]
        .into()
    }
}
