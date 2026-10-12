//! Viewport-aligned decorations for a continuous editor. No Markdown knowledge.
use iced::advanced::{
    Clipboard, Layout, Renderer as _, Shell, Widget,
    image::{self, Renderer as _},
    layout, mouse, renderer,
    svg::{self, Renderer as _},
    text::{self, Renderer as _},
    widget::{Operation, Tree},
};
use iced::widget::text_editor::Content;
use iced::{Color, Element, Event, Length, Rectangle, Renderer, Size, Theme};

fn placeholder(
    renderer: &mut Renderer,
    bounds: Rectangle,
    clip: Rectangle,
    theme: &Theme,
    label: &str,
) {
    renderer.fill_text(
        text::Text {
            content: label.to_owned(),
            bounds: bounds.size(),
            size: 12.0.into(),
            line_height: text::LineHeight::Relative(1.5),
            font: iced::Font::with_name("Yu Gothic UI"),
            align_x: text::Alignment::Default,
            align_y: iced::alignment::Vertical::Top,
            shaping: text::Shaping::Advanced,
            wrapping: text::Wrapping::WordOrGlyph,
        },
        bounds.position(),
        theme.palette().text,
        clip,
    );
}

pub enum Decoration<Message> {
    InlineDiagram {
        line: usize,
        range: std::ops::Range<usize>,
        handle: Option<svg::Handle>,
        on_press: Message,
    },
    Table {
        line: usize,
        cells: Vec<Vec<String>>,
        alignment: Vec<u8>,
        on_cell: Vec<Vec<Message>>,
    },
    Image {
        line: usize,
        handle: Option<image::Handle>,
        on_press: Message,
    },
    Strike {
        line: usize,
        range: std::ops::Range<usize>,
    },
    Separator {
        line: usize,
    },
    Rule {
        line: usize,
        color: Color,
    },
    Diagram {
        line: usize,
        handle: Option<svg::Handle>,
        status: String,
        on_press: Message,
    },
}
impl<M> Decoration<M> {
    fn line(&self) -> usize {
        match self {
            Self::Rule { line, .. }
            | Self::Diagram { line, .. }
            | Self::Table { line, .. }
            | Self::Image { line, .. }
            | Self::Strike { line, .. }
            | Self::Separator { line }
            | Self::InlineDiagram { line, .. } => *line,
        }
    }
    fn action(&self) -> Option<&M> {
        match self {
            Self::Diagram { on_press, .. }
            | Self::Image { on_press, .. }
            | Self::InlineDiagram { on_press, .. } => Some(on_press),
            Self::Table { on_cell, .. } => on_cell.first().and_then(|r| r.first()),
            _ => None,
        }
    }
}
pub fn decorate<'a, Message: Clone + 'a>(
    editor: Element<'a, Message>,
    content: &'a Content,
    decorations: Vec<Decoration<Message>>,
) -> Element<'a, Message> {
    Element::new(Decorated {
        editor,
        content,
        decorations,
    })
}
struct Decorated<'a, Message> {
    editor: Element<'a, Message>,
    content: &'a Content,
    decorations: Vec<Decoration<Message>>,
}
impl<Message: Clone> Decorated<'_, Message> {
    fn decoration_bounds(
        &self,
        decoration: &Decoration<Message>,
        layout: Layout<'_>,
    ) -> Option<Rectangle> {
        if let Decoration::InlineDiagram { line, range, .. } = decoration {
            self.content
                .span_bounds(*line, range.clone())
                .first()
                .map(|r| {
                    *r + iced::Vector::new(
                        layout.bounds().x + crate::EDITOR_PADDING.left,
                        layout.bounds().y + crate::EDITOR_PADDING.top,
                    )
                })
        } else {
            self.bounds(decoration.line(), layout)
        }
    }
    fn bounds(&self, line: usize, layout: Layout<'_>) -> Option<Rectangle> {
        self.content.line_bounds(line).map(|r| {
            r + iced::Vector::new(
                layout.bounds().x,
                layout.bounds().y + crate::EDITOR_PADDING.top,
            )
        })
    }
}
impl<Message: Clone> Widget<Message, Theme, Renderer> for Decorated<'_, Message> {
    fn size(&self) -> Size<Length> {
        self.editor.as_widget().size()
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.editor)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.editor));
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.editor
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.editor
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        ) {
            for decoration in &self.decorations {
                if let Some(on_press) = decoration.action()
                    && let Some(bounds) = self
                        .decoration_bounds(decoration, layout)
                        .and_then(|r| r.intersection(&layout.bounds()))
                        .and_then(|r| r.intersection(viewport))
                    && cursor.is_over(bounds)
                {
                    let message = if let Decoration::Table { on_cell, .. } = decoration {
                        let bounds = self.decoration_bounds(decoration, layout).unwrap();
                        let point = cursor.position().unwrap();
                        let columns = on_cell.first().map_or(1, Vec::len).max(1);
                        let r = ((point.y - bounds.y) / 34.0) as usize;
                        let c = ((point.x - bounds.x) / (bounds.width / columns as f32)) as usize;
                        on_cell
                            .get(r)
                            .and_then(|row| row.get(c))
                            .unwrap_or(on_press)
                    } else {
                        on_press
                    };
                    shell.publish(message.clone());
                    shell.capture_event();
                    return;
                }
            }
        }
        self.editor.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        for decoration in &self.decorations {
            if decoration.action().is_some()
                && let Some(bounds) = self
                    .decoration_bounds(decoration, layout)
                    .and_then(|r| r.intersection(&layout.bounds()))
                && cursor.is_over(bounds)
            {
                return mouse::Interaction::Pointer;
            }
        }
        self.editor.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.editor.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
        let Some(clip) = layout.bounds().intersection(viewport) else {
            return;
        };
        renderer.with_layer(clip, |renderer| {
            for decoration in &self.decorations {
                let line = decoration.line();
                let Some(mut bounds) = self.decoration_bounds(decoration, layout) else {
                    continue;
                };
                if bounds.intersection(&clip).is_none() {
                    continue;
                }
                match decoration {
                    Decoration::InlineDiagram {
                        handle: Some(handle),
                        ..
                    } => {
                        let native = renderer.measure_svg(handle);
                        let scale = (bounds.width / native.width as f32)
                            .min(bounds.height / native.height as f32);
                        renderer.draw_svg(
                            svg::Svg::new(handle.clone()).color(theme.palette().text),
                            Rectangle::new(
                                bounds.position(),
                                Size::new(
                                    native.width as f32 * scale,
                                    native.height as f32 * scale,
                                ),
                            ),
                            clip,
                        );
                    }
                    Decoration::InlineDiagram { .. } => {
                        placeholder(renderer, bounds, clip, theme, "数式…");
                    }
                    Decoration::Separator { .. } => {
                        bounds.y += bounds.height / 2.0;
                        bounds.height = 1.0;
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds,
                                ..Default::default()
                            },
                            theme.extended_palette().background.strong.color,
                        );
                    }
                    Decoration::Strike { range, .. } => {
                        for rect in self.content.span_bounds(line, range.clone()) {
                            let mut bounds = rect
                                + iced::Vector::new(
                                    layout.bounds().x + crate::EDITOR_PADDING.left,
                                    layout.bounds().y + crate::EDITOR_PADDING.top,
                                );
                            bounds.y += bounds.height * 0.53;
                            bounds.height = 1.0;
                            renderer.fill_quad(
                                renderer::Quad {
                                    bounds,
                                    ..Default::default()
                                },
                                theme.palette().text,
                            );
                        }
                    }
                    Decoration::Table {
                        cells, alignment, ..
                    } => {
                        let columns = cells.iter().map(Vec::len).max().unwrap_or(1).max(1);
                        let width = bounds.width / columns as f32;
                        for (r, row) in cells.iter().enumerate() {
                            for (c, value) in row.iter().enumerate() {
                                let cell = Rectangle {
                                    x: bounds.x + c as f32 * width,
                                    y: bounds.y + r as f32 * 34.0,
                                    width,
                                    height: 34.0,
                                };
                                renderer.fill_quad(
                                    renderer::Quad {
                                        bounds: cell,
                                        border: iced::Border {
                                            width: 1.0,
                                            color: theme.extended_palette().background.strong.color,
                                            ..Default::default()
                                        },
                                        ..Default::default()
                                    },
                                    if r == 0 {
                                        theme.extended_palette().background.weak.color
                                    } else {
                                        theme.palette().background
                                    },
                                );
                                // fill_text positions the text by its alignment anchor.
                                let inner = cell.shrink(7.0);
                                let (align_x, x) = match alignment.get(c) {
                                    Some(1) => (text::Alignment::Center, inner.center_x()),
                                    Some(2) => (text::Alignment::Right, inner.x + inner.width),
                                    _ => (text::Alignment::Default, inner.x),
                                };
                                renderer.fill_text(
                                    text::Text {
                                        content: value.clone(),
                                        bounds: inner.size(),
                                        font: iced::Font::with_name("Yu Gothic UI"),
                                        size: 14.0.into(),
                                        line_height: text::LineHeight::Relative(1.4),
                                        align_x,
                                        align_y: iced::alignment::Vertical::Top,
                                        shaping: text::Shaping::Advanced,
                                        wrapping: text::Wrapping::None,
                                    },
                                    iced::Point::new(x, inner.y),
                                    theme.palette().text,
                                    cell.intersection(&clip).unwrap_or_default(),
                                );
                            }
                        }
                    }
                    Decoration::Image {
                        handle: Some(handle),
                        ..
                    } => {
                        let Some(native) = renderer.measure_image(handle) else {
                            continue;
                        };
                        let scale = (bounds.width / native.width as f32)
                            .min(bounds.height / native.height as f32)
                            .min(1.0);
                        let size =
                            Size::new(native.width as f32 * scale, native.height as f32 * scale);
                        renderer.draw_image(
                            image::Image::new(handle.clone()),
                            Rectangle::new(bounds.position(), size),
                            clip,
                        );
                    }
                    Decoration::Image { .. } => {
                        placeholder(
                            renderer,
                            bounds,
                            clip,
                            theme,
                            "画像を表示できません。クリックでパスを編集",
                        );
                    }
                    Decoration::Rule { color, .. } => {
                        bounds.width = 3.0;
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds,
                                ..Default::default()
                            },
                            *color,
                        );
                    }
                    Decoration::Diagram {
                        handle: Some(handle),
                        ..
                    } => {
                        bounds = bounds.shrink(8.0);
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds,
                                ..Default::default()
                            },
                            Color::WHITE,
                        );
                        let native = renderer.measure_svg(handle);
                        let scale = (bounds.width / native.width as f32)
                            .min(bounds.height / native.height as f32);
                        let size =
                            Size::new(native.width as f32 * scale, native.height as f32 * scale);
                        let image_bounds = Rectangle::new(
                            iced::Point::new(
                                bounds.center_x() - size.width / 2.0,
                                bounds.center_y() - size.height / 2.0,
                            ),
                            size,
                        );
                        renderer.draw_svg(svg::Svg::new(handle.clone()), image_bounds, clip);
                    }
                    Decoration::Diagram { status, .. } => {
                        renderer.fill_text(
                            text::Text {
                                content: status.clone(),
                                bounds: bounds.shrink(12.0).size(),
                                size: 12.0.into(),
                                line_height: text::LineHeight::Relative(1.5),
                                font: iced::Font::with_name("Yu Gothic UI"),
                                align_x: text::Alignment::Default,
                                align_y: iced::alignment::Vertical::Top,
                                shaping: text::Shaping::Advanced,
                                wrapping: text::Wrapping::WordOrGlyph,
                            },
                            bounds.position() + iced::Vector::new(12.0, 12.0),
                            theme.palette().text,
                            clip,
                        );
                    }
                }
            }
        });
    }
}
