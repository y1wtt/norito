//! Window actions are kept out of the document view model.
use super::Message;
use iced::{Subscription, Task};
#[derive(Debug, Clone, Copy)]
pub enum Action {
    Drag,
    Minimize,
    Maximize,
    Resize(iced::window::Direction),
}
pub fn perform(action: Action) -> Task<Message> {
    iced::window::latest().then(move |id| match id {
        Some(id) => match action {
            Action::Drag => iced::window::drag(id),
            Action::Minimize => iced::window::minimize(id, true),
            Action::Maximize => iced::window::toggle_maximize(id),
            Action::Resize(direction) => iced::window::drag_resize(id, direction),
        },
        None => Task::none(),
    })
}
pub fn initialize() -> Task<Message> {
    #[cfg(target_os = "macos")]
    {
        iced::window::latest().and_then(|id| {
            iced::window::run(id, |_| {
                super::native_menu::install();
                Message::Noop
            })
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        Task::none()
    }
}
pub fn subscription() -> Subscription<Message> {
    #[cfg(target_os = "macos")]
    {
        iced::time::every(std::time::Duration::from_millis(40)).map(|_| Message::NativePoll)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Subscription::none()
    }
}
pub fn resize_frame(base: iced::Element<'_, Message>) -> iced::Element<'_, Message> {
    use iced::window::Direction::*;
    use iced::{
        Alignment, Length,
        widget::{container, mouse_area, space, stack},
    };
    let mut layers = stack![base];
    for (direction, x, y, width, height) in [
        (
            North,
            Alignment::Center,
            Alignment::Start,
            Length::Fill,
            Length::Fixed(4.0),
        ),
        (
            South,
            Alignment::Center,
            Alignment::End,
            Length::Fill,
            Length::Fixed(4.0),
        ),
        (
            West,
            Alignment::Start,
            Alignment::Center,
            Length::Fixed(4.0),
            Length::Fill,
        ),
        (
            East,
            Alignment::End,
            Alignment::Center,
            Length::Fixed(4.0),
            Length::Fill,
        ),
        (
            NorthWest,
            Alignment::Start,
            Alignment::Start,
            Length::Fixed(8.0),
            Length::Fixed(8.0),
        ),
        (
            NorthEast,
            Alignment::End,
            Alignment::Start,
            Length::Fixed(8.0),
            Length::Fixed(8.0),
        ),
        (
            SouthWest,
            Alignment::Start,
            Alignment::End,
            Length::Fixed(8.0),
            Length::Fixed(8.0),
        ),
        (
            SouthEast,
            Alignment::End,
            Alignment::End,
            Length::Fixed(8.0),
            Length::Fixed(8.0),
        ),
    ] {
        layers = layers.push(
            container(
                mouse_area(space().width(width).height(height))
                    .on_press(Message::Window(Action::Resize(direction))),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(x)
            .align_y(y),
        );
    }
    layers.into()
}
