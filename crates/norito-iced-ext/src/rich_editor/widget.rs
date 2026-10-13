//! The editor widget: input handling follows Iced's `TextEditor`, drawing
//! adds line and span backgrounds and the inline IME composition.
use super::{Composition, Content};
use crate::ink;
use crate::styled_text::Styles;
use iced::advanced::graphics::text as graphics;
use iced::advanced::text::{self, LineHeight, Wrapping};
use iced::advanced::widget::{self, Widget, operation, tree};
use iced::advanced::{
    Clipboard, InputMethod, Layout, Shell, clipboard, input_method, layout, mouse, renderer,
};
use iced::time::{Duration, Instant};
use iced::widget::text_editor::{Action, Binding, Edit, KeyPress, Status, Style, StyleFn};
use iced::{
    Color, Element, Event, Font, Length, Padding, Pixels, Point, Rectangle, Size, Theme, Vector,
    keyboard, window,
};
use std::{ops::Range, sync::Arc};

type KeyBinding<'a, Message> = Box<dyn Fn(KeyPress) -> Option<Binding<Message>> + 'a>;

pub fn rich_editor<'a, Message>(content: &'a Content) -> RichEditor<'a, Message> {
    RichEditor {
        id: None,
        content,
        font: None,
        text_size: None,
        line_height: LineHeight::default(),
        height: Length::Fill,
        padding: Padding::new(5.0),
        styles: Styles::default(),
        block_backgrounds: vec![],
        span_backgrounds: vec![],
        style: Box::new(iced::widget::text_editor::default),
        key_binding: None,
        on_edit: None,
        last_status: None,
    }
}

pub struct RichEditor<'a, Message> {
    id: Option<widget::Id>,
    content: &'a Content,
    font: Option<Font>,
    text_size: Option<Pixels>,
    line_height: LineHeight,
    height: Length,
    padding: Padding,
    styles: Styles,
    block_backgrounds: Vec<(Range<usize>, Color, f32)>,
    span_backgrounds: Vec<(usize, Range<usize>, Color)>,
    style: StyleFn<'a, Theme>,
    key_binding: Option<KeyBinding<'a, Message>>,
    on_edit: Option<Box<dyn Fn(Action) -> Message + 'a>>,
    last_status: Option<Status>,
}

impl<'a, Message> RichEditor<'a, Message> {
    pub fn id(mut self, id: impl Into<widget::Id>) -> Self {
        self.id = Some(id.into());
        self
    }
    pub fn font(mut self, font: impl Into<Font>) -> Self {
        self.font = Some(font.into());
        self
    }
    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.text_size = Some(size.into());
        self
    }
    pub fn line_height(mut self, line_height: impl Into<LineHeight>) -> Self {
        self.line_height = line_height.into();
        self
    }
    /// `Fill` or a fixed height; the text scrolls within it.
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }
    pub fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.padding = padding.into();
        self
    }
    /// Per-line spans, applied during layout so siblings can query line geometry.
    pub fn styles(mut self, styles: Styles) -> Self {
        self.styles = styles;
        self
    }
    /// Full-width backgrounds behind contiguous logical line ranges, with vertical padding.
    pub fn block_backgrounds(mut self, backgrounds: Vec<(Range<usize>, Color, f32)>) -> Self {
        self.block_backgrounds = backgrounds;
        self
    }
    /// Backgrounds behind UTF-8 ranges of logical lines.
    pub fn span_backgrounds(mut self, backgrounds: Vec<(usize, Range<usize>, Color)>) -> Self {
        self.span_backgrounds = backgrounds;
        self
    }
    pub fn style(mut self, style: impl Fn(&Theme, Status) -> Style + 'a) -> Self {
        self.style = Box::new(style);
        self
    }
    pub fn key_binding(
        mut self,
        key_binding: impl Fn(KeyPress) -> Option<Binding<Message>> + 'a,
    ) -> Self {
        self.key_binding = Some(Box::new(key_binding));
        self
    }
    /// Without it, the editor is disabled.
    pub fn on_action(mut self, on_edit: impl Fn(Action) -> Message + 'a) -> Self {
        self.on_edit = Some(Box::new(on_edit));
        self
    }

    fn input_method<'b>(
        &self,
        state: &'b State,
        layout: Layout<'_>,
        default_size: Pixels,
    ) -> InputMethod<&'b str> {
        let Some(Focus {
            is_window_focused: true,
            ..
        }) = &state.focus
        else {
            return InputMethod::Disabled;
        };
        let mut internal = self.content.0.borrow_mut();
        internal.compose(state.composition());
        let editor = internal.shown();
        let translation = layout.bounds().shrink(self.padding).position() - Point::ORIGIN;
        let caret = editor.caret_bounds().unwrap_or_else(|| {
            let position = match editor.selection() {
                text::editor::Selection::Caret(position) => position,
                text::editor::Selection::Range(ranges) => {
                    ranges.first().cloned().unwrap_or_default().position()
                }
            };
            let size = self.text_size.unwrap_or(default_size);
            Rectangle::new(
                position,
                Size::new(1.0, self.line_height.to_absolute(size).0),
            )
        });
        InputMethod::Enabled {
            cursor: caret + translation,
            purpose: input_method::Purpose::Normal,
            // A composition is drawn in place, so the system shows no overlay.
            preedit: if state.is_composing() {
                None
            } else {
                state.preedit.as_ref().map(input_method::Preedit::as_ref)
            },
        }
    }
}

