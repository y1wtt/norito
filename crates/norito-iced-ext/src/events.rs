//! Event adapters around a child widget.
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, input_method, layout, mouse, overlay, renderer,
    widget::{Operation, Tree},
};
use iced::{Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, Vector};

/// Emit the local cursor position when the right button is pressed over
/// `content` and the child did not capture it.
pub fn on_right_press_at<'a, M: 'a>(
    content: impl Into<Element<'a, M>>,
    on_press: impl Fn(Point) -> M + 'a,
) -> Element<'a, M> {
    Element::new(Adapter {
        content: content.into(),
        kind: Kind::RightPressAt(Box::new(on_press)),
    })
}

/// Drop empty IME commits before `input` sees them. Windows sends one when
/// composition is cancelled, and text inputs paste it over their selection.
pub fn keep_selection_on_ime_cancel<'a, M: 'a>(input: impl Into<Element<'a, M>>) -> Element<'a, M> {
    Element::new(Adapter {
        content: input.into(),
        kind: Kind::KeepSelectionOnImeCancel,
    })
}

enum Kind<'a, M> {
    RightPressAt(Box<dyn Fn(Point) -> M + 'a>),
    KeepSelectionOnImeCancel,
}

struct Adapter<'a, M> {
    content: Element<'a, M>,
    kind: Kind<'a, M>,
}

impl<M> Widget<M, Theme, Renderer> for Adapter<'_, M> {
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }
    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
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
        self.content
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
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        if let Kind::KeepSelectionOnImeCancel = self.kind
            && let Event::InputMethod(input_method::Event::Commit(text)) = event
            && text.is_empty()
        {
            return;
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        if let Kind::RightPressAt(on_press) = &self.kind
            && !shell.is_event_captured()
            && let Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) = event
            && let Some(point) = cursor.position_in(layout.bounds())
        {
            shell.publish(on_press(point));
            shell.capture_event();
        }
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
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
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}
