//! Editable text on cosmic-text: per-span typography, layout geometry from
//! the actual runs, and a temporary layout with an IME composition inserted.
use crate::ink;
use crate::styled_text::{Format, Styles};
use cosmic_text::Edit as _;
use iced::advanced::graphics::text::{self, cosmic_text};
use iced::advanced::text::editor::{Action, Cursor, Direction, Edit, Motion, Position, Selection};
use iced::advanced::text::{LineHeight, Wrapping};
use iced::{Font, Pixels, Point, Rectangle, Size};
use std::{ops::Range, sync::Arc};

pub struct Editor {
    editor: cosmic_text::Editor<'static>,
    font: Font,
    bounds: Size,
    version: text::Version,
    styles: Styles,
    /// Lines from here on may carry attributes that do not match `styles`.
    restyle_from: Option<usize>,
    /// The composition inserted by `with_preedit`.
    preedit: Option<(cosmic_text::Cursor, cosmic_text::Cursor)>,
}

impl Editor {
    pub fn with_text(text: &str) -> Self {
        let mut font_system = text::font_system().write().expect("Write font system");
        let mut buffer = cosmic_text::Buffer::new_empty(cosmic_text::Metrics::new(1.0, 1.0));
        buffer.set_text(
            font_system.raw(),
            text,
            &cosmic_text::Attrs::new(),
            cosmic_text::Shaping::Advanced,
            None,
        );
        Self {
            editor: cosmic_text::Editor::new(buffer),
            font: Font::default(),
            bounds: Size::ZERO,
            version: font_system.version(),
            styles: Styles::default(),
            restyle_from: Some(0),
            preedit: None,
        }
    }

    pub fn buffer(&self) -> &cosmic_text::Buffer {
        buffer(&self.editor)
    }

    pub fn is_empty(&self) -> bool {
        let lines = &self.buffer().lines;
        lines.is_empty() || (lines.len() == 1 && lines[0].text().is_empty())
    }

    pub fn line_count(&self) -> usize {
        self.buffer().lines.len()
    }

    pub fn line(&self, index: usize) -> Option<&str> {
        self.buffer().lines.get(index).map(|line| line.text())
    }

    pub fn text(&self) -> String {
        let lines = &self.buffer().lines;
        let mut text = String::new();
        for (index, line) in lines.iter().enumerate() {
            text.push_str(line.text());
            if index + 1 < lines.len() {
                text.push_str(match line.ending() {
                    cosmic_text::LineEnding::None => "\n",
                    ending => ending.as_str(),
                });
            }
        }
        text
    }

    /// The selected text.
    pub fn copy(&self) -> Option<String> {
        self.editor.copy_selection()
    }

    /// The caret or the selected runs, from layout coordinates: spans differ in line height.
    pub fn selection(&self) -> Selection {
        match self.editor.selection_bounds() {
            Some((start, end)) => Selection::Range(highlights(self.buffer(), start, end)),
            None => {
                let (x, y) = self.editor.cursor_position().unwrap_or((-100, -100));
                let inset = line_inset(self.buffer(), self.editor.cursor().line);
                Selection::Caret(Point::new(x as f32 + inset, y as f32))
            }
        }
    }

    /// Height of the run holding the caret.
    pub fn caret_height(&self) -> Option<f32> {
        let (_, y) = self.editor.cursor_position()?;
        self.buffer()
            .layout_runs()
            .find(|run| (run.line_top - y as f32).abs() < 1.1)
            .map(|run| run.line_height)
    }

    /// The caret, including when a selection is present.
    pub fn caret_bounds(&self) -> Option<Rectangle> {
        let (x, y) = self.editor.cursor_position()?;
        let x = x as f32 + line_inset(self.buffer(), self.editor.cursor().line);
        Some(Rectangle::new(
            Point::new(x, y as f32),
            Size::new(1.0, self.caret_height()?),
        ))
    }

    /// Visible bounds of a logical line, including its wrapped runs.
    pub fn line_bounds(&self, line: usize) -> Option<Rectangle> {
        let mut runs = self.buffer().layout_runs().filter(|run| run.line_i == line);
        let first = runs.next()?;
        let top = first.line_top;
        let bottom = runs
            .last()
            .map_or(first.line_top + first.line_height, |run| {
                run.line_top + run.line_height
            });
        Some(Rectangle::new(
            Point::new(0.0, top),
            Size::new(self.bounds.width, bottom - top),
        ))
    }