struct State {
    focus: Option<Focus>,
    preedit: Option<input_method::Preedit>,
    last_click: Option<mouse::Click>,
    drag_click: Option<mouse::click::Kind>,
    partial_scroll: f32,
}

impl State {
    fn composition(&self) -> Option<Composition> {
        let preedit = self.preedit.as_ref().filter(|_| self.focus.is_some())?;
        Some(Composition {
            content: preedit.content.clone(),
            selection: preedit.selection.clone(),
        })
    }
    fn is_composing(&self) -> bool {
        self.preedit.as_ref().is_some_and(|p| !p.content.is_empty())
    }
}

#[derive(Debug, Clone)]
struct Focus {
    updated_at: Instant,
    now: Instant,
    is_window_focused: bool,
}

impl Focus {
    const CURSOR_BLINK_INTERVAL_MILLIS: u128 = 500;

    fn now() -> Self {
        let now = Instant::now();
        Self {
            updated_at: now,
            now,
            is_window_focused: true,
        }
    }
    fn is_cursor_visible(&self) -> bool {
        self.is_window_focused
            && ((self.now - self.updated_at).as_millis() / Self::CURSOR_BLINK_INTERVAL_MILLIS)
                .is_multiple_of(2)
    }
}

impl operation::Focusable for State {
    fn is_focused(&self) -> bool {
        self.focus.is_some()
    }
    fn focus(&mut self) {
        self.focus = Some(Focus::now());
    }
    fn unfocus(&mut self) {
        self.focus = None;
    }
}

