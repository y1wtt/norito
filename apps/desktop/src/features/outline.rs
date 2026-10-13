//! The navigation panel: headings, links, images, diagrams, code blocks and tables.
use super::{Event, Kind, message};
use crate::{App, Message, icons, theme, view::controls::outline_icon};
use iced::{
    Element, Length,
    widget::{self, button, column, container, row, scrollable, text},
};
use norito_model::structure::Diagram;

impl App {
    pub(crate) fn outline_view(&self) -> Element<'_, Message> {
        let c = theme::colors(self.light);
        let structure = &self.features.structure;
        let link = |label: String, message: Message| {
            button(text(label).size(12))
                .on_press(message)
                .style(widget::button::text)
        };
        let mut content = column![text("目次").size(12).color(c.muted)].spacing(10);
        for h in &structure.headings {
            content = content.push(
                button(text(&h.title).size(12))
                    .padding(iced::Padding {
                        left: 8.0 + (h.level - 1) as f32 * 10.0,
                        ..iced::Padding::from(5)
                    })
                    .on_press(message(Event::Jump(h.offset)))
                    .style(widget::button::text),
            );
        }
        if structure.headings.is_empty() {
            content = content.push(text("見出しなし").size(12).color(c.muted));
        }
        for l in &structure.links {
            if !l.image {
                content = content.push(
                    button(
                        row![
                            outline_icon(icons::FOLLOW, 14.0, c.text),
                            text(&l.label).size(12)
                        ]
                        .spacing(4)
                        .align_y(iced::Alignment::Center),
                    )
                    .on_press(message(Event::Follow(l.destination.clone())))
                    .style(widget::button::text),
                );
                continue;
            }
            content = content.push(text(&l.label).size(12).color(c.muted));
            match self.features.media.images.get(&l.destination) {
                Some(Ok(handle)) => {
                    content = content.push(widget::image(handle.clone()).width(Length::Fill))
                }
                Some(Err(error)) => content = content.push(text(error).size(11).color(c.muted)),
                None => {}
            }
        }
        for code in &structure.code {
            if code.is_mermaid() {
                content = content.push(self.diagram_view(&Diagram::Mermaid(code.content.clone())));
            }
            let language = if code.language.is_empty() {
                "code".to_owned()
            } else {
                code.language.clone()
            };
            content = content.push(
                button(text(language).size(11))
                    .on_press(Message::EditBlock(code.range.start, Kind::Code))
                    .style(widget::button::text),
            );
        }
        for (i, table) in structure.tables.iter().enumerate() {
            content = content.push(link(
                format!("表 {}", i + 1),
                Message::EditBlock(table.range.start, Kind::Table),
            ));
        }
        container(scrollable(content))
            .padding(16)
            .width(240)
            .height(Length::Fill)
            .into()
    }
    fn diagram_view(&self, diagram: &Diagram) -> Element<'_, Message> {
        match self.features.media.diagrams.get(diagram) {
            Some(Ok(rendered)) => {
                container(widget::svg(rendered.handle.clone()).width(Length::Fill))
                    .padding(8)
                    .style(|_| widget::container::Style {
                        background: Some(iced::Color::WHITE.into()),
                        ..Default::default()
                    })
                    .into()
            }
            Some(Err(error)) => text(format!("Mermaid: {error}")).size(12).into(),
            None => text("図を描画中…").size(12).into(),
        }
    }
}