    /// Visible glyph bounds of a UTF-8 range on one logical line, one per run.
    pub fn span_bounds(&self, line: usize, range: Range<usize>) -> Vec<Rectangle> {
        let inset = line_inset(self.buffer(), line);
        self.buffer()
            .layout_runs()
            .filter(|run| run.line_i == line)
            .filter_map(|run| {
                let (left, right) = run
                    .glyphs
                    .iter()
                    .filter(|g| g.start < range.end && g.end > range.start)
                    .fold((f32::INFINITY, f32::NEG_INFINITY), |(l, r), g| {
                        (l.min(g.x), r.max(g.x + g.w))
                    });
                (right > left).then(|| {
                    Rectangle::new(
                        Point::new(left + inset, run.line_top),
                        Size::new(right - left, run.line_height),
                    )
                })
            })
            .collect()
    }

    /// Baseline of the first run holding glyphs of a UTF-8 range on one logical line.
    pub fn span_baseline(&self, line: usize, range: Range<usize>) -> Option<f32> {
        self.buffer()
            .layout_runs()
            .filter(|run| run.line_i == line)
            .find(|run| {
                run.glyphs
                    .iter()
                    .any(|g| g.start < range.end && g.end > range.start)
            })
            .map(|run| run.line_y)
    }

    /// A temporary layout with `content` composed at the caret. The document
    /// and its history are untouched; cosmic-text gives the composition the
    /// insertion point's typography, so it matches the committed text.
    pub fn with_preedit(&self, content: &str, selection: Option<Range<usize>>) -> Self {
        let mut editor = cosmic_text::Editor::new(self.buffer().clone());
        editor.set_cursor(self.editor.cursor());
        editor.set_selection(self.editor.selection());
        let start = editor
            .selection_bounds()
            .map_or(editor.cursor(), |(start, _)| start);
        let body = ink::attributes(self.font);
        // Empty spacer lines become normal paragraphs while composing.
        if let Some(line) = buffer_mut(&mut editor).lines.get_mut(start.line)
            && line.text().is_empty()
            && line.attrs_list().defaults().family == body.family
            && line
                .attrs_list()
                .defaults()
                .metrics_opt
                .is_some_and(|metrics| {
                    cosmic_text::Metrics::from(metrics).font_size
                        < self.buffer().metrics().font_size
                })
        {
            let _ = line.set_attrs_list(cosmic_text::AttrsList::new(&body));
        }
        editor.insert_string(content, None);
        let end = editor.cursor();
        // Editing resets line metadata; the composed lines keep the line's inset.
        let kept = line_inset(self.buffer(), start.line);
        for line in &mut buffer_mut(&mut editor).lines[start.line..=end.line] {
            line.set_metadata(kept.to_bits() as usize);
        }
        if let Some(range) = selection {
            let position = |offset: usize| {
                let mut offset = offset.min(content.len());
                while !content.is_char_boundary(offset) {
                    offset -= 1;
                }
                let prefix = &content[..offset];
                let lines = prefix.bytes().filter(|b| *b == b'\n').count();
                cosmic_text::Cursor::new(
                    start.line + lines,
                    if lines == 0 {
                        start.index + offset
                    } else {
                        prefix.rsplit('\n').next().unwrap_or_default().len()
                    },
                )
            };
            editor.set_cursor(position(range.end));
            editor.set_selection(if range.is_empty() {
                cosmic_text::Selection::None
            } else {
                cosmic_text::Selection::Normal(position(range.start))
            });
        }
        let mut font_system = text::font_system().write().expect("Write font system");
        editor.shape_as_needed(font_system.raw(), true);
        wrap_insets(buffer_mut(&mut editor), font_system.raw());
        Self {
            editor,
            font: self.font,
            bounds: self.bounds,
            version: font_system.version(),
            styles: self.styles.clone(),
            restyle_from: None,
            preedit: Some((start, end)),
        }
    }

    /// Visible rectangles of the composition inserted by `with_preedit`.
    pub fn preedit_rectangles(&self) -> Vec<Rectangle> {
        self.preedit
            .map(|(start, end)| highlights(self.buffer(), start, end))
            .unwrap_or_default()
    }