impl<Message, Renderer> Widget<Message, Theme, Renderer> for RichEditor<'_, Message>
where
    Renderer: text::Renderer<Font = Font> + graphics::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State {
            focus: None,
            preedit: None,
            last_click: None,
            drag_click: None,
            partial_scroll: 0.0,
        })
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, self.height)
    }
    fn layout(
        &mut self,
        _tree: &mut widget::Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let limits = limits.width(Length::Fill).height(self.height);
        let mut internal = self.content.0.borrow_mut();
        internal.preedit = None;
        internal.editor.update(
            limits.shrink(self.padding).max(),
            self.font.unwrap_or_else(|| renderer.default_font()),
            self.text_size.unwrap_or_else(|| renderer.default_size()),
            self.line_height,
            Wrapping::default(),
            &self.styles,
        );
        layout::Node::new(limits.max())
    }
    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let Some(on_edit) = self.on_edit.as_ref() else {
            return;
        };
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Window(window::Event::Unfocused) => {
                state.preedit = None;
                if let Some(focus) = &mut state.focus {
                    focus.is_window_focused = false;
                }
            }
            Event::Window(window::Event::Focused) => {
                if let Some(focus) = &mut state.focus {
                    focus.is_window_focused = true;
                    focus.updated_at = Instant::now();
                    shell.request_redraw();
                }
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                if let Some(focus) = &mut state.focus
                    && focus.is_window_focused
                {
                    focus.now = *now;
                    let blink = Focus::CURSOR_BLINK_INTERVAL_MILLIS;
                    let millis = blink - (focus.now - focus.updated_at).as_millis() % blink;
                    shell.request_redraw_at(focus.now + Duration::from_millis(millis as u64));
                }
            }
            _ => {}
        }
        if let Some(update) = Update::from_event(
            event,
            state,
            layout.bounds(),
            self.padding,
            cursor,
            self.key_binding.as_deref(),
        ) {
            match update {
                Update::Click(click) => {
                    let action = match click.kind() {
                        mouse::click::Kind::Single => Action::Click(click.position()),
                        mouse::click::Kind::Double => Action::SelectWord,
                        mouse::click::Kind::Triple => Action::SelectLine,
                    };
                    state.focus = Some(Focus::now());
                    state.last_click = Some(click);
                    state.drag_click = Some(click.kind());
                    shell.publish(on_edit(action));
                    shell.capture_event();
                }
                Update::Drag(position) => shell.publish(on_edit(Action::Drag(position))),
                Update::Release => state.drag_click = None,
                Update::Scroll(lines) => {
                    let lines = lines + state.partial_scroll;
                    state.partial_scroll = lines.fract();
                    shell.publish(on_edit(Action::Scroll {
                        lines: lines as i32,
                    }));
                    shell.capture_event();
                }
                Update::Preedit(preedit) => {
                    state.preedit = preedit.map(|preedit| input_method::Preedit {
                        text_size: self.text_size,
                        ..preedit
                    });
                    shell.request_redraw();
                }
                Update::Commit(text) => {
                    state.preedit = None;
                    shell.request_redraw();
                    // Windows sends an empty commit when composition is cancelled.
                    // Pasting it would delete the selected text.
                    if !text.is_empty() {
                        shell.publish(on_edit(Action::Edit(Edit::Paste(Arc::new(text)))));
                    }
                }
                Update::Binding(binding) => {
                    if !matches!(binding, Binding::Unfocus) {
                        shell.capture_event();
                    }
                    apply(binding, self.content, state, on_edit, clipboard, shell);
                    if let Some(focus) = &mut state.focus {
                        focus.updated_at = Instant::now();
                    }
                }
            }
        }
        let is_hovered = cursor.is_over(layout.bounds());
        let status = if state.focus.is_some() {
            Status::Focused { is_hovered }
        } else if is_hovered {
            Status::Hovered
        } else {
            Status::Active
        };
        if let Event::Window(window::Event::RedrawRequested(_)) = event {
            self.last_status = Some(status);
            shell.request_input_method(&self.input_method(state, layout, renderer.default_size()));
        } else if self.last_status.is_some_and(|last| last != status) {
            shell.request_redraw();
        }
    }
    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree.state.downcast_ref::<State>();
        let mut internal = self.content.0.borrow_mut();
        internal.compose(state.composition());
        let editor = internal.shown();
        let status = if self.on_edit.is_none() {
            Status::Disabled
        } else {
            self.last_status.unwrap_or(Status::Active)
        };
        let style = (self.style)(theme, status);
        let quad = |renderer: &mut Renderer, bounds, color: Color| {
            renderer.fill_quad(
                renderer::Quad {
                    bounds,
                    ..Default::default()
                },
                color,
            );
        };
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: style.border,
                ..Default::default()
            },
            style.background,
        );
        let text_bounds = bounds.shrink(self.padding);
        let translation = text_bounds.position() - Point::ORIGIN;
        for (lines, color, padding) in &self.block_backgrounds {
            let Some(rect) = lines
                .clone()
                .filter_map(|line| editor.line_bounds(line))
                .reduce(|a, b| a.union(&b))
            else {
                continue;
            };
            let rect = Rectangle {
                x: bounds.x,
                y: rect.y + translation.y - padding,
                width: bounds.width,
                height: rect.height + padding * 2.0,
            };
            if let Some(rect) = rect.intersection(&bounds) {
                quad(renderer, rect, *color);
            }
        }
        for (line, range, color) in &self.span_backgrounds {
            for rect in editor.span_bounds(*line, range.clone()) {
                if let Some(rect) = (rect + translation).intersection(&bounds) {
                    quad(renderer, rect, *color);
                }
            }
        }
        // Quads stay in this layer, below the text layer: selection and caret
        // are painted under the glyphs, as in Iced's editor.
        for rect in editor.preedit_rectangles() {
            let underline = Rectangle {
                x: rect.x + translation.x,
                y: rect.y + translation.y + rect.height - 2.0,
                width: rect.width,
                height: 1.0,
            };
            if let Some(underline) = text_bounds.intersection(&underline) {
                quad(renderer, underline, style.value);
            }
        }
        if let Some(focus) = &state.focus {
            match editor.selection() {
                text::editor::Selection::Caret(_) if focus.is_cursor_visible() => {
                    if let Some(caret) = editor
                        .caret_bounds()
                        .and_then(|caret| text_bounds.intersection(&(caret + translation)))
                    {
                        quad(renderer, caret, style.value);
                    }
                }
                text::editor::Selection::Range(ranges) => {
                    for range in ranges
                        .into_iter()
                        .filter_map(|range| text_bounds.intersection(&(range + translation)))
                    {
                        quad(renderer, range, style.selection);
                    }
                }
                text::editor::Selection::Caret(_) => {}
            }
        }
        // A nested layer is not clipped by its parent: clip to the viewport too.
        let Some(clip) = text_bounds.intersection(viewport) else {
            return;
        };
        let frame = Arc::new(editor.snapshot());
        ink::fill(renderer, &frame, text_bounds.position(), style.value, clip);
        internal.frame = Some(frame);
    }
    fn mouse_interaction(
        &self,
        _tree: &widget::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if !cursor.is_over(layout.bounds()) {
            mouse::Interaction::default()
        } else if self.on_edit.is_none() {
            mouse::Interaction::NotAllowed
        } else {
            mouse::Interaction::Text
        }
    }
    fn operate(
        &mut self,
        tree: &mut widget::Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        let state = tree.state.downcast_mut::<State>();
        operation.focusable(self.id.as_ref(), layout.bounds(), state);
    }
}

