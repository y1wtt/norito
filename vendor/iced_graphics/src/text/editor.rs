//! Draw and edit text.
use crate::core::text::editor::{
    self, Action, Cursor, Direction, Edit, Motion, Position, Selection,
};
use crate::core::text::highlighter::{self, Highlighter};
use crate::core::text::{LineHeight, Wrapping};
use crate::core::{Font, Pixels, Point, Rectangle, Size};
use crate::text;

use cosmic_text::Edit as _;

use std::borrow::Cow;
use std::fmt;
use std::sync::{self, Arc, RwLock};

/// A multi-line text editor.
#[derive(Debug, PartialEq)]
pub struct Editor(Option<Arc<Internal>>);

struct Internal {
    render_buffer: sync::OnceLock<Arc<cosmic_text::Buffer>>,
    preedit: Option<(cosmic_text::Cursor, cosmic_text::Cursor)>,
    editor: cosmic_text::Editor<'static>,
    selection: RwLock<Option<Selection>>,
    font: Font,
    bounds: Size,
    topmost_line_changed: Option<usize>,
    version: text::Version,
}

impl Editor {
    /// Creates a new empty [`Editor`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the buffer of the [`Editor`].
    pub fn buffer(&self) -> &cosmic_text::Buffer {
        buffer_from_editor(&self.internal().editor)
    }

    /// Creates a [`Weak`] reference to the [`Editor`].
    ///
    /// This is useful to avoid cloning the [`Editor`] when
    /// referential guarantees are unnecessary. For instance,
    /// when creating a rendering tree.
    pub fn downgrade(&self) -> Weak {
        let editor = self.internal();

        Weak {
            raw: Arc::downgrade(editor),
            bounds: editor.bounds,
            buffer: editor.render_buffer.get_or_init(|| Arc::new(self.buffer().clone())).clone(),
        }
    }

    fn internal(&self) -> &Arc<Internal> {
        self.0
            .as_ref()
            .expect("Editor should always be initialized")
    }

    fn with_internal_mut<T>(
        &mut self,
        f: impl FnOnce(&mut Internal) -> T,
    ) -> T {
        let editor =
            self.0.take().expect("Editor should always be initialized");

        // TODO: Handle multiple strong references somehow
        let mut internal = Arc::try_unwrap(editor)
            .expect("Editor cannot have multiple strong references");

        let _ = internal.render_buffer.take();

        // Clear cursor cache
        let _ = internal
            .selection
            .write()
            .expect("Write to cursor cache")
            .take();

        let result = f(&mut internal);

        self.0 = Some(Arc::new(internal));

        result
    }
}

impl editor::Editor for Editor {
    type Font = Font;

    fn with_text(text: &str) -> Self {
        let mut buffer = cosmic_text::Buffer::new_empty(cosmic_text::Metrics {
            font_size: 1.0,
            line_height: 1.0,
        });

        let mut font_system =
            text::font_system().write().expect("Write font system");

        buffer.set_text(
            font_system.raw(),
            text,
            &cosmic_text::Attrs::new(),
            cosmic_text::Shaping::Advanced,
            None,
        );

        Editor(Some(Arc::new(Internal {
            editor: cosmic_text::Editor::new(buffer),
            version: font_system.version(),
            ..Default::default()
        })))
    }

    fn is_empty(&self) -> bool {
        let buffer = self.buffer();

        buffer.lines.is_empty()
            || (buffer.lines.len() == 1 && buffer.lines[0].text().is_empty())
    }