    pub fn cursor(&self) -> Cursor {
        let editor = &self.editor;
        let position = |cursor: cosmic_text::Cursor| Position {
            line: cursor.line,
            column: cursor.index,
        };
        // Word and line selections store their click anchor, not their
        // expanded bounds. Expose the range that is painted and copied.
        if matches!(
            editor.selection(),
            cosmic_text::Selection::Word(_) | cosmic_text::Selection::Line(_)
        ) && let Some((start, end)) = editor.selection_bounds()
        {
            return Cursor {
                position: position(end),
                selection: Some(position(start)),
            };
        }
        Cursor {
            position: position(editor.cursor()),
            selection: match editor.selection() {
                cosmic_text::Selection::None => None,
                cosmic_text::Selection::Normal(cursor)
                | cosmic_text::Selection::Line(cursor)
                | cosmic_text::Selection::Word(cursor) => Some(position(cursor)),
            },
        }
    }

    pub fn perform(&mut self, action: Action) {
        let mut font_system = text::font_system().write().expect("Write font system");
        let font_system = font_system.raw();
        let editor = &mut self.editor;
        let mut changed = None;
        match action {
            Action::Move(motion) => {
                if let Some((start, end)) = editor.selection_bounds() {
                    editor.set_selection(cosmic_text::Selection::None);
                    match motion {
                        // Performed as-is even when a selection is present.
                        Motion::Home
                        | Motion::End
                        | Motion::DocumentStart
                        | Motion::DocumentEnd => {
                            editor.action(
                                font_system,
                                cosmic_text::Action::Motion(to_motion(motion)),
                            );
                        }
                        // Others move the cursor to one end of the selection.
                        _ => editor.set_cursor(match motion.direction() {
                            Direction::Left => start,
                            Direction::Right => end,
                        }),
                    }
                } else {
                    editor.action(font_system, cosmic_text::Action::Motion(to_motion(motion)));
                }
            }
            Action::Select(motion) => {
                let cursor = editor.cursor();
                if editor.selection_bounds().is_none() {
                    editor.set_selection(cosmic_text::Selection::Normal(cursor));
                }
                editor.action(font_system, cosmic_text::Action::Motion(to_motion(motion)));
                deselect_if_empty(editor);
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
                let lines = &buffer(editor).lines;
                if lines.len() > 1 || lines.first().is_some_and(|line| !line.text().is_empty()) {
                    let cursor = editor.cursor();
                    editor.set_selection(cosmic_text::Selection::Normal(cosmic_text::Cursor {
                        line: 0,
                        index: 0,
                        ..cursor
                    }));
                    editor.action(
                        font_system,
                        cosmic_text::Action::Motion(cosmic_text::Motion::BufferEnd),
                    );
                }
            }
            Action::Edit(edit) => {
                let topmost = |editor: &cosmic_text::Editor<'static>| {
                    editor
                        .selection_bounds()
                        .map_or(editor.cursor(), |(start, _)| start)
                        .line
                };
                let before = topmost(editor);
                match edit {
                    Edit::Insert(c) => editor.action(font_system, cosmic_text::Action::Insert(c)),
                    Edit::Paste(text) => editor.insert_string(&text, None),
                    Edit::Indent => editor.action(font_system, cosmic_text::Action::Indent),
                    Edit::Unindent => editor.action(font_system, cosmic_text::Action::Unindent),
                    Edit::Enter => editor.action(font_system, cosmic_text::Action::Enter),
                    Edit::Backspace => editor.action(font_system, cosmic_text::Action::Backspace),
                    Edit::Delete => editor.action(font_system, cosmic_text::Action::Delete),
                }
                changed = Some(topmost(editor).min(before));
            }
            Action::Click(position) => {
                let x = position.x - inset_at(buffer(editor), position.y);
                editor.action(
                    font_system,
                    cosmic_text::Action::Click {
                        x: x as i32,
                        y: position.y as i32,
                    },
                );
            }
            Action::Drag(position) => {
                let x = position.x - inset_at(buffer(editor), position.y);
                editor.action(
                    font_system,
                    cosmic_text::Action::Drag {
                        x: x as i32,
                        y: position.y as i32,
                    },
                );
                deselect_if_empty(editor);
            }
            Action::Scroll { lines } => {
                let pixels = lines as f32 * buffer(editor).metrics().line_height;
                editor.action(font_system, cosmic_text::Action::Scroll { pixels });
            }
        }
        wrap_insets(buffer_mut(editor), font_system);
        if let Some(line) = changed {
            self.restyle(line);
        }
    }

    /// Put the cursor at `cursor`, clearing an absent selection, and reveal it.
    pub fn move_to(&mut self, cursor: Cursor) {
        let to_cursor = |position: Position| cosmic_text::Cursor {
            line: position.line,
            index: position.column,
            affinity: cosmic_text::Affinity::Before,
        };
        self.editor.set_cursor(to_cursor(cursor.position));
        self.editor.set_selection(
            cursor
                .selection
                .map_or(cosmic_text::Selection::None, |selection| {
                    cosmic_text::Selection::Normal(to_cursor(selection))
                }),
        );
        let mut font_system = text::font_system().write().expect("Write font system");
        self.editor.shape_as_needed(font_system.raw(), true);
        wrap_insets(buffer_mut(&mut self.editor), font_system.raw());
    }

    pub fn bounds(&self) -> Size {
        self.bounds
    }

    /// Lay out for `bounds`. Every line from the first changed one is styled,
    /// not only those in view: span metrics decide which lines are in view.
    pub fn update(
        &mut self,
        bounds: Size,
        font: Font,
        size: Pixels,
        line_height: LineHeight,
        wrapping: Wrapping,
        styles: &Styles,
    ) {
        let mut font_system = text::font_system().write().expect("Write font system");
        if font_system.version() != self.version {
            for line in &mut buffer_mut(&mut self.editor).lines {
                line.reset();
            }
            self.version = font_system.version();
            self.restyle(0);
        }
        if font != self.font {
            self.font = font;
            self.restyle(0);
        }
        if !Arc::ptr_eq(&styles.0, &self.styles.0) {
            let unchanged = self
                .styles
                .0
                .iter()
                .zip(styles.0.iter())
                .take_while(|(old, new)| old == new)
                .count();
            self.styles = styles.clone();
            self.restyle(unchanged);
        }
        let metrics = cosmic_text::Metrics::new(size.0, line_height.to_absolute(size).0);
        if metrics != self.buffer().metrics() {
            // Span line heights default to the editor's ratio.
            self.restyle(0);
        }
        if let Some(from) = self.restyle_from.take() {
            self.apply_styles(from, metrics);
        }
        let font_system = font_system.raw();
        let buffer = buffer_mut(&mut self.editor);
        if metrics != buffer.metrics() {
            buffer.set_metrics(font_system, metrics);
        }
        let wrap = text::to_wrap(wrapping);
        if wrap != buffer.wrap() {
            buffer.set_wrap(font_system, wrap);
        }
        if bounds != self.bounds {
            buffer.set_size(font_system, Some(bounds.width), Some(bounds.height));
            self.bounds = bounds;
        }
        self.editor.shape_as_needed(font_system, false);
        wrap_insets(buffer_mut(&mut self.editor), font_system);
    }

    /// The laid-out lines in view, detached for one frame of drawing. Lines
    /// sharing an inset form one buffer, drawn shifted right by that inset.
    pub fn snapshot(&self) -> Vec<(f32, cosmic_text::Buffer)> {
        let buffer = self.buffer();
        let metrics = buffer.metrics();
        let scroll = buffer.scroll();
        let end = buffer
            .layout_runs()
            .last()
            .map_or(scroll.line, |run| run.line_i)
            + 1;
        let lines = buffer
            .lines
            .get(scroll.line..end.min(buffer.lines.len()))
            .unwrap_or_default();
        let mut font_system = text::font_system().write().expect("Write font system");
        let mut snapshots = vec![];
        let mut height = 0.0;
        for group in lines.chunk_by(|a, b| inset(a) == inset(b)) {
            let mut snapshot = cosmic_text::Buffer::new_empty(metrics);
            snapshot.set_size(
                font_system.raw(),
                Some(ink::UNBOUNDED),
                Some(ink::UNBOUNDED),
            );
            snapshot.lines = group.to_vec();
            // Later groups start below the earlier ones, as in the full layout.
            snapshot.set_scroll(cosmic_text::Scroll {
                line: 0,
                vertical: scroll.vertical - height,
                ..scroll
            });
            height += group
                .iter()
                .filter_map(|line| line.layout_opt())
                .flatten()
                .map(|layout| layout.line_height_opt.unwrap_or(metrics.line_height))
                .sum::<f32>();
            snapshots.push((inset(&group[0]), snapshot));
        }
        snapshots
    }

    fn restyle(&mut self, line: usize) {
        self.restyle_from = Some(self.restyle_from.map_or(line, |from| from.min(line)));
    }

    fn apply_styles(&mut self, from: usize, metrics: cosmic_text::Metrics) {
        let body = ink::attributes(self.font);
        let ratio = metrics.line_height / metrics.font_size;
        let attrs = |format: &Format| cosmic_text::Attrs {
            color_opt: format.color.map(text::to_color),
            metrics_opt: (format.size.is_some() || format.line_height.is_some()).then(|| {
                let size = format.size.map_or(metrics.font_size, |size| size.0);
                let height = format.line_height.map_or(size * ratio, |height| height.0);
                cosmic_text::Metrics::new(size, height).into()
            }),
            ..format
                .font
                .map(ink::attributes)
                .unwrap_or_else(|| body.clone())
        };
        let styles = self.styles.0.clone();
        let lines = &mut buffer_mut(&mut self.editor).lines;
        for (index, line) in lines.iter_mut().enumerate().skip(from) {
            let spans = styles.get(index).map_or(&[][..], Vec::as_slice);
            // Defaults also style empty lines and insertions at a heading's start.
            let defaults = spans
                .first()
                .filter(|(range, _)| range.start == 0)
                .map_or_else(|| body.clone(), |(_, format)| attrs(format));
            let mut list = cosmic_text::AttrsList::new(&defaults);
            for (range, format) in spans {
                list.add_span(range.clone(), &attrs(format));
            }
            let line_inset = spans
                .iter()
                .filter_map(|(_, format)| format.inset)
                .fold(0.0, |max, inset| inset.0.max(max));
            // Unchanged lines keep their shaping. Metadata carries the inset
            // to layout, hit testing and drawing; changed attributes clear it.
            if line.set_attrs_list(list) || line_inset != inset(line) {
                line.set_metadata(line_inset.to_bits() as usize);
                line.reset_layout();
            }
        }
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::with_text("")
    }
}