impl<'a, Message: 'a, Renderer> From<RichEditor<'a, Message>>
    for Element<'a, Message, Theme, Renderer>
where
    Renderer: text::Renderer<Font = Font> + graphics::Renderer,
{
    fn from(editor: RichEditor<'a, Message>) -> Self {
        Self::new(editor)
    }
}

fn apply<Message>(
    binding: Binding<Message>,
    content: &Content,
    state: &mut State,
    on_edit: &dyn Fn(Action) -> Message,
    clipboard: &mut dyn Clipboard,
    shell: &mut Shell<'_, Message>,
) {
    let mut publish = |action| shell.publish(on_edit(action));
    match binding {
        Binding::Unfocus => {
            state.preedit = None;
            state.focus = None;
            state.drag_click = None;
        }
        Binding::Copy => {
            if let Some(selection) = content.selection() {
                clipboard.write(clipboard::Kind::Standard, selection);
            }
        }
        Binding::Cut => {
            if let Some(selection) = content.selection() {
                clipboard.write(clipboard::Kind::Standard, selection);
                publish(Action::Edit(Edit::Delete));
            }
        }
        Binding::Paste => {
            if let Some(contents) = clipboard.read(clipboard::Kind::Standard) {
                publish(Action::Edit(Edit::Paste(Arc::new(contents))));
            }
        }
        Binding::Move(motion) => publish(Action::Move(motion)),
        Binding::Select(motion) => publish(Action::Select(motion)),
        Binding::SelectWord => publish(Action::SelectWord),
        Binding::SelectLine => publish(Action::SelectLine),
        Binding::SelectAll => publish(Action::SelectAll),
        Binding::Insert(c) => publish(Action::Edit(Edit::Insert(c))),
        Binding::Enter => publish(Action::Edit(Edit::Enter)),
        Binding::Backspace => publish(Action::Edit(Edit::Backspace)),
        Binding::Delete => publish(Action::Edit(Edit::Delete)),
        Binding::Sequence(sequence) => {
            for binding in sequence {
                apply(binding, content, state, on_edit, clipboard, shell);
            }
        }
        Binding::Custom(message) => shell.publish(message),
    }
}

