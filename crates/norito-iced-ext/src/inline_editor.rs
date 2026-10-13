//! An interactive child anchored to a logical editor line, within its viewport.
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, renderer,
    widget::{Operation, Tree},
};
use iced::widget::text_editor::Content;
use iced::{Element, Event, Length, Rectangle, Renderer, Size, Theme};

pub fn inline<'a, M: Clone + 'a>(
    base: Element<'a, M>,
    content: &'a Content,
    line: usize,
    child: Element<'a, M>,
    clicks: Vec<Vec<M>>,
) -> Element<'a, M> {
    Element::new(Inline {
        base,
        content,
        line,
        child,
        clicks,
    })
}
struct Inline<'a, M> {
    base: Element<'a, M>,
    content: &'a Content,
    line: usize,
    child: Element<'a, M>,
    clicks: Vec<Vec<M>>,
}
impl<M: Clone> Widget<M, Theme, Renderer> for Inline<'_, M> {
    fn size(&self) -> Size<Length> {
        self.base.as_widget().size()
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.base), Tree::new(&self.child)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.base, &self.child]);
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let base = self
            .base
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        let rect = self.content.line_bounds(self.line).unwrap_or(Rectangle {
            x: 0.0,
            y: -10000.0,
            width: base.size().width,
            height: 0.0,
        });
        let child = self.child.as_widget_mut().layout(
            &mut tree.children[1],
            renderer,
            &layout::Limits::new(Size::ZERO, Size::new(rect.width, 10000.0)),
        );
        let mut position = rect.position();
        position.x += crate::EDITOR_PADDING.left;
        position.y += crate::EDITOR_PADDING.top;
        if self.clicks.is_empty() {
            position.y = position
                .y
                .clamp(0.0, (base.size().height - child.size().height).max(0.0));
        }
        layout::Node::with_children(base.size(), vec![base, child.move_to(position)])
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.base.as_widget_mut().operate(
            &mut tree.children[0],
            layout.children().next().unwrap(),
            renderer,
            operation,
        );
        self.child.as_widget_mut().operate(
            &mut tree.children[1],
            layout.children().nth(1).unwrap(),
            renderer,
            operation,
        );
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        let Some(clip) = viewport.intersection(&layout.bounds()) else {
            return;
        };
        if matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        ) && cursor.is_over(clip)
        {
            let bounds = layout.children().nth(1).unwrap().bounds();
            if let Some(point) = cursor.position_in(bounds) {
                let columns = self.clicks.first().map_or(1, Vec::len).max(1);
                let r = (point.y / 34.0) as usize;
                let c = (point.x / (bounds.width / columns as f32)) as usize;
                if let Some(message) = self.clicks.get(r).and_then(|row| row.get(c)) {
                    shell.publish(message.clone());
                }
            }
        }
        // Wheel input always scrolls the document, including over the child.
        if !matches!(event, Event::Mouse(mouse::Event::WheelScrolled { .. })) {
            self.child.as_widget_mut().update(
                &mut tree.children[1],
                event,
                layout.children().nth(1).unwrap(),
                cursor,
                renderer,
                clipboard,
                shell,
                &clip,
            );
            if shell.is_event_captured() {
                return;
            }
        }
        self.base.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout.children().next().unwrap(),
            cursor,
            renderer,
            clipboard,
            shell,
            &clip,
        );
        shell.invalidate_layout();
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let child = layout.children().nth(1).unwrap();
        if cursor.is_over(child.bounds()) && cursor.is_over(layout.bounds()) {
            self.child.as_widget().mouse_interaction(
                &tree.children[1],
                child,
                cursor,
                viewport,
                renderer,
            )
        } else {
            self.base.as_widget().mouse_interaction(
                &tree.children[0],
                layout.children().next().unwrap(),
                cursor,
                viewport,
                renderer,
            )
        }
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
        use iced::advanced::Renderer as _;
        self.base.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout.children().next().unwrap(),
            cursor,
            viewport,
        );
        if let Some(clip) = viewport.intersection(&layout.bounds()) {
            renderer.with_layer(clip, |renderer| {
                self.child.as_widget().draw(
                    &tree.children[1],
                    renderer,
                    theme,
                    style,
                    layout.children().nth(1).unwrap(),
                    cursor,
                    &clip,
                )
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::{Font, widget::text_editor};
    #[test]
    fn child_starts_where_the_text_does() {
        let content = Content::with_text("Text\n ");
        let editor = text_editor::<(), Theme, Renderer>(&content)
            .padding(crate::EDITOR_PADDING)
            .height(400);
        let child = iced::widget::Space::new().width(100).height(34);
        let mut element = inline(editor.into(), &content, 1, child.into(), vec![]);
        let mut tree = Tree::new(&element);
        let renderer = Renderer::new(Font::DEFAULT, 15.0.into());
        let node = element.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(500.0, 400.0)),
        );
        let text = content.span_bounds(0, 0..1)[0].x + crate::EDITOR_PADDING.left;
        assert!((node.children()[1].bounds().x - text).abs() < 1.0);
    }
}