/// Rectangles covering `start..end`, one per laid-out run.
fn highlights(
    buffer: &cosmic_text::Buffer,
    start: cosmic_text::Cursor,
    end: cosmic_text::Cursor,
) -> Vec<Rectangle> {
    buffer
        .layout_runs()
        .filter_map(|run| {
            let (x, width) = run.highlight(start, end)?;
            (width > 0.0).then_some(Rectangle {
                x: x + line_inset(buffer, run.line_i),
                y: run.line_top,
                width,
                height: run.line_height,
            })
        })
        .collect()
}

/// The inset a span gave this line ([`Format::inset`]). Layout coordinates
/// exclude it: geometry adds it, pointer positions drop it, drawing shifts by it.
fn inset(line: &cosmic_text::BufferLine) -> f32 {
    line.metadata()
        .map_or(0.0, |bits| f32::from_bits(bits as u32))
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
        let narrow = (width - 2.0 * inset(line)).max(0.0);
        if inset(line) > 0.0
            && line
                .layout_opt()
                .is_some_and(|layout| layout.iter().any(|visual| visual.w > narrow))
        {
            line.reset_layout();
            let _ = line.layout(font_system, font_size, Some(narrow), wrap, monospace, tab);
        }
    }
}

fn deselect_if_empty(editor: &mut cosmic_text::Editor<'static>) {
    if let Some((start, end)) = editor.selection_bounds()
        && start.line == end.line
        && start.index == end.index
    {
        editor.set_selection(cosmic_text::Selection::None);
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

fn buffer<'a>(editor: &'a cosmic_text::Editor<'static>) -> &'a cosmic_text::Buffer {
    match editor.buffer_ref() {
        cosmic_text::BufferRef::Owned(buffer) => buffer,
        cosmic_text::BufferRef::Borrowed(buffer) => buffer,
        cosmic_text::BufferRef::Arc(buffer) => buffer,
    }
}

fn buffer_mut<'a>(editor: &'a mut cosmic_text::Editor<'static>) -> &'a mut cosmic_text::Buffer {
    match editor.buffer_ref_mut() {
        cosmic_text::BufferRef::Owned(buffer) => buffer,
        cosmic_text::BufferRef::Borrowed(buffer) => buffer,
        cosmic_text::BufferRef::Arc(buffer) => Arc::make_mut(buffer),
    }
}