enum Update<Message> {
    Click(mouse::Click),
    Drag(Point),
    Release,
    Scroll(f32),
    Preedit(Option<input_method::Preedit>),
    Commit(String),
    Binding(Binding<Message>),
}

impl<Message> Update<Message> {
    fn from_event(
        event: &Event,
        state: &State,
        bounds: Rectangle,
        padding: Padding,
        cursor: mouse::Cursor,
        key_binding: Option<&dyn Fn(KeyPress) -> Option<Binding<Message>>>,
    ) -> Option<Self> {
        let padding = Vector::new(padding.left, padding.top);
        match event {
            Event::Mouse(event) => match event {
                mouse::Event::ButtonPressed(mouse::Button::Left) => {
                    if let Some(position) = cursor.position_in(bounds) {
                        Some(Update::Click(mouse::Click::new(
                            position - padding,
                            mouse::Button::Left,
                            state.last_click,
                        )))
                    } else if state.focus.is_some() {
                        Some(Update::Binding(Binding::Unfocus))
                    } else {
                        None
                    }
                }
                mouse::Event::ButtonReleased(mouse::Button::Left) => Some(Update::Release),
                mouse::Event::CursorMoved { .. } => match state.drag_click {
                    Some(mouse::click::Kind::Single) => {
                        Some(Update::Drag(cursor.position_in(bounds)? - padding))
                    }
                    _ => None,
                },
                mouse::Event::WheelScrolled { delta } if cursor.is_over(bounds) => {
                    Some(Update::Scroll(match delta {
                        mouse::ScrollDelta::Lines { y, .. } if y.abs() > 0.0 => {
                            y.signum() * -(y.abs() * 4.0).max(1.0)
                        }
                        mouse::ScrollDelta::Lines { .. } => 0.0,
                        mouse::ScrollDelta::Pixels { y, .. } => -y / 4.0,
                    }))
                }
                _ => None,
            },
            Event::InputMethod(event) => match event {
                input_method::Event::Opened => {
                    Some(Update::Preedit(Some(input_method::Preedit::new())))
                }
                input_method::Event::Closed => Some(Update::Preedit(None)),
                input_method::Event::Preedit(content, selection) if state.focus.is_some() => {
                    Some(Update::Preedit(Some(input_method::Preedit {
                        content: content.clone(),
                        selection: selection.clone(),
                        text_size: None,
                    })))
                }
                input_method::Event::Commit(content) if state.focus.is_some() => {
                    Some(Update::Commit(content.clone()))
                }
                _ => None,
            },
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modified_key,
                physical_key,
                modifiers,
                text,
                ..
            }) => {
                // Keys go to the IME while it composes.
                if state.is_composing() {
                    return None;
                }
                let key_press = KeyPress {
                    key: key.clone(),
                    modified_key: modified_key.clone(),
                    physical_key: *physical_key,
                    modifiers: *modifiers,
                    text: text.clone(),
                    status: if state.focus.is_some() {
                        Status::Focused {
                            is_hovered: cursor.is_over(bounds),
                        }
                    } else {
                        Status::Active
                    },
                };
                match key_binding {
                    Some(key_binding) => key_binding(key_press),
                    None => Binding::from_key_press(key_press),
                }
                .map(Update::Binding)
            }
            _ => None,
        }
    }
}