    fn line(&self, index: usize) -> Option<editor::Line<'_>> {
        self.buffer().lines.get(index).map(|line| editor::Line {
            text: Cow::Borrowed(line.text()),
            ending: match line.ending() {
                cosmic_text::LineEnding::Lf => editor::LineEnding::Lf,
                cosmic_text::LineEnding::CrLf => editor::LineEnding::CrLf,
                cosmic_text::LineEnding::Cr => editor::LineEnding::Cr,
                cosmic_text::LineEnding::LfCr => editor::LineEnding::LfCr,
                cosmic_text::LineEnding::None => editor::LineEnding::None,
            },
        })
    }

    fn line_count(&self) -> usize {
        self.buffer().lines.len()
    }

    fn copy(&self) -> Option<String> {
        self.internal().editor.copy_selection()
    }

    fn selection(&self) -> editor::Selection {
        let internal = self.internal();

        if let Ok(Some(cursor)) = internal.selection.read().as_deref() {
            return cursor.clone();
        }

        let buffer = buffer_from_editor(&internal.editor);
        // Rich spans have different line heights. Use actual layout coordinates.
        let cursor = match internal.editor.selection_bounds() {
            Some((start, end)) => Selection::Range(buffer.layout_runs().filter_map(|run| {
                run.highlight(start, end).and_then(|(x, width)| {
                    let x = x + line_inset(buffer, run.line_i);
                    (width > 0.0).then_some(Rectangle { x, y: run.line_top, width, height: run.line_height })
                })
            }).collect()),
            None => {
                let line = internal.editor.cursor().line;
                let (x, y) = internal.editor.cursor_position().unwrap_or((-100, -100));
                Selection::Caret(Point::new(x as f32 + line_inset(buffer, line), y as f32))
            }
        };
        *internal.selection.write().expect("Write to cursor cache") =
            Some(cursor.clone());

        cursor
    }

    fn caret_height(&self) -> Option<f32> {
        let editor = &self.internal().editor;
        let (_, y) = editor.cursor_position()?;
        buffer_from_editor(editor).layout_runs().find(|run| (run.line_top-y as f32).abs() < 1.1).map(|run| run.line_height)
    }

    fn caret_bounds(&self) -> Option<Rectangle> {
        let editor = &self.internal().editor;
        let (x, y) = editor.cursor_position()?;
        let x = x as f32 + line_inset(self.buffer(), editor.cursor().line);
        Some(Rectangle::new(Point::new(x, y as f32), Size::new(1.0, self.caret_height()?)))
    }

    fn line_bounds(&self, line: usize) -> Option<Rectangle> {
        let mut runs = self.buffer().layout_runs().filter(|run| run.line_i == line);
        let first = runs.next()?;
        let top = first.line_top;
        let bottom = runs.last().map_or(first.line_top + first.line_height, |run| run.line_top + run.line_height);
        Some(Rectangle::new(Point::new(0.0, top), Size::new(self.bounds().width, bottom - top)))
    }
    fn span_bounds(&self, line: usize, range: std::ops::Range<usize>) -> Vec<Rectangle> {
        let inset = line_inset(self.buffer(), line);
        self.buffer().layout_runs().filter(|run| run.line_i == line).filter_map(|run| {
            let mut left = f32::INFINITY;
            let mut right = f32::NEG_INFINITY;
            for glyph in run.glyphs.iter().filter(|g| g.start < range.end && g.end > range.start) {
                left = left.min(glyph.x); right = right.max(glyph.x + glyph.w);
            }
            (right > left).then(|| Rectangle::new(Point::new(left + inset, run.line_top), Size::new(right - left, run.line_height)))
        }).collect()
    }
    fn span_baseline(&self, line: usize, range: std::ops::Range<usize>) -> Option<f32> {
        self.buffer().layout_runs().filter(|run| run.line_i == line)
            .find(|run| run.glyphs.iter().any(|g| g.start < range.end && g.end > range.start))
            .map(|run| run.line_y)
    }

    fn with_preedit(&self, content: &str, selection: Option<std::ops::Range<usize>>) -> Option<Self> {
        let original = self.internal();
        let mut editor = cosmic_text::Editor::new(self.buffer().clone());
        editor.set_cursor(original.editor.cursor());
        editor.set_selection(original.editor.selection());
        let start = editor.selection_bounds().map_or(editor.cursor(), |(start, _)| start);
        // Empty spacer lines become normal paragraphs while composing.
        if let Some(line) = buffer_mut_from_editor(&mut editor).lines.get_mut(start.line)
            && line.text().is_empty()
            && line.attrs_list().defaults().family == text::to_attributes(original.font).family
            && line.attrs_list().defaults().metrics_opt.is_some_and(|metrics| {
                cosmic_text::Metrics::from(metrics).font_size < self.buffer().metrics().font_size
            }) {
            let _ = line.set_attrs_list(cosmic_text::AttrsList::new(&text::to_attributes(original.font)));
        }
        // cosmic-text inherits the insertion point's font, size and line metrics.
        editor.insert_string(content, None);
        let end = editor.cursor();
        // Editing resets line metadata; composition keeps the line's inset.
        let kept = line_inset(self.buffer(), start.line);
        for line in &mut buffer_mut_from_editor(&mut editor).lines[start.line..=end.line] {
            line.set_metadata(kept.to_bits() as usize);
        }
        if let Some(range) = selection {
            let position = |offset: usize| {
                let mut offset = offset.min(content.len());
                while !content.is_char_boundary(offset) { offset -= 1; }
                let prefix = &content[..offset];
                let lines = prefix.bytes().filter(|b| *b == b'\n').count();
                cosmic_text::Cursor::new(start.line + lines, if lines == 0 { start.index + offset } else { prefix.rsplit('\n').next().unwrap_or_default().len() })
            };
            editor.set_cursor(position(range.end));
            editor.set_selection(if range.start == range.end { cosmic_text::Selection::None } else { cosmic_text::Selection::Normal(position(range.start)) });
        }
        let mut font_system = text::font_system().write().expect("Write font system");
        editor.shape_as_needed(font_system.raw(), true);
        wrap_insets(buffer_mut_from_editor(&mut editor), font_system.raw());
        Some(Self(Some(Arc::new(Internal {
            editor,
            preedit: Some((start, end)),
            font: original.font,
            bounds: original.bounds,
            version: font_system.version(),
            ..Default::default()
        }))))
    }

    fn preedit_rectangles(&self) -> Vec<Rectangle> {
        let Some((start, end)) = self.internal().preedit else { return vec![]; };
        let buffer = self.buffer();
        buffer.layout_runs().filter_map(|run| {
            run.highlight(start, end).and_then(|(x, width)| {
                let x = x + line_inset(buffer, run.line_i);
                (width > 0.0).then_some(Rectangle { x, y: run.line_top, width, height: run.line_height })
            })
        }).collect()
    }

    fn cursor(&self) -> Cursor {
        let editor = &self.internal().editor;

        // Word and line selections store their original click anchor, not their
        // expanded bounds. Expose the same range that is painted and copied.
        if matches!(editor.selection(), cosmic_text::Selection::Word(_) | cosmic_text::Selection::Line(_))
            && let Some((start, end)) = editor.selection_bounds()
        {
            return Cursor {
                position: Position { line: end.line, column: end.index },
                selection: Some(Position { line: start.line, column: start.index }),
            };
        }

        let position = {
            let cursor = editor.cursor();

            Position {
                line: cursor.line,
                column: cursor.index,
            }
        };

        let selection = match editor.selection() {
            cosmic_text::Selection::None => None,
            cosmic_text::Selection::Normal(cursor)
            | cosmic_text::Selection::Line(cursor)
            | cosmic_text::Selection::Word(cursor) => Some(Position {
                line: cursor.line,
                column: cursor.index,
            }),
        };

        Cursor {
            position,
            selection,
        }
    }

    fn perform(&mut self, action: Action) {
        let mut font_system =
            text::font_system().write().expect("Write font system");

        self.with_internal_mut(|internal| {
            let editor = &mut internal.editor;

            match action {
                // Motion events
                Action::Move(motion) => {
                    if let Some((start, end)) = editor.selection_bounds() {
                        editor.set_selection(cosmic_text::Selection::None);

                        match motion {
                            // These motions are performed as-is even when a selection
                            // is present
                            Motion::Home
                            | Motion::End
                            | Motion::DocumentStart
                            | Motion::DocumentEnd => {
                                editor.action(
                                    font_system.raw(),
                                    cosmic_text::Action::Motion(to_motion(
                                        motion,
                                    )),
                                );
                            }
                            // Other motions simply move the cursor to one end of the selection
                            _ => editor.set_cursor(match motion.direction() {
                                Direction::Left => start,
                                Direction::Right => end,
                            }),
                        }
                    } else {
                        editor.action(
                            font_system.raw(),
                            cosmic_text::Action::Motion(to_motion(motion)),
                        );
                    }
                }

                // Selection events
                Action::Select(motion) => {
                    let cursor = editor.cursor();

                    if editor.selection_bounds().is_none() {
                        editor.set_selection(cosmic_text::Selection::Normal(
                            cursor,
                        ));
                    }

                    editor.action(
                        font_system.raw(),
                        cosmic_text::Action::Motion(to_motion(motion)),
                    );

                    // Deselect if selection matches cursor position
                    if let Some((start, end)) = editor.selection_bounds()
                        && start.line == end.line
                        && start.index == end.index
                    {
                        editor.set_selection(cosmic_text::Selection::None);
                    }
                }
                Action::SelectWord => {
                    let cursor = editor.cursor();

                    editor.set_selection(cosmic_text::Selection::Word(cursor));
                }
                Action::SelectLine => {
                    let cursor = editor.cursor();

                    editor.set_selection(cosmic_text::Selection::Line(cursor));
                }
                Action::SelectAll => {
                    let buffer = buffer_from_editor(editor);

                    if buffer.lines.len() > 1
                        || buffer
                            .lines
                            .first()
                            .is_some_and(|line| !line.text().is_empty())
                    {
                        let cursor = editor.cursor();

                        editor.set_selection(cosmic_text::Selection::Normal(
                            cosmic_text::Cursor {
                                line: 0,
                                index: 0,
                                ..cursor
                            },
                        ));

                        editor.action(
                            font_system.raw(),
                            cosmic_text::Action::Motion(
                                cosmic_text::Motion::BufferEnd,
                            ),
                        );
                    }
                }

                // Editing events
                Action::Edit(edit) => {
                    let topmost_line_before_edit = editor
                        .selection_bounds()
                        .map(|(start, _)| start)
                        .unwrap_or_else(|| editor.cursor())
                        .line;

                    match edit {
                        Edit::Insert(c) => {
                            editor.action(
                                font_system.raw(),
                                cosmic_text::Action::Insert(c),
                            );
                        }
                        Edit::Paste(text) => {
                            editor.insert_string(&text, None);
                        }
                        Edit::Indent => {
                            editor.action(
                                font_system.raw(),
                                cosmic_text::Action::Indent,
                            );
                        }
                        Edit::Unindent => {
                            editor.action(
                                font_system.raw(),
                                cosmic_text::Action::Unindent,
                            );
                        }
                        Edit::Enter => {
                            editor.action(
                                font_system.raw(),
                                cosmic_text::Action::Enter,
                            );
                        }
                        Edit::Backspace => {
                            editor.action(
                                font_system.raw(),
                                cosmic_text::Action::Backspace,
                            );
                        }
                        Edit::Delete => {
                            editor.action(
                                font_system.raw(),
                                cosmic_text::Action::Delete,
                            );
                        }
                    }

                    let cursor = editor.cursor();
                    let selection_start = editor
                        .selection_bounds()
                        .map(|(start, _)| start)
                        .unwrap_or(cursor);

                    internal.topmost_line_changed = Some(
                        selection_start.line.min(topmost_line_before_edit),
                    );
                }

                // Mouse events
                Action::Click(position) => {
                    let x = position.x - inset_at(buffer_from_editor(editor), position.y);
                    editor.action(
                        font_system.raw(),
                        cosmic_text::Action::Click {
                            x: x as i32,
                            y: position.y as i32,
                        },
                    );
                }
                Action::Drag(position) => {
                    let x = position.x - inset_at(buffer_from_editor(editor), position.y);
                    editor.action(
                        font_system.raw(),
                        cosmic_text::Action::Drag {
                            x: x as i32,
                            y: position.y as i32,
                        },
                    );

                    // Deselect if selection matches cursor position
                    if let Some((start, end)) = editor.selection_bounds()
                        && start.line == end.line
                        && start.index == end.index
                    {
                        editor.set_selection(cosmic_text::Selection::None);
                    }
                }
                Action::Scroll { lines } => {
                    editor.action(
                        font_system.raw(),
                        cosmic_text::Action::Scroll {
                            pixels: lines as f32
                                * buffer_from_editor(editor)
                                    .metrics()
                                    .line_height,
                        },
                    );
                }
            }
            wrap_insets(buffer_mut_from_editor(editor), font_system.raw());
        });
    }

    fn move_to(&mut self, cursor: Cursor) {
        self.with_internal_mut(|internal| {
            // TODO: Expose `Affinity`
            internal.editor.set_cursor(cosmic_text::Cursor {
                line: cursor.position.line,
                index: cursor.position.column,
                affinity: cosmic_text::Affinity::Before,
            });

            if let Some(selection) = cursor.selection {
                internal
                    .editor
                    .set_selection(cosmic_text::Selection::Normal(
                        cosmic_text::Cursor {
                            line: selection.line,
                            index: selection.column,
                            affinity: cosmic_text::Affinity::Before,
                        },
                    ));
            } else {
                internal.editor.set_selection(cosmic_text::Selection::None);
            }
            let mut font_system = text::font_system().write().expect("Write font system");
            internal.editor.shape_as_needed(font_system.raw(), true);
            wrap_insets(buffer_mut_from_editor(&mut internal.editor), font_system.raw());
        });
    }

    fn bounds(&self) -> Size {
        self.internal().bounds
    }

    fn min_bounds(&self) -> Size {
        let internal = self.internal();

        let (bounds, _has_rtl) =
            text::measure(buffer_from_editor(&internal.editor));

        bounds
    }

    fn update(
        &mut self,
        new_bounds: Size,
        new_font: Font,
        new_size: Pixels,
        new_line_height: LineHeight,
        new_wrapping: Wrapping,
        new_highlighter: &mut impl Highlighter,
    ) {
        self.with_internal_mut(|internal| {
            let mut font_system =
                text::font_system().write().expect("Write font system");

            let buffer = buffer_mut_from_editor(&mut internal.editor);

            if font_system.version() != internal.version {
                log::trace!("Updating `FontSystem` of `Editor`...");

                for line in buffer.lines.iter_mut() {
                    line.reset();
                }

                internal.version = font_system.version();
                internal.topmost_line_changed = Some(0);
            }

            if new_font != internal.font {
                log::trace!("Updating font of `Editor`...");

                for line in buffer.lines.iter_mut() {
                    let _ = line.set_attrs_list(cosmic_text::AttrsList::new(
                        &text::to_attributes(new_font),
                    ));
                }

                internal.font = new_font;
                internal.topmost_line_changed = Some(0);
            }

            let metrics = buffer.metrics();
            let new_line_height = new_line_height.to_absolute(new_size);

            if new_size.0 != metrics.font_size
                || new_line_height.0 != metrics.line_height
            {
                log::trace!("Updating `Metrics` of `Editor`...");

                buffer.set_metrics(
                    font_system.raw(),
                    cosmic_text::Metrics::new(new_size.0, new_line_height.0),
                );
            }

            let new_wrap = text::to_wrap(new_wrapping);

            if new_wrap != buffer.wrap() {
                log::trace!("Updating `Wrap` strategy of `Editor`...");

                buffer.set_wrap(font_system.raw(), new_wrap);
            }

            if new_bounds != internal.bounds {
                log::trace!("Updating size of `Editor`...");

                buffer.set_size(
                    font_system.raw(),
                    Some(new_bounds.width),
                    Some(new_bounds.height),
                );

                internal.bounds = new_bounds;
            }

            if let Some(topmost_line_changed) =
                internal.topmost_line_changed.take()
            {
                log::trace!(
                    "Notifying highlighter of line \
                    change: {topmost_line_changed}"
                );

                new_highlighter.change_line(topmost_line_changed);
            }

            let _ = internal.selection.write().expect("cursor cache").take();
        internal.editor.shape_as_needed(font_system.raw(), false);
            wrap_insets(buffer_mut_from_editor(&mut internal.editor), font_system.raw());
        });
    }

    fn highlight<H: Highlighter>(
        &mut self,
        font: Self::Font,
        highlighter: &mut H,
        format_highlight: impl Fn(&H::Highlight) -> highlighter::Format<Self::Font>,
    ) {
        let internal = self.internal();
        let buffer = buffer_from_editor(&internal.editor);

        let metrics = buffer.metrics();
        let line_height_ratio = metrics.line_height / metrics.font_size;
        // Span metrics change which lines fit in the viewport. An estimate based
        // on body line height leaves newly exposed headings temporarily unstyled.
        // Format remaining lines once; the highlighter cursor skips them until
        // their text or settings change.
        let last_visible_line = buffer.lines.len().saturating_sub(1);

        let current_line = highlighter.current_line();

        if current_line > last_visible_line {
            return;
        }

        let editor =
            self.0.take().expect("Editor should always be initialized");

        let mut internal = Arc::try_unwrap(editor)
            .expect("Editor cannot have multiple strong references");

        let _ = internal.render_buffer.take();

        let mut font_system =
            text::font_system().write().expect("Write font system");

        let attributes = text::to_attributes(font);

        for line in &mut buffer_mut_from_editor(&mut internal.editor).lines
            [current_line..=last_visible_line]
        {
            let spans: Vec<_> = highlighter.highlight_line(line.text()).map(|(range, highlight)| (range, format_highlight(&highlight))).collect();
            let attrs = |format: highlighter::Format<Font>| cosmic_text::Attrs {
                color_opt: format.color.map(text::to_color),
                metrics_opt: (format.size.is_some() || format.line_height.is_some()).then(|| {
                    let size = format.size.map_or(metrics.font_size, |size| size.0);
                    cosmic_text::Metrics::new(size, format.line_height.map_or(size * line_height_ratio, |height| height.0)).into()
                }),
                ..format.font.map(text::to_attributes).unwrap_or_else(|| attributes.clone())
            };
            // Defaults also style empty lines and insertions at the start of a heading.
            let defaults = spans.first().filter(|(range, _)| range.start == 0).map_or_else(|| attributes.clone(), |(_, format)| attrs(*format));
            let mut list = cosmic_text::AttrsList::new(&defaults);
            let line_inset = spans.iter().filter_map(|(_, format)| format.inset).fold(0.0, |max, inset| inset.0.max(max));
            for (range, format) in spans {
                list.add_span(range, &attrs(format));
            }

            // Metadata carries the inset to layout, hit testing and drawing.
            if line.set_attrs_list(list) || line_inset != inset(line) {
                line.set_metadata(line_inset.to_bits() as usize);
                line.reset_layout();
            }
        }

        let _ = internal.selection.write().expect("cursor cache").take();
        internal.editor.shape_as_needed(font_system.raw(), false);
        wrap_insets(buffer_mut_from_editor(&mut internal.editor), font_system.raw());

        self.0 = Some(Arc::new(internal));
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self(Some(Arc::new(Internal::default())))
    }
}

