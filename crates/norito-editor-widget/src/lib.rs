//! Continuous WYSIWYG editing, projected onto the loss-preserving Markdown model.
mod syntax;
mod tables;
use iced::{
    Element, Font, Length, Theme,
    advanced::text::{
        editor::{Cursor, Position},
        highlighter::Format,
    },
    widget::text_editor,
};
use norito_iced_ext::styled_text::{Span, StyledText, Styles};
use norito_model::{
    Document,
    projection::{Projection, changed_range},
};
use std::{ops::Range, sync::Arc};
pub use tables::TableAction;

#[derive(Debug, Clone)]
pub enum EditorEvent {
    Action(text_editor::Action),
    Format(Formatting),
    Undo,
    Redo,
    ContextMenu,
    FollowLink,
    EditCode(usize),
    InsertTable,
    EditTable(usize),
    Table(TableAction),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Rich,
    Source,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClickTarget {
    Link { source: usize, destination: String },
    Task { source: usize },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Formatting {
    Bold,
    Italic,
    Code,
    Heading(u8),
    Bullet,
    Quote,
    Strike,
}

pub struct MarkdownEditor {
    syntax_cache: syntax::Cache,
    style_cache: std::cell::RefCell<Option<StyleCache>>,
    structure_cache: std::cell::RefCell<Option<(String, Arc<norito_model::structure::Structure>)>>,
    content: text_editor::Content,
    projection: Projection,
    revision: u64,
    mode: Mode,
    pending_selection: Option<Range<usize>>,
    active: Option<usize>,
    auto_closers: Vec<usize>,
    code_colors: Vec<Option<iced::Color>>,
    table_edit: Option<tables::TableEdit>,
    table_picker: Option<(Range<usize>, usize, usize)>,
}
struct StyleCache {
    source: String,
    display: String,
    mode: Mode,
    palette: [iced::Color; 3],
    styles: Styles,
}
impl MarkdownEditor {
    /// Resolve interactive glyphs before changing the cursor or revealing syntax.
    /// Hit testing the glyph bounds avoids activating links from trailing whitespace.
    pub fn click_target(&self, point: iced::Point) -> Option<ClickTarget> {
        if self.mode != Mode::Rich {
            return None;
        }
        for cell in &self.projection.cells {
            let checkbox = matches!(&self.projection.text[cell.display.clone()], "☐" | "☑")
                && matches!(
                    &self.projection.source[cell.source.clone()],
                    "[ ]" | "[x]" | "[X]"
                );
            if !cell.style.link && !checkbox {
                continue;
            }
            let pos = position(&self.projection.text, cell.display.start);
            if !self
                .content
                .span_bounds(pos.line, pos.column..pos.column + cell.display.len())
                .iter()
                .any(|rect| rect.contains(point))
            {
                continue;
            }
            if checkbox {
                return Some(ClickTarget::Task {
                    source: cell.source.start,
                });
            }
            if let Some(link) = self
                .structure()
                .links
                .iter()
                .find(|link| !link.image && link.range.contains(&cell.source.start))
            {
                return Some(ClickTarget::Link {
                    source: cell.source.start,
                    destination: link.destination.clone(),
                });
            }
        }
        None
    }
    pub fn toggle_task_at(&mut self, at: usize) -> Option<String> {
        let marker = self.projection.source.get(at..at + 3)?;
        let replacement = match marker {
            "[ ]" => "[x]",
            "[x]" | "[X]" => "[ ]",
            _ => return None,
        };
        let mut source = self.projection.source.clone();
        source.replace_range(at..at + 3, replacement);
        // Keep the current caret and rendered checkbox when toggling with the mouse.
        self.pending_selection = Some(self.source_selection());
        Some(source)
    }
    pub fn source_selection(&self) -> Range<usize> {
        let selected = self.selection();
        if self.mode == Mode::Source {
            selected
        } else {
            self.projection.source_position(selected.start)
                ..self.projection.source_position(selected.end)
        }
    }
    pub fn selection_anchor(&self) -> Option<iced::Point> {
        let selected = self.selection();
        if selected.is_empty() {
            return None;
        }
        let text = self.content.text();
        let mut offset = 0;
        for (line, value) in text.split_inclusive('\n').enumerate() {
            let start = selected.start.saturating_sub(offset);
            let end = selected
                .end
                .saturating_sub(offset)
                .min(value.trim_end_matches(['\r', '\n']).len());
            if start < end
                && let Some(rect) = self.content.span_bounds(line, start..end).first()
            {
                return Some(
                    rect.position()
                        + iced::Vector::new(
                            norito_iced_ext::EDITOR_PADDING.left,
                            norito_iced_ext::EDITOR_PADDING.top,
                        ),
                );
            }
            offset += value.len();
        }
        None
    }
    pub fn jump(&mut self, source: usize) {
        self.table_edit = None;
        self.table_picker = None;
        let at = if self.mode == Mode::Rich {
            self.projection.display_position(source)
        } else {
            source.min(self.projection.source.len())
        };
        self.content.move_to(Cursor {
            position: position(self.displayed(), at),
            selection: None,
        });
    }
    pub fn insert_markdown(
        &mut self,
        mut range: Range<usize>,
        markdown: &str,
        block: bool,
    ) -> String {
        let mut source = self.projection.source.clone();
        if self.mode == Mode::Rich
            && range == self.source_selection()
            && !self.selection().is_empty()
        {
            let (cleaned, caret) = self.projection.replace(self.selection(), "");
            source = cleaned;
            range = caret..caret;
        }
        if block && range.is_empty() {
            let line_end = source[range.start..]
                .find('\n')
                .map_or(source.len(), |offset| range.start + offset);
            range = line_end..line_end;
        }
        let mut insertion = String::new();
        if block && range.start > 0 && !source[..range.start].ends_with("\n\n") {
            insertion.push_str(if source[..range.start].ends_with('\n') {
                "\n"
            } else {
                "\n\n"
            });
        }
        insertion.push_str(markdown);
        let end = range.start + insertion.len();
        if block && range.end < source.len() && !source[range.end..].starts_with('\n') {
            insertion.push('\n');
        }
        source.replace_range(range, &insertion);
        self.pending_selection = Some(end..end);
        source
    }
    pub fn insert_code(&mut self, language: &str) -> String {
        self.table_edit = None;
        self.table_picker = None;
        let selected = Projection::new(&self.selected_text()).text;
        let markdown = norito_model::structure::fenced_code(language, &selected);
        let source = self.insert_markdown(self.source_selection(), &markdown, true);
        let start = self.pending_selection.as_ref().unwrap().end - markdown.len();
        let caret = start + markdown.find('\n').unwrap() + 1;
        self.pending_selection = Some(caret..caret);
        source
    }
    pub fn selected_text(&self) -> String {
        self.content.text()[self.selection()].to_owned()
    }
    pub fn character_count(&self) -> usize {
        self.projection.source.chars().count()
    }
    pub fn new(document: &Document) -> Self {
        let projection = Projection::editable(&document.source, None);
        let mut syntax_cache = syntax::Cache::default();
        let code_colors = syntax_cache.colors(&document.source);
        Self {
            syntax_cache,
            style_cache: Default::default(),
            structure_cache: Default::default(),
            table_edit: None,
            table_picker: None,
            content: text_editor::Content::with_text(&projection.text),
            projection,
            revision: document.revision,
            mode: Mode::Rich,
            pending_selection: None,
            active: None,
            auto_closers: vec![],
            code_colors,
        }
    }
    pub fn displayed(&self) -> &str {
        if self.mode == Mode::Rich {
            &self.projection.text
        } else {
            &self.projection.source
        }
    }
    fn selection(&self) -> Range<usize> {
        let cursor = self.content.cursor();
        let text = self.content.text();
        let end = offset(&text, cursor.position);
        let start = cursor.selection.map_or(end, |p| offset(&text, p));
        start.min(end)..start.max(end)
    }
    pub fn set_mode(&mut self, mode: Mode) {
        if self.mode == mode {
            return;
        }
        self.table_edit = None;
        self.table_picker = None;
        let selection = self.selection();
        let source = if self.mode == Mode::Rich {
            self.projection.source_position(selection.start)
                ..self.projection.source_position(selection.end)
        } else {
            selection
        };
        self.mode = mode;
        self.reset_input(source);
    }
    fn reset_input(&mut self, source: Range<usize>) {
        let (start, end) = if self.mode == Mode::Rich {
            (
                self.projection.display_position(source.start),
                self.projection.display_position(source.end),
            )
        } else {
            (
                source.start.min(self.projection.source.len()),
                source.end.min(self.projection.source.len()),
            )
        };
        // Preserve the editor, shaped lines and scroll position during typing.
        // Most keystrokes already match the new projection exactly.
        let current = self.content.text();
        if current != self.displayed() {
            let (removed, added) = changed_range(&current, self.displayed());
            self.content.move_to(Cursor {
                position: position(&current, removed.end),
                selection: Some(position(&current, removed.start)),
            });
            let inserted = self.displayed()[added].to_owned();
            self.content
                .perform(text_editor::Action::Edit(text_editor::Edit::Paste(
                    Arc::new(inserted),
                )));
        }
        let cursor = Cursor {
            position: position(self.displayed(), end),
            selection: (start != end).then(|| position(self.displayed(), start)),
        };
        if self.content.cursor() != cursor {
            self.content.move_to(cursor);
        }
    }
    pub fn perform(&mut self, action: text_editor::Action) -> Option<String> {
        if action.is_edit() || matches!(action, text_editor::Action::Click(_)) {
            self.table_edit = None;
            self.table_picker = None;
        }
        if let Some(result) = self.auto_pair(&action) {
            return result;
        }
        let edit = action.is_edit();
        let selected = self.selection();
        let old_cursor = offset(&self.content.text(), self.content.cursor().position);
        let operation = action.clone();
        self.content.perform(action);
        if !edit {
            if self.mode == Mode::Rich && !matches!(operation, text_editor::Action::Scroll { .. }) {
                let source = self.source_selection();
                self.activate(source);
            }
            return None;
        }
        let new = self.content.text();
        if self.mode == Mode::Source {
            return Some(new);
        }
        if new == self.projection.text
            && matches!(
                operation,
                text_editor::Action::Edit(text_editor::Edit::Backspace)
            )
            && selected.is_empty()
            && self.content.cursor().position.column == 0
        {
            return self.format(Formatting::Heading(0));
        }
        let old = &self.projection.text;
        let (removed, added) = match operation {
            text_editor::Action::Edit(
                text_editor::Edit::Insert(_)
                | text_editor::Edit::Paste(_)
                | text_editor::Edit::Enter,
            ) => {
                let inserted = new.len().saturating_sub(old.len() - selected.len());
                (selected.clone(), selected.start..selected.start + inserted)
            }
            text_editor::Action::Edit(text_editor::Edit::Backspace | text_editor::Edit::Delete) => {
                let range = if !selected.is_empty() {
                    selected
                } else if matches!(
                    operation,
                    text_editor::Action::Edit(text_editor::Edit::Backspace)
                ) {
                    offset(&new, self.content.cursor().position)..old_cursor
                } else {
                    old_cursor..old_cursor + old.len().saturating_sub(new.len())
                };
                let start = range.start;
                (range, start..start)
            }
            _ => changed_range(old, &new),
        };
        if removed.is_empty() && added.is_empty() {
            return None;
        }
        let source_start = self.projection.source_position(removed.start);
        let source_end = self.projection.source_position(removed.end);
        let multiline = new[added.clone()].contains('\n');
        let (source, caret) = self.projection.replace(removed, &new[added]);
        if multiline {
            self.auto_closers.clear();
        } else {
            let delta = source.len() as isize - self.projection.source.len() as isize;
            self.auto_closers
                .retain(|p| *p < source_start || *p >= source_end);
            for p in &mut self.auto_closers {
                if *p >= source_end {
                    *p = p.saturating_add_signed(delta);
                }
            }
        }
        self.pending_selection = Some(caret..caret);
        Some(source)
    }
    pub fn activate(&mut self, source: Range<usize>) {
        self.active = Some(source.end);
        let projection = Projection::editable(&self.projection.source, self.active);
        let changed = projection.text != self.projection.text;
        self.projection = projection;
        if changed {
            self.reset_input(source);
        }
    }
    pub fn edit_code(&mut self, source: usize) {
        self.table_edit = None;
        self.table_picker = None;
        self.activate(source..source);
        self.reset_input(source..source);
    }
    pub fn finish_editing(&mut self) {
        self.table_edit = None;
        self.table_picker = None;
        let selection = self.source_selection();
        self.active = None;
        self.projection = Projection::editable(&self.projection.source, None);
        self.reset_input(selection);
    }
    pub fn toggle_task(&mut self, marker_only: bool) -> Option<String> {
        let at = self.source_selection().start;
        let source = &self.projection.source;
        let start = source[..at].rfind('\n').map_or(0, |i| i + 1);
        let line = source[start..].split('\n').next()?;
        let prefix = line
            .find("[ ]")
            .or_else(|| line.find("[x]"))
            .or_else(|| line.find("[X]"))?;
        if !line[..prefix].trim_start().starts_with(['-', '*', '+']) {
            return None;
        }
        if marker_only && !(start + prefix..=start + prefix + 3).contains(&at) {
            return None;
        }
        let mut result = source.clone();
        result.replace_range(
            start + prefix + 1..start + prefix + 2,
            if line.as_bytes()[prefix + 1] == b' ' {
                "x"
            } else {
                " "
            },
        );
        self.pending_selection = Some(at..at);
        Some(result)
    }
    pub fn select_source(&mut self, range: Range<usize>) {
        self.active = Some(range.end);
        self.projection = Projection::editable(&self.projection.source, self.active);
        self.reset_input(range);
    }
    fn auto_pair(&mut self, action: &text_editor::Action) -> Option<Option<String>> {
        if self.mode != Mode::Rich {
            return None;
        }
        let selection = self.source_selection();
        let at = selection.start;
        let source = self.projection.source.clone();
        if matches!(
            action,
            text_editor::Action::Edit(text_editor::Edit::Backspace)
        ) && selection.is_empty()
            && self.auto_closers.contains(&at)
        {
            let opening = source[..at].chars().next_back()?;
            let closing = source[at..].chars().next()?;
            if matches!(
                (opening, closing),
                ('*', '*') | ('_', '_') | ('~', '~') | ('`', '`') | ('[', ']') | ('(', ')')
            ) {
                let start = at - opening.len_utf8();
                let end = at + closing.len_utf8();
                let mut result = source;
                result.replace_range(start..end, "");
                self.auto_closers.retain(|p| *p != at);
                for p in &mut self.auto_closers {
                    if *p >= end {
                        *p -= end - start;
                    }
                }
                self.pending_selection = Some(start..start);
                return Some(Some(result));
            }
        }
        let text_editor::Action::Edit(text_editor::Edit::Insert(ch)) = action else {
            return None;
        };
        if self
            .projection
            .cells
            .iter()
            .any(|c| c.source.start <= at && at < c.source.end && c.style.block_code)
        {
            return None;
        }
        let paired = match *ch {
            '*' | '_' | '`' | '~' => *ch,
            '[' => ']',
            '(' => ')',
            _ => '\0',
        };
        if selection.is_empty() && source[at..].starts_with(*ch) && self.auto_closers.contains(&at)
        {
            let prev = source[..at].chars().next_back();
            // A second opening star creates **|**, not *|* followed by a close.
            if matches!(ch, '*' | '_' | '~')
                && prev == Some(*ch)
                && source[..at]
                    .rsplit(char::is_whitespace)
                    .next()
                    .is_some_and(|run| run.chars().all(|c| c == *ch))
                && source[..at].chars().rev().take_while(|c| c == ch).count() < 2
            {
                let mut result = source;
                result.insert_str(at, &format!("{ch}{ch}"));
                self.auto_closers
                    .iter_mut()
                    .filter(|p| **p >= at)
                    .for_each(|p| *p += 2);
                self.auto_closers.push(at + 1);
                self.pending_selection = Some(at + 1..at + 1);
                return Some(Some(result));
            }
            self.auto_closers.retain(|p| *p != at);
            self.activate(at + ch.len_utf8()..at + ch.len_utf8());
            self.reset_input(at + ch.len_utf8()..at + ch.len_utf8());
            return Some(None);
        }
        let fence_start = *ch == '`'
            && source[..at]
                .rsplit('\n')
                .next()
                .is_some_and(|line| line.chars().all(|c| c == '`' || c.is_whitespace()));
        let closing_inline_code = *ch == '`'
            && source[..at]
                .rsplit('\n')
                .next()
                .is_some_and(|line| line.chars().filter(|c| *c == '`').count() % 2 == 1);
        if paired != '\0'
            && !fence_start
            && !closing_inline_code
            && (source[at..].starts_with(char::is_whitespace)
                || at == source.len()
                || !selection.is_empty())
        {
            let mut result = source;
            result.insert(selection.end, paired);
            result.insert(at, *ch);
            self.auto_closers.push(selection.end + ch.len_utf8());
            self.pending_selection = Some(at + ch.len_utf8()..selection.end + ch.len_utf8());
            return Some(Some(result));
        }
        // Keep generated closing markers aligned while text is inserted.
        if selection.is_empty() && !self.auto_closers.is_empty() {
            let mut result = source;
            result.insert(at, *ch);
            for end in &mut self.auto_closers {
                if *end >= at {
                    *end += ch.len_utf8();
                }
            }
            self.pending_selection = Some(at + ch.len_utf8()..at + ch.len_utf8());
            return Some(Some(result));
        }
        None
    }
    pub fn format(&mut self, formatting: Formatting) -> Option<String> {
        let selected = self.selection();
        let marker = match formatting {
            Formatting::Bold => Some("**"),
            Formatting::Italic => Some("*"),
            Formatting::Code => Some("`"),
            Formatting::Strike => Some("~~"),
            _ => None,
        };
        if selected.is_empty()
            && let Some(marker) = marker
        {
            let at = self.source_selection().start;
            if self
                .projection
                .cells
                .iter()
                .any(|c| c.source.contains(&at) && c.style.block_code)
            {
                return None;
            }
            let mut source = self.projection.source.clone();
            source.insert_str(at, &marker.repeat(2));
            let caret = at + marker.len();
            self.auto_closers.extend(caret..caret + marker.len());
            self.pending_selection = Some(caret..caret);
            return Some(source);
        }
        let raw;
        let projection = if self.mode == Mode::Source {
            raw = Projection::raw(&self.projection.source);
            &raw
        } else {
            &self.projection
        };
        let result = match formatting {
            Formatting::Bold => projection.format(selected, "**"),
            Formatting::Italic => projection.format(selected, "*"),
            Formatting::Code => projection.format(selected, "`"),
            Formatting::Heading(level) => self.block_prefix(&"#".repeat(level as usize)),
            Formatting::Bullet => self.block_prefix("-"),
            Formatting::Quote => self.quote(),
            Formatting::Strike => projection.format(selected, "~~"),
        }?;
        if result.0 == self.projection.source {
            return None;
        }
        self.pending_selection = Some(result.1);
        Some(result.0)
    }
    fn block_prefix(&self, prefix: &str) -> Option<(String, Range<usize>)> {
        let caret = self.source_selection().start;
        let start = self.projection.source[..caret]
            .rfind('\n')
            .map_or(0, |p| p + 1);
        let end = self.projection.source[caret..]
            .find('\n')
            .map_or(self.projection.source.len(), |p| caret + p);
        let raw = &self.projection.source[start..end];
        let hashes = raw.bytes().take_while(|b| *b == b'#').count();
        let prefix_len = if (1..=6).contains(&hashes) && raw.as_bytes().get(hashes) == Some(&b' ') {
            hashes + 1
        } else if raw.starts_with("> ")
            || raw.starts_with("- ")
            || raw.starts_with("* ")
            || raw.starts_with("+ ")
        {
            2
        } else {
            0
        };
        let visible_start = start + prefix_len;
        if self
            .projection
            .cells
            .iter()
            .any(|c| c.source.start >= start && c.source.start < end && c.style.block_code)
        {
            return None;
        }
        let old_prefix = &self.projection.source[start..visible_start];
        let new_prefix = if old_prefix.trim() == prefix || prefix.is_empty() {
            String::new()
        } else {
            format!("{prefix} ")
        };
        let mut source = self.projection.source.clone();
        let remove_end = if raw.starts_with("- ") || raw.starts_with("* ") || raw.starts_with("+ ")
        {
            start + 2
        } else {
            visible_start
        };
        source.replace_range(start..remove_end, &new_prefix);
        let cursor = start + new_prefix.len();
        Some((source, cursor..cursor))
    }
    fn quote(&self) -> Option<(String, Range<usize>)> {
        let selection = self.source_selection();
        let source = &self.projection.source;
        let start = source[..selection.start].rfind('\n').map_or(0, |i| i + 1);
        let end = source[selection.end..]
            .find('\n')
            .map_or(source.len(), |i| selection.end + i);
        if self
            .projection
            .cells
            .iter()
            .any(|c| c.source.start >= start && c.source.start < end && c.style.block_code)
        {
            return None;
        }
        let lines: Vec<_> = source[start..end].split('\n').collect();
        let remove = lines.iter().all(|line| line.starts_with('>'));
        let replacement = lines
            .iter()
            .map(|line| {
                if remove {
                    line[if line.starts_with("> ") { 2 } else { 1 }..].to_owned()
                } else {
                    format!("> {line}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let mut result = source.clone();
        result.replace_range(start..end, &replacement);
        Some((result, start..start + replacement.len()))
    }
    fn diagram_slots(&self) -> Vec<(usize, usize, String)> {
        self.structure()
            .code
            .iter()
            .filter(|code| code.language.split_whitespace().next() == Some("mermaid"))
            .filter_map(|code| {
                let cell = self
                    .projection
                    .cells
                    .iter()
                    .find(|c| c.source.start == code.range.start && c.style.diagram)?;
                let display = cell.display.start;
                let line = self.projection.text[..display]
                    .bytes()
                    .filter(|b| *b == b'\n')
                    .count();
                Some((line, code.range.start, code.content.clone()))
            })
            .collect()
    }
    pub fn synchronize(&mut self, document: &Document, replace_input: bool) {
        if !replace_input && self.revision == document.revision {
            return;
        }
        self.validate_table(&document.source);
        let previous = offset(&self.content.text(), self.content.cursor().position);
        let previous = if self.mode == Mode::Rich {
            self.projection.source_position(previous)
        } else {
            previous
        };
        let pending = self.pending_selection.clone();
        if let Some(selection) = &pending {
            self.active = Some(selection.end);
        } else if replace_input {
            self.active = None;
            self.auto_closers.clear();
        }
        self.projection = Projection::editable(&document.source, self.active);
        self.revision = document.revision;
        self.code_colors = self.syntax_cache.colors(&document.source);
        if let Some(selection) = self.pending_selection.take() {
            self.reset_input(selection);
        } else if replace_input {
            self.reset_input(
                previous.min(document.source.len())..previous.min(document.source.len()),
            );
        }
    }
    fn styles(&self, theme: &Theme) -> Styles {
        let palette = [
            theme.palette().background,
            theme.palette().text,
            theme.palette().primary,
        ];
        if let Some(cache) = self.style_cache.borrow().as_ref()
            && cache.source == self.projection.source
            && cache.display == self.projection.text
            && cache.mode == self.mode
            && cache.palette == palette
        {
            return cache.styles.clone();
        }
        let styles = self.compute_styles(theme);
        *self.style_cache.borrow_mut() = Some(StyleCache {
            source: self.projection.source.clone(),
            display: self.projection.text.clone(),
            mode: self.mode,
            palette,
            styles: styles.clone(),
        });
        styles
    }
    fn structure(&self) -> Arc<norito_model::structure::Structure> {
        let mut cache = self.structure_cache.borrow_mut();
        if let Some((source, structure)) = cache.as_ref()
            && source == &self.projection.source
        {
            return structure.clone();
        }
        let structure = Arc::new(norito_model::structure::Structure::parse(
            &self.projection.source,
        ));
        *cache = Some((self.projection.source.clone(), structure.clone()));
        structure
    }
    fn compute_styles(&self, theme: &Theme) -> Styles {
        let raw;
        let projection = if self.mode == Mode::Rich {
            &self.projection
        } else {
            raw = Projection::raw(&self.projection.source);
            &raw
        };
        let mut lines: Vec<Vec<Span>> = vec![vec![]];
        let kinds: Vec<_> = projection
            .line_styles
            .iter()
            .map(|style| (style.heading, style.block_code))
            .collect();
        let mut line_start = 0;
        for cell in &projection.cells {
            let value = &projection.text[cell.display.clone()];
            if value.contains('\n') {
                lines.push(vec![]);
                line_start = cell.display.end;
                continue;
            }
            let style = cell.style;
            let size = match style.heading {
                1 => 24.0,
                2 => 20.0,
                3 => 17.0,
                4..=6 => 16.0,
                _ => {
                    if style.small {
                        12.0
                    } else if style.code {
                        14.0
                    } else {
                        15.0
                    }
                }
            };
            let font = Font {
                family: if style.code {
                    iced::font::Family::Monospace
                } else {
                    iced::font::Family::Name("Yu Gothic UI")
                },
                weight: if style.bold || style.heading > 0 {
                    iced::font::Weight::Semibold
                } else {
                    iced::font::Weight::Normal
                },
                style: if style.italic {
                    iced::font::Style::Oblique
                } else {
                    iced::font::Style::Normal
                },
                ..Font::DEFAULT
            };
            let format = Format {
                line_height: Some(
                    if style.heading > 0 {
                        size * 1.3
                    } else if style.block_code {
                        23.0
                    } else {
                        27.0
                    }
                    .into(),
                ),
                font: Some(font),
                size: Some(size.into()),
                color: Some(if style.block_code {
                    self.code_colors
                        .get(cell.source.start)
                        .copied()
                        .flatten()
                        .unwrap_or(iced::Color::from_rgb8(210, 216, 225))
                } else if style.heading > 0 && !style.syntax {
                    theme.extended_palette().background.strongest.text
                } else if style.syntax {
                    theme
                        .extended_palette()
                        .background
                        .strong
                        .text
                        .scale_alpha(0.6)
                } else if style.quote {
                    theme.extended_palette().background.strong.text
                } else if style.link {
                    theme.palette().primary
                } else {
                    theme.palette().text
                }),
            };
            let range = cell.display.start - line_start..cell.display.end - line_start;
            let line = lines.last_mut().unwrap();
            if let Some((last, old_format)) = line.last_mut()
                && *old_format == format
                && last.end == range.start
            {
                last.end = range.end;
            } else {
                line.push((range, format));
            }
        }
        let last = lines.len().saturating_sub(1);
        for (index, line) in lines.iter_mut().enumerate() {
            if line.is_empty() && kinds[index].0 > 0 {
                let size = match kinds[index].0 {
                    1 => 24.0,
                    2 => 20.0,
                    3 => 17.0,
                    _ => 16.0,
                };
                line.push((
                    0..0,
                    Format {
                        size: Some(size.into()),
                        line_height: Some((size * 1.3).into()),
                        font: Some(Font {
                            family: iced::font::Family::Name("Yu Gothic UI"),
                            weight: iced::font::Weight::Semibold,
                            ..Font::DEFAULT
                        }),
                        ..Default::default()
                    },
                ));
            }
            if line.is_empty() && index != last {
                let gap = if kinds[index].1 {
                    23.0
                } else if kinds.get(index + 1).is_some_and(|kind| kind.0 > 0) {
                    24.0
                } else if index > 0 && kinds[index - 1].0 > 0 {
                    12.0
                } else {
                    16.0
                };
                line.push((
                    0..0,
                    Format {
                        size: Some(if kinds[index].1 { 14.0 } else { gap / 1.8 }.into()),
                        font: kinds[index].1.then_some(Font::MONOSPACE),
                        line_height: Some(gap.into()),
                        ..Default::default()
                    },
                ));
            }
        }
        for (line, _, _) in if self.mode == Mode::Rich {
            self.diagram_slots()
        } else {
            vec![]
        } {
            if let Some(spans) = lines.get_mut(line) {
                *spans = vec![(
                    0..0,
                    Format {
                        size: Some(15.0.into()),
                        line_height: Some(180.0.into()),
                        ..Default::default()
                    },
                )];
            }
        }
        if self.mode == Mode::Rich {
            let structure = self.structure();
            for cell in &projection.cells {
                let height = if cell.style.table {
                    structure
                        .tables
                        .iter()
                        .find(|t| t.range.start == cell.source.start)
                        .map_or(68.0, |t| t.cells.len() as f32 * 34.0 + 8.0)
                } else if cell.style.image {
                    220.0
                } else if cell.style.rule {
                    24.0
                } else if cell.style.math {
                    if cell.style.diagram { 90.0 } else { 27.0 }
                } else {
                    continue;
                };
                let line = projection.text[..cell.display.start]
                    .bytes()
                    .filter(|c| *c == b'\n')
                    .count();
                if cell.style.math && !cell.style.diagram {
                    for (_, format) in &mut lines[line] {
                        format.line_height = Some(height.into());
                    }
                } else {
                    lines[line] = vec![(
                        0..0,
                        Format {
                            size: Some(15.0.into()),
                            line_height: Some(height.into()),
                            ..Default::default()
                        },
                    )];
                }
            }
        }
        Styles(Arc::new(lines))
    }
    pub fn view<'a>(
        &'a self,
        theme: &Theme,
        enabled: bool,
        diagrams: &std::collections::HashMap<String, Result<iced::widget::svg::Handle, String>>,
        images: &std::collections::HashMap<String, Result<iced::widget::image::Handle, String>>,
    ) -> Element<'a, EditorEvent> {
        let settings = self.styles(theme);
        let raw;
        let projection = if self.mode == Mode::Rich {
            &self.projection
        } else {
            raw = Projection::raw(&self.projection.source);
            &raw
        };
        let mut backgrounds: Vec<(Range<usize>, iced::Color, f32)> = Vec::new();
        for (line, style) in projection.line_styles.iter().enumerate() {
            if style.block_code && !style.diagram {
                if let Some((range, _, _)) = backgrounds.last_mut()
                    && range.end == line
                {
                    range.end += 1;
                } else {
                    backgrounds.push((
                        line..line + 1,
                        if theme.extended_palette().is_dark {
                            norito_iced_ext::oklch(0.245, 0.018, 58.0)
                        } else {
                            norito_iced_ext::oklch(0.190, 0.014, 55.0)
                        },
                        8.0,
                    ));
                }
            }
        }
        let mut marks = vec![];
        let mut line = 0;
        let mut start = 0;
        for cell in &projection.cells {
            if projection.text[cell.display.clone()].contains('\n') {
                line += 1;
                start = cell.display.end;
            } else if cell.style.highlight {
                marks.push((
                    line,
                    cell.display.start - start..cell.display.end - start,
                    iced::Color::from_rgba(0.8, 0.6, 0.15, 0.35),
                ));
            }
        }
        let mut editor = text_editor(&self.content)
            .layout_theme(theme.clone())
            .key_binding(|key| {
                use iced::keyboard::{Key, key::Named};
                use text_editor::{Binding, Status};
                if matches!(key.status, Status::Focused { .. }) {
                    if key.key == Key::Named(Named::ContextMenu)
                        || (key.key == Key::Named(Named::F10) && key.modifiers.shift())
                    {
                        return Some(Binding::Custom(EditorEvent::ContextMenu));
                    }
                    if key.modifiers.command() && !key.modifiers.alt() {
                        if key.key == Key::Named(Named::Enter) {
                            return Some(Binding::Custom(EditorEvent::FollowLink));
                        }
                        let event = match key
                            .key
                            .to_latin(key.physical_key)
                            .map(|c| c.to_ascii_lowercase())
                        {
                            Some('z') if key.modifiers.shift() => Some(EditorEvent::Redo),
                            Some('z') => Some(EditorEvent::Undo),
                            Some('y') => Some(EditorEvent::Redo),
                            Some('q') if key.modifiers.shift() => {
                                Some(EditorEvent::Format(Formatting::Quote))
                            }
                            Some('t') if key.modifiers.shift() => Some(EditorEvent::InsertTable),
                            Some('x') if key.modifiers.shift() => {
                                Some(EditorEvent::Format(Formatting::Strike))
                            }
                            Some('b') => Some(EditorEvent::Format(Formatting::Bold)),
                            Some('i') => Some(EditorEvent::Format(Formatting::Italic)),
                            Some('e') => Some(EditorEvent::Format(Formatting::Code)),
                            Some(n @ '0'..='6') => {
                                Some(EditorEvent::Format(Formatting::Heading(n as u8 - b'0')))
                            }
                            _ => None,
                        };
                        if let Some(event) = event {
                            return Some(Binding::Custom(event));
                        }
                    }
                }
                Binding::from_key_press(key)
            })
            .id("document")
            .font(if self.mode == Mode::Source {
                Font::MONOSPACE
            } else {
                Font::with_name("Yu Gothic UI")
            })
            .highlight_with::<StyledText>(settings, |format, _| *format)
            .block_backgrounds(backgrounds)
            .span_backgrounds(marks)
            .size(15)
            .line_height(1.8)
            .padding(norito_iced_ext::EDITOR_PADDING)
            .height(Length::Fill)
            .style(|theme: &Theme, _| text_editor::Style {
                background: theme.palette().background.into(),
                border: iced::Border::default(),
                value: theme.palette().text,
                placeholder: theme.palette().text,
                selection: theme.extended_palette().primary.weak.color,
            });
        if enabled {
            editor = editor.on_action(EditorEvent::Action);
        }
        use norito_iced_ext::editor_decorations::{Decoration, decorate};
        let mut decorations = vec![];
        if self.mode == Mode::Rich {
            let structure = self.structure();
            let mut line = 0;
            let mut line_start = 0;
            for cell in &self.projection.cells {
                let value = &self.projection.text[cell.display.clone()];
                if value.contains('\n') {
                    line += 1;
                    line_start = cell.display.end;
                    continue;
                }
                if cell.style.table
                    && let Some(table) = structure
                        .tables
                        .iter()
                        .find(|t| t.range.start == cell.source.start)
                {
                    decorations.push(Decoration::Table {
                        line,
                        alignment: table.alignment.clone(),
                        cells: table
                            .cells
                            .iter()
                            .map(|r| r.iter().map(|c| Projection::new(c).text).collect())
                            .collect(),
                        on_cell: table
                            .cells
                            .iter()
                            .enumerate()
                            .map(|(r, values)| {
                                values
                                    .iter()
                                    .enumerate()
                                    .map(|(c, _)| {
                                        EditorEvent::Table(TableAction::Focus(
                                            cell.source.start,
                                            r,
                                            c,
                                        ))
                                    })
                                    .collect()
                            })
                            .collect(),
                    });
                }
                if cell.style.image
                    && let Some(link) = structure
                        .links
                        .iter()
                        .find(|l| l.image && l.range.start == cell.source.start)
                {
                    decorations.push(Decoration::Image {
                        line,
                        handle: images
                            .get(&link.destination)
                            .and_then(|r| r.as_ref().ok())
                            .cloned(),
                        on_press: EditorEvent::EditCode(cell.source.start),
                    });
                }
                if cell.style.math
                    && let Some(math) = structure
                        .math
                        .iter()
                        .find(|m| m.range.start == cell.source.start)
                {
                    let key = format!("math:{}", math.content);
                    let handle = diagrams.get(&key).and_then(|r| r.as_ref().ok()).cloned();
                    let on_press = EditorEvent::EditCode(cell.source.start);
                    if math.display {
                        decorations.push(Decoration::Diagram {
                            line,
                            handle,
                            status: diagrams
                                .get(&key)
                                .and_then(|r| r.as_ref().err())
                                .cloned()
                                .unwrap_or_else(|| "数式を描画中…".into()),
                            on_press,
                        });
                    } else {
                        decorations.push(Decoration::InlineDiagram {
                            line,
                            range: cell.display.start - line_start..cell.display.end - line_start,
                            handle,
                            on_press,
                        });
                    }
                }
                if cell.style.rule {
                    decorations.push(Decoration::Separator { line });
                }
                if cell.style.strike && !cell.style.syntax {
                    decorations.push(Decoration::Strike {
                        line,
                        range: cell.display.start - line_start..cell.display.end - line_start,
                    });
                }
            }
            for (line, style) in self.projection.line_styles.iter().enumerate() {
                if style.quote {
                    decorations.push(Decoration::Rule {
                        line,
                        color: theme.extended_palette().background.strong.color,
                    });
                }
            }
            for (line, source, content) in self.diagram_slots() {
                decorations.push(Decoration::Diagram {
                    line,
                    status: match diagrams.get(&content) {
                        Some(Err(error)) => format!("Mermaid: {error}\nクリックして編集"),
                        _ => "Mermaidを描画中…".into(),
                    },
                    handle: diagrams
                        .get(&content)
                        .and_then(|r| r.as_ref().ok())
                        .cloned(),
                    on_press: EditorEvent::EditCode(source),
                });
            }
        }
        let base = decorate(editor.into(), &self.content, decorations);
        if self.mode == Mode::Rich
            && let Some((line, child, clicks)) = self.table_inline()
        {
            norito_iced_ext::inline_editor::inline(base, &self.content, line, child, clicks)
        } else {
            base
        }
    }
}

fn offset(text: &str, position: Position) -> usize {
    let start: usize = text
        .split_inclusive('\n')
        .take(position.line)
        .map(str::len)
        .sum();
    (start + position.column).min(text.len())
}
fn position(text: &str, mut offset: usize) -> Position {
    offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    let prefix = &text[..offset];
    Position {
        line: prefix.bytes().filter(|b| *b == b'\n').count(),
        column: prefix.rfind('\n').map_or(offset, |n| offset - n - 1),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "manual continuous input timing"]
    fn measure_continuous_input() {
        use super::*;
        let source = "本文 **太字** and text.\n\n".repeat(400);
        for rebuild in [true, false] {
            let mut doc = Document::new(source.clone());
            let mut editor = MarkdownEditor::new(&doc);
            editor.jump(source.len());
            let start = std::time::Instant::now();
            for _ in 0..200 {
                let edited = editor
                    .perform(text_editor::Action::Edit(text_editor::Edit::Insert('a')))
                    .unwrap();
                doc.edit(edited);
                editor.synchronize(&doc, false);
                std::hint::black_box(editor.styles(&Theme::Dark));
                if rebuild {
                    let caret = editor.content.cursor();
                    editor.content = text_editor::Content::with_text(editor.displayed());
                    editor.content.move_to(caret);
                }
            }
            assert!(doc.source.ends_with(&"a".repeat(200)));
            eprintln!(
                "200 keys, {} bytes, force rebuild={rebuild}: {:?}",
                source.len(),
                start.elapsed()
            );
        }
    }
    #[test]
    fn style_cache_reuses_spans_and_invalidates_for_theme_mode_and_edits() {
        use super::*;
        let mut document = Document::from_bytes(b"# Heading\n\n**bold**\n").unwrap();
        let mut editor = MarkdownEditor::new(&document);
        let first = editor.styles(&Theme::Dark);
        assert!(Arc::ptr_eq(&first.0, &editor.styles(&Theme::Dark).0));
        assert!(!Arc::ptr_eq(&first.0, &editor.styles(&Theme::Light).0));
        editor.set_mode(Mode::Source);
        assert_eq!(
            editor.styles(&Theme::Dark),
            editor.compute_styles(&Theme::Dark)
        );
        document = Document::from_bytes(b"## Changed\n").unwrap();
        editor.synchronize(&document, true);
        assert_eq!(
            editor.styles(&Theme::Dark),
            editor.compute_styles(&Theme::Dark)
        );
        assert!(!Arc::ptr_eq(&first.0, &editor.styles(&Theme::Dark).0));
    }

    #[test]
    #[ignore = "manual redraw timing"]
    fn measure_cached_styles() {
        use super::*;
        let source = "# Heading\n\nText **bold** and *italic*.\n\n".repeat(250);
        let editor = MarkdownEditor::new(&Document::from_bytes(source.as_bytes()).unwrap());
        editor.styles(&Theme::Dark);
        let start = std::time::Instant::now();
        for _ in 0..100 {
            std::hint::black_box(editor.compute_styles(&Theme::Dark));
        }
        let uncached = start.elapsed();
        let start = std::time::Instant::now();
        for _ in 0..100 {
            std::hint::black_box(editor.styles(&Theme::Dark));
        }
        eprintln!(
            "100 style passes, {} bytes: uncached {:?}, cached {:?}",
            source.len(),
            uncached,
            start.elapsed()
        );
    }
    use super::*;
    #[test]
    fn format_shortcut_works_without_selection_and_in_source_mode() {
        let mut doc = Document::new(String::new());
        let mut editor = MarkdownEditor::new(&doc);
        doc.edit(editor.format(Formatting::Bold).unwrap());
        editor.synchronize(&doc, true);
        doc.edit(
            editor
                .perform(text_editor::Action::Edit(text_editor::Edit::Paste(
                    Arc::new("本文".into()),
                )))
                .unwrap(),
        );
        editor.synchronize(&doc, false);
        assert_eq!(doc.source, "**本文**");
        editor.set_mode(Mode::Source);
        editor.select_source(2..8);
        doc.edit(editor.format(Formatting::Bold).unwrap());
        editor.synchronize(&doc, true);
        assert_eq!(doc.source, "本文");
        assert_eq!(editor.selected_text(), "本文");
    }
    #[test]
    fn multiline_formatting_through_editor_preserves_trailing_newline() {
        for mode in [Mode::Rich, Mode::Source] {
            let mut doc = Document::new("一行目\n二行目\n".into());
            let mut editor = MarkdownEditor::new(&doc);
            editor.set_mode(mode);
            editor.select_source(0..doc.source.len());
            doc.edit(editor.format(Formatting::Bold).unwrap());
            editor.synchronize(&doc, true);
            assert_eq!(doc.source, "**一行目**\n**二行目**\n");
            assert!(!editor.selected_text().ends_with('\n'));
        }
    }
    #[test]
    fn bold_over_mixed_selection_can_be_toggled_again() {
        for mode in [Mode::Rich, Mode::Source] {
            for original in ["before **bold** after", "**one**\ntwo\n"] {
                let mut doc = Document::new(original.into());
                let mut editor = MarkdownEditor::new(&doc);
                editor.set_mode(mode);
                editor.select_source(0..doc.source.len());
                doc.edit(editor.format(Formatting::Bold).unwrap());
                editor.synchronize(&doc, true);
                assert_eq!(
                    doc.source,
                    if original.contains('\n') {
                        "**one**\n**two**\n"
                    } else {
                        "**before bold after**"
                    }
                );
                doc.edit(editor.format(Formatting::Bold).unwrap());
                editor.synchronize(&doc, true);
                assert_eq!(
                    doc.source,
                    if original.contains('\n') {
                        "one\ntwo\n"
                    } else {
                        "before bold after"
                    }
                );
            }
        }
    }
    #[test]
    fn backspace_removes_only_generated_empty_pairs() {
        let mut doc = Document::new(String::new());
        let mut editor = MarkdownEditor::new(&doc);
        for edit in [
            text_editor::Edit::Insert('*'),
            text_editor::Edit::Insert('*'),
            text_editor::Edit::Backspace,
            text_editor::Edit::Backspace,
        ] {
            let source = editor.perform(text_editor::Action::Edit(edit)).unwrap();
            doc.edit(source);
            editor.synchronize(&doc, false);
        }
        assert_eq!(doc.source, "");
        assert_eq!(editor.source_selection(), 0..0);
    }
    #[test]
    fn generated_bold_closers_survive_ime_commit_and_are_skipped() {
        let mut doc = Document::new(String::new());
        let mut editor = MarkdownEditor::new(&doc);
        for edit in [
            text_editor::Edit::Insert('*'),
            text_editor::Edit::Insert('*'),
            text_editor::Edit::Paste(Arc::new("日本語".into())),
            text_editor::Edit::Insert('*'),
            text_editor::Edit::Insert('*'),
        ] {
            if let Some(source) = editor.perform(text_editor::Action::Edit(edit)) {
                doc.edit(source);
                editor.synchronize(&doc, false);
            }
        }
        assert_eq!(doc.source, "**日本語**");
        assert_eq!(
            editor.source_selection(),
            doc.source.len()..doc.source.len()
        );
        doc.edit(
            editor
                .perform(text_editor::Action::Edit(text_editor::Edit::Insert('外')))
                .unwrap(),
        );
        assert_eq!(doc.source, "**日本語**外");
    }
    #[test]
    fn diagram_reveals_in_place_and_collapses_without_changing_source() {
        let source = "```mermaid\nflowchart LR; A-->B\n```\n\n本文";
        let doc = Document::new(source.into());
        let mut editor = MarkdownEditor::new(&doc);
        assert_eq!(editor.diagram_slots().len(), 1);
        assert!(!editor.displayed().contains("flowchart"));
        editor.edit_code(0);
        assert!(editor.displayed().contains("```mermaid"));
        assert_eq!(editor.diagram_slots().len(), 0);
        editor.finish_editing();
        assert_eq!(editor.diagram_slots().len(), 1);
        assert_eq!(editor.projection.source, source);
    }
    #[test]
    fn code_highlighting_and_source_heading_metrics_are_shared() {
        let colors = syntax::colors("```rust\nfn main() { let n = 42; }\n```\n");
        let distinct: std::collections::HashSet<_> = colors
            .iter()
            .flatten()
            .map(|c| (c.r.to_bits(), c.g.to_bits(), c.b.to_bits()))
            .collect();
        assert!(distinct.len() > 2);
        let doc = Document::new("# Heading".into());
        let mut editor = MarkdownEditor::new(&doc);
        let rich_size = editor.styles(&Theme::Dark).0[0][0].1.size;
        editor.set_mode(Mode::Source);
        assert_eq!(editor.styles(&Theme::Dark).0[0][0].1.size, rich_size);
    }
    #[test]
    fn consecutive_headings_keep_ime_commits_after_their_markers() {
        let mut doc = Document::new(String::new());
        let mut editor = MarkdownEditor::new(&doc);
        let actions = [
            text_editor::Edit::Insert('#'),
            text_editor::Edit::Insert(' '),
            text_editor::Edit::Paste(Arc::new("見出し".into())),
            text_editor::Edit::Enter,
            text_editor::Edit::Insert('#'),
            text_editor::Edit::Insert('#'),
            text_editor::Edit::Insert(' '),
            text_editor::Edit::Paste(Arc::new("見出し".into())),
            text_editor::Edit::Insert('2'),
        ];
        for action in actions {
            if let Some(source) = editor.perform(text_editor::Action::Edit(action)) {
                doc.edit(source);
                editor.synchronize(&doc, false);
            }
        }
        assert_eq!(doc.source, "# 見出し\n## 見出し2");
        assert_eq!(editor.content.text(), "見出し\n見出し2");
        assert_eq!(
            editor.content.cursor().position,
            Position {
                line: 1,
                column: 10
            }
        );
    }
    #[test]
    fn empty_heading_composition_matches_committed_layout_at_every_level() {
        use iced::advanced::{
            graphics::text::Editor,
            text::{Editor as _, Highlighter as _, LineHeight, Wrapping},
        };
        fn layout(source: &str, line: usize, column: usize) -> Editor {
            let document = Document::new(source.into());
            let widget = MarkdownEditor::new(&document);
            let mut editor = Editor::with_text(&widget.projection.text);
            let mut highlighter = StyledText::new(&widget.styles(&Theme::Dark));
            editor.update(
                iced::Size::new(720.0, 500.0),
                Font::with_name("Yu Gothic UI"),
                15.0.into(),
                LineHeight::Relative(1.8),
                Wrapping::WordOrGlyph,
                &mut highlighter,
            );
            editor.highlight(
                Font::with_name("Yu Gothic UI"),
                &mut highlighter,
                |format| *format,
            );
            editor.move_to(Cursor {
                position: Position { line, column },
                selection: None,
            });
            editor
        }
        for level in 1..=6 {
            for (before, line, after) in [("", 0, ""), ("本文\n\n", 2, "\n\n後続の本文")] {
                let marker = "#".repeat(level);
                let source = format!("{before}{marker} {after}");
                let original = layout(&source, line, 0);
                let content = "日本語の見出し";
                let preview = original
                    .with_preedit(content, Some(content.len()..content.len()))
                    .unwrap();
                let committed = layout(
                    &format!("{before}{marker} {content}{after}"),
                    line,
                    content.len(),
                );
                assert_eq!(
                    preview.caret_bounds(),
                    committed.caret_bounds(),
                    "H{level}: {source:?}"
                );
                let geometry = |editor: &Editor| {
                    editor
                        .buffer()
                        .layout_runs()
                        .map(|run| {
                            (
                                run.line_y,
                                run.line_height,
                                run.glyphs
                                    .iter()
                                    .map(|glyph| (glyph.font_id, glyph.font_size, glyph.x, glyph.y))
                                    .collect::<Vec<_>>(),
                            )
                        })
                        .collect::<Vec<_>>()
                };
                assert_eq!(
                    geometry(&preview),
                    geometry(&committed),
                    "H{level}: {source:?}"
                );
            }
        }
    }
    #[test]
    fn typing_inline_markdown_shortcuts_preserves_source() {
        for input in [
            "**日本語**",
            "*italic*",
            "[link](https://example.com)",
            "`code`",
            "> quote",
            "- list",
        ] {
            let mut doc = Document::new(String::new());
            let mut editor = MarkdownEditor::new(&doc);
            for c in input.chars() {
                if let Some(source) =
                    editor.perform(text_editor::Action::Edit(text_editor::Edit::Insert(c)))
                {
                    doc.edit(source);
                    editor.synchronize(&doc, false);
                }
            }
            assert_eq!(doc.source, input, "typing {input}");
        }
    }
    #[test]
    fn enter_typing_backspace_and_undo_preserve_bold() {
        let mut doc = Document::new("**太字**".into());
        let mut editor = MarkdownEditor::new(&doc);
        editor.jump(doc.source.len());
        for edit in [
            text_editor::Edit::Enter,
            text_editor::Edit::Insert('次'),
            text_editor::Edit::Enter,
            text_editor::Edit::Insert('行'),
        ] {
            doc.edit(editor.perform(text_editor::Action::Edit(edit)).unwrap());
            editor.synchronize(&doc, false);
        }
        assert_eq!(doc.source, "**太字**\n次\n行");
        doc.undo();
        editor.synchronize(&doc, true);
        assert_eq!(editor.content.text(), "太字\n次\n");
        let mut doc = Document::new("**太字**".into());
        let mut editor = MarkdownEditor::new(&doc);
        editor.jump(5);
        doc.edit(
            editor
                .perform(text_editor::Action::Edit(text_editor::Edit::Enter))
                .unwrap(),
        );
        editor.synchronize(&doc, false);
        assert_eq!(doc.source, "**太**\n**字**");
        editor.perform(text_editor::Action::Move(text_editor::Motion::Home));
        doc.edit(
            editor
                .perform(text_editor::Action::Edit(text_editor::Edit::Backspace))
                .unwrap(),
        );
        editor.synchronize(&doc, false);
        assert_eq!(doc.source, "**太字**");
    }
    #[test]
    fn fenced_mermaid_typing_and_paste_preserve_markdown() {
        let input = "```mermaid\nflowchart LR\nA-->B\n```\n\n本文";
        for paste in [false, true] {
            let mut doc = Document::new(String::new());
            let mut editor = MarkdownEditor::new(&doc);
            let edits = if paste {
                vec![text_editor::Edit::Paste(Arc::new(input.into()))]
            } else {
                input
                    .chars()
                    .map(|c| {
                        if c == '\n' {
                            text_editor::Edit::Enter
                        } else {
                            text_editor::Edit::Insert(c)
                        }
                    })
                    .collect()
            };
            for edit in edits {
                if let Some(source) = editor.perform(text_editor::Action::Edit(edit)) {
                    doc.edit(source);
                }
                editor.synchronize(&doc, false);
            }
            assert_eq!(doc.source, input, "paste={paste}");
            assert_eq!(editor.diagram_slots().len(), 1);
        }
    }
    #[test]
    fn quote_formats_all_selected_lines_and_toggles_off() {
        let mut doc = Document::new("first\nsecond".into());
        let mut editor = MarkdownEditor::new(&doc);
        editor.perform(text_editor::Action::SelectAll);
        doc.edit(editor.format(Formatting::Quote).unwrap());
        editor.synchronize(&doc, false);
        assert_eq!(doc.source, "> first\n> second");
        assert!(editor.projection.line_styles.iter().all(|s| s.quote));
        doc.edit(editor.format(Formatting::Quote).unwrap());
        assert_eq!(doc.source, "first\nsecond");
    }
    #[test]
    fn leaving_quote_or_list_keeps_next_paragraph_outside() {
        for initial in ["> 引用", "- item", "1. item"] {
            let mut doc = Document::new(initial.into());
            let mut editor = MarkdownEditor::new(&doc);
            editor.jump(doc.source.len());
            for edit in [
                text_editor::Edit::Enter,
                text_editor::Edit::Enter,
                text_editor::Edit::Paste(Arc::new("通常の本文".into())),
            ] {
                doc.edit(editor.perform(text_editor::Action::Edit(edit)).unwrap());
                editor.synchronize(&doc, false);
            }
            assert_eq!(doc.source, format!("{initial}\n\n通常の本文"));
            assert!(!editor.projection.line_styles.last().unwrap().quote);
        }
    }
    #[test]
    fn repeated_character_insertion_uses_cursor_not_ambiguous_text_diff() {
        let mut doc = Document::new("**aa**aa".into());
        let mut editor = MarkdownEditor::new(&doc);
        editor.content.move_to(Cursor {
            position: Position { line: 0, column: 1 },
            selection: None,
        });
        doc.edit(
            editor
                .perform(text_editor::Action::Edit(text_editor::Edit::Insert('a')))
                .unwrap(),
        );
        editor.synchronize(&doc, false);
        assert_eq!(doc.source, "**aaa**aa");
    }
    #[test]
    fn cross_block_selection_replacement_and_undo() {
        let mut doc = Document::new("# Heading\n\n**body** text".into());
        let mut editor = MarkdownEditor::new(&doc);
        editor.perform(text_editor::Action::SelectAll);
        doc.edit(
            editor
                .perform(text_editor::Action::Edit(text_editor::Edit::Paste(
                    Arc::new("新しい本文".into()),
                )))
                .unwrap(),
        );
        editor.synchronize(&doc, false);
        assert_eq!(doc.source, "新しい本文");
        doc.undo();
        editor.synchronize(&doc, true);
        assert_eq!(editor.content.text(), "Heading\n\nbody text");
    }
    #[test]
    fn heading_command_keeps_inline_formatting() {
        let mut doc = Document::new("**text**".into());
        let mut editor = MarkdownEditor::new(&doc);
        doc.edit(editor.format(Formatting::Heading(2)).unwrap());
        editor.synchronize(&doc, false);
        assert_eq!(doc.source, "## **text**");
    }
    #[test]
    fn edits_rich_text_and_saves_markdown_not_plain_text() {
        let mut doc = Document::new("# Heading\n\n**日本語**".into());
        let mut editor = MarkdownEditor::new(&doc);
        assert_eq!(editor.content.text(), "Heading\n\n日本語");
        editor.content.move_to(Cursor {
            position: Position { line: 2, column: 3 },
            selection: None,
        });
        let source = editor
            .perform(text_editor::Action::Edit(text_editor::Edit::Insert('本')))
            .unwrap();
        doc.edit(source);
        editor.synchronize(&doc, false);
        assert_eq!(doc.source, "# Heading\n\n**日本本語**");
        assert_eq!(editor.content.text(), "Heading\n\n**日本本語**");
        doc.undo();
        editor.synchronize(&doc, true);
        assert_eq!(editor.content.text(), "Heading\n\n日本語");
    }
    #[test]
    fn markdown_shortcut_becomes_rich_heading_without_losing_cursor() {
        let mut doc = Document::new(String::new());
        let mut editor = MarkdownEditor::new(&doc);
        for c in "# Title".chars() {
            let source = editor
                .perform(text_editor::Action::Edit(text_editor::Edit::Insert(c)))
                .unwrap();
            doc.edit(source);
            editor.synchronize(&doc, false);
        }
        assert_eq!(doc.source, "# Title");
        assert_eq!(editor.content.text(), "Title");
    }
    #[test]
    fn source_switch_preserves_original_bytes() {
        let doc = Document::new("# Heading\r\n\r\n**日本語**\r\n".into());
        let mut editor = MarkdownEditor::new(&doc);
        editor.set_mode(Mode::Source);
        assert_eq!(editor.content.text(), doc.source);
        editor.set_mode(Mode::Rich);
        assert_eq!(editor.content.text(), "Heading\r\n\r\n日本語\r\n");
    }
    #[test]
    fn mode_switch_preserves_selected_text() {
        let doc = Document::new("# Heading\n\n日本語".into());
        let mut editor = MarkdownEditor::new(&doc);
        editor.content.move_to(Cursor {
            position: Position { line: 2, column: 9 },
            selection: Some(Position { line: 2, column: 0 }),
        });
        editor.set_mode(Mode::Source);
        assert_eq!(&editor.content.text()[editor.selection()], "日本語");
        editor.set_mode(Mode::Rich);
        assert_eq!(&editor.content.text()[editor.selection()], "日本語");
    }
}
