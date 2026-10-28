//! Viewport-aligned decorations for a continuous editor. No Markdown knowledge.
use crate::{fonts, ink, rich_editor::Content};
use iced::advanced::graphics::text::cosmic_text::Buffer;
use iced::advanced::{
    Clipboard, Layout, Renderer as _, Shell, Widget,
    image::{self, Renderer as _},
    layout, mouse, renderer,
    svg::{self, Renderer as _},
    widget::{Operation, Tree, tree},
};
use iced::{Color, Element, Event, Length, Point, Rectangle, Renderer, Size, Theme};
use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{Arc, LazyLock},
};

/// The tick of a checked box, as a path rather than a font glyph.
static TICK: LazyLock<svg::Handle> = LazyLock::new(|| {
    svg::Handle::from_memory(
        br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M3.8 8.3l2.8 2.8 5.6-6.2" fill="none" stroke="black" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/></svg>"#
            .as_slice(),
    )
});

type LabelKey = (String, u32, Option<u32>);

/// Labels shaped for the last frame. Renderers reference them weakly until
/// the frame is presented.
#[derive(Default)]
struct Labels(RefCell<HashMap<LabelKey, Arc<Buffer>>>);

/// Draws one frame's labels, reusing those whose text and width are unchanged.
/// Each label is clipped to its own area, which the renderer also redraws.
struct Painter {
    previous: HashMap<LabelKey, Arc<Buffer>>,
    current: HashMap<LabelKey, Arc<Buffer>>,
}
impl Painter {
    fn label(&mut self, content: &str, size: f32, width: Option<f32>) -> Arc<Buffer> {
        let key = (content.to_owned(), size.to_bits(), width.map(f32::to_bits));
        if let Some(label) = self.current.get(&key) {
            return label.clone();
        }
        let label = self.previous.remove(&key).unwrap_or_else(|| {
            let line_height = if width.is_some() { 1.5 } else { 1.4 };
            Arc::new(ink::label(
                content,
                fonts::TEXT,
                size,
                size * line_height,
                width,
            ))
        });
        self.current.insert(key, label.clone());
        label
    }
    /// Small text wrapped within `bounds`.
    fn note(
        &mut self,
        renderer: &mut Renderer,
        bounds: Rectangle,
        clip: Rectangle,
        color: Color,
        content: &str,
    ) {
        let label = self.label(content, 12.0, Some(bounds.width));
        if let Some(clip) = bounds.intersection(&clip) {
            ink::fill(renderer, [(&label, bounds.position())], color, clip);
        }
    }
}

/// An SVG with its size, and its baseline's distance from the top, in any one unit.
#[derive(Debug, Clone)]
pub struct Figure {
    pub handle: svg::Handle,
    pub size: Size,
    pub baseline: f32,
}