impl PartialEq for Internal {
    fn eq(&self, other: &Self) -> bool {
        self.font == other.font
            && self.bounds == other.bounds
            && buffer_from_editor(&self.editor).metrics()
                == buffer_from_editor(&other.editor).metrics()
    }
}

impl Default for Internal {
    fn default() -> Self {
        Self {
            render_buffer: sync::OnceLock::new(),
            preedit: None,
            editor: cosmic_text::Editor::new(cosmic_text::Buffer::new_empty(
                cosmic_text::Metrics {
                    font_size: 1.0,
                    line_height: 1.0,
                },
            )),
            selection: RwLock::new(None),
            font: Font::default(),
            bounds: Size::ZERO,
            topmost_line_changed: None,
            version: text::Version::default(),
        }
    }
}

impl fmt::Debug for Internal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Internal")
            .field("font", &self.font)
            .field("bounds", &self.bounds)
            .finish()
    }
}

/// A weak reference to an [`Editor`].
#[derive(Debug, Clone)]
pub struct Weak {
    raw: sync::Weak<Internal>,
    buffer: Arc<cosmic_text::Buffer>,
    /// The bounds of the [`Editor`].
    pub bounds: Size,
}

impl Weak {
    /// Immutable ink for this frame. Relayout may invalidate the live editor
    /// before a cached render group is consumed by the compositor.
    pub fn buffer(&self) -> &cosmic_text::Buffer {
        &self.buffer
    }
    /// Tries to update the reference into an [`Editor`].
    pub fn upgrade(&self) -> Option<Editor> {
        self.raw.upgrade().map(Some).map(Editor)
    }
}

impl PartialEq for Weak {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.buffer, &other.buffer) && self.bounds == other.bounds
    }
}

fn to_motion(motion: Motion) -> cosmic_text::Motion {
    match motion {
        Motion::Left => cosmic_text::Motion::Left,
        Motion::Right => cosmic_text::Motion::Right,
        Motion::Up => cosmic_text::Motion::Up,
        Motion::Down => cosmic_text::Motion::Down,
        Motion::WordLeft => cosmic_text::Motion::LeftWord,
        Motion::WordRight => cosmic_text::Motion::RightWord,
        Motion::Home => cosmic_text::Motion::Home,
        Motion::End => cosmic_text::Motion::End,
        Motion::PageUp => cosmic_text::Motion::PageUp,
        Motion::PageDown => cosmic_text::Motion::PageDown,
        Motion::DocumentStart => cosmic_text::Motion::BufferStart,
        Motion::DocumentEnd => cosmic_text::Motion::BufferEnd,
    }
}

/// The horizontal inset a highlighter gave this line ([`highlighter::Format::inset`]).
/// Layout glyph coordinates exclude it; consumers add it when placing ink and
/// remove it from pointer positions.
pub fn inset(line: &cosmic_text::BufferLine) -> f32 {
    line.metadata().map_or(0.0, |bits| f32::from_bits(bits as u32))
}