pub enum Decoration<Message> {
    /// Set in the text color, as wide as its span, its baseline on the text's.
    InlineDiagram {
        line: usize,
        range: std::ops::Range<usize>,
        figure: Option<Figure>,
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
    /// A box over a blank placeholder in the text.
    Checkbox {
        line: usize,
        range: std::ops::Range<usize>,
        checked: bool,
    },
    /// A list bullet over a blank placeholder: a disc, circle or square by depth.
    Bullet {
        line: usize,
        range: std::ops::Range<usize>,
        depth: u8,
    },
    Separator {
        line: usize,
    },
    Rule {
        line: usize,
        color: Color,
    },
    /// Centered over its line, or `status` until there is an SVG.
    Diagram {
        line: usize,
        handle: Option<svg::Handle>,
        /// Draw at this size, shrunk to fit, in the text color. Without one the SVG
        /// keeps its own colors on a white panel it fills.
        size: Option<Size>,
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
            | Self::Checkbox { line, .. }
            | Self::Bullet { line, .. }
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
        if let Decoration::InlineDiagram { line, range, .. }
        | Decoration::Checkbox { line, range, .. }
        | Decoration::Bullet { line, range, .. } = decoration
        {
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
    /// The line within the text column, so blocks start where text does.
    fn bounds(&self, line: usize, layout: Layout<'_>) -> Option<Rectangle> {
        self.content.line_bounds(line).map(|r| {
            r + iced::Vector::new(
                layout.bounds().x + crate::EDITOR_PADDING.left,
                layout.bounds().y + crate::EDITOR_PADDING.top,
            )
        })
    }
}
impl<Message: Clone> Widget<Message, Theme, Renderer> for Decorated<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Labels>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(Labels::default())
    }
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
        let labels = tree.state.downcast_ref::<Labels>();
        let mut painter = Painter {
            previous: labels.0.take(),
            current: HashMap::new(),
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
                        range,
                        figure: Some(figure),
                        ..
                    } => {
                        let scale = bounds.width / figure.size.width;
                        let baseline = self
                            .content
                            .span_baseline(line, range.clone())
                            .map_or(bounds.center_y(), |y| {
                                y + layout.bounds().y + crate::EDITOR_PADDING.top
                            });
                        renderer.draw_svg(
                            svg::Svg::new(figure.handle.clone()).color(theme.palette().text),
                            Rectangle::new(
                                iced::Point::new(bounds.x, baseline - figure.baseline * scale),
                                Size::new(bounds.width, figure.size.height * scale),
                            ),
                            clip,
                        );
                    }
                    Decoration::InlineDiagram { .. } => {
                        painter.note(renderer, bounds, clip, theme.palette().text, "数式…");
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
                    Decoration::Checkbox { checked, .. } => {
                        let side = (bounds.width * 0.87).round();
                        let square = Rectangle {
                            x: (bounds.center_x() - side / 2.0).round(),
                            y: (bounds.center_y() - side / 2.0).round(),
                            width: side,
                            height: side,
                        };
                        let accent = theme.palette().primary;
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: square,
                                border: iced::Border {
                                    width: 1.5,
                                    color: if *checked {
                                        accent
                                    } else {
                                        theme.palette().text.scale_alpha(0.6)
                                    },
                                    radius: 3.0.into(),
                                },
                                ..Default::default()
                            },
                            if *checked { accent } else { Color::TRANSPARENT },
                        );
                        if *checked {
                            renderer.draw_svg(
                                svg::Svg::new(TICK.clone()).color(theme.palette().background),
                                square,
                                clip,
                            );
                        }
                    }
                    Decoration::Bullet { depth, .. } => {
                        let color = theme.palette().text;
                        let side = (bounds.width * 0.36).round().max(4.0);
                        let (side, fill, border) = match depth % 3 {
                            0 => (side, color, iced::Border::default().rounded(side / 2.0)),
                            1 => (
                                side,
                                Color::TRANSPARENT,
                                iced::Border::default()
                                    .rounded(side / 2.0)
                                    .width(1.2)
                                    .color(color),
                            ),
                            _ => ((side * 0.9).round(), color, iced::Border::default()),
                        };
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: Rectangle {
                                    x: (bounds.center_x() - side / 2.0).round(),
                                    y: (bounds.center_y() - side / 2.0).round(),
                                    width: side,
                                    height: side,
                                },
                                border,
                                ..Default::default()
                            },
                            fill,
                        );
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
                                let inner = cell.shrink(7.0);
                                let label = painter.label(value, 14.0, None);
                                let width = ink::width(&label);
                                let x = match alignment.get(c) {
                                    Some(1) => inner.center_x() - width / 2.0,
                                    Some(2) => inner.x + inner.width - width,
                                    _ => inner.x,
                                };
                                if let Some(cell_clip) = cell.intersection(&clip) {
                                    ink::fill(
                                        renderer,
                                        [(&label, Point::new(x, inner.y))],
                                        theme.palette().text,
                                        cell_clip,
                                    );
                                }
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
                        painter.note(
                            renderer,
                            bounds,
                            clip,
                            theme.palette().text,
                            "画像を表示できません。クリックでパスを編集",
                        );
                    }
                    Decoration::Rule { color, .. } => {
                        // In the left padding, beside the text it marks.
                        bounds.x = layout.bounds().x;
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
                        size,
                        ..
                    } => {
                        bounds = bounds.shrink([8.0, 0.0]);
                        let mut image = svg::Svg::new(handle.clone());
                        let size = if let Some(size) = size {
                            image = image.color(theme.palette().text);
                            let scale = (bounds.width / size.width)
                                .min(bounds.height / size.height)
                                .min(1.0);
                            Size::new(size.width * scale, size.height * scale)
                        } else {
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
                            Size::new(native.width as f32 * scale, native.height as f32 * scale)
                        };
                        let image_bounds = Rectangle::new(
                            iced::Point::new(
                                bounds.center_x() - size.width / 2.0,
                                bounds.center_y() - size.height / 2.0,
                            ),
                            size,
                        );
                        renderer.draw_svg(image, image_bounds, clip);
                    }
                    Decoration::Diagram { status, .. } => {
                        painter.note(
                            renderer,
                            bounds.shrink([12.0, 0.0]),
                            clip,
                            theme.palette().text,
                            status,
                        );
                    }
                }
            }
        });
        *labels.0.borrow_mut() = painter.current;
    }
}