fn line_inset(buffer: &cosmic_text::Buffer, line: usize) -> f32 {
    buffer.lines.get(line).map_or(0.0, inset)
}

/// Inset of the visual line at `y`, for hit testing.
fn inset_at(buffer: &cosmic_text::Buffer, y: f32) -> f32 {
    buffer
        .layout_runs()
        .find(|run| y < run.line_top + run.line_height)
        .map_or(0.0, |run| line_inset(buffer, run.line_i))
}

/// Wrap inset lines within the text width less both insets. cosmic-text lays
/// out every line at the buffer width; a line that already fits the narrower
/// width has the same layout either way, so only overflowing lines are redone.
fn wrap_insets(buffer: &mut cosmic_text::Buffer, font_system: &mut cosmic_text::FontSystem) {
    let Some(width) = buffer.size().0 else {
        return;
    };
    let font_size = buffer.metrics().font_size;
    let (wrap, monospace, tab) = (buffer.wrap(), buffer.monospace_width(), buffer.tab_width());
    for line in &mut buffer.lines {
        let amount = inset(line);
        let narrow = (width - 2.0 * amount).max(0.0);
        if amount > 0.0
            && line
                .layout_opt()
                .is_some_and(|layout| layout.iter().any(|visual| visual.w > narrow))
        {
            line.reset_layout();
            let _ = line.layout(font_system, font_size, Some(narrow), wrap, monospace, tab);
        }
    }
}

fn buffer_from_editor<'a, 'b>(
    editor: &'a impl cosmic_text::Edit<'b>,
) -> &'a cosmic_text::Buffer
where
    'b: 'a,
{
    match editor.buffer_ref() {
        cosmic_text::BufferRef::Owned(buffer) => buffer,
        cosmic_text::BufferRef::Borrowed(buffer) => buffer,
        cosmic_text::BufferRef::Arc(buffer) => buffer,
    }
}

fn buffer_mut_from_editor<'a, 'b>(
    editor: &'a mut impl cosmic_text::Edit<'b>,
) -> &'a mut cosmic_text::Buffer
where
    'b: 'a,
{
    match editor.buffer_ref_mut() {
        cosmic_text::BufferRef::Owned(buffer) => buffer,
        cosmic_text::BufferRef::Borrowed(buffer) => buffer,
        cosmic_text::BufferRef::Arc(_buffer) => unreachable!(),
    }
}
