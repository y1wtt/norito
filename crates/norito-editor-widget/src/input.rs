//! Selection, typing, and keeping the text editor in step with the projection.
use crate::cursor::{offset, position};
use crate::{Formatting, MarkdownEditor, Mode};
use iced::{advanced::text::editor::Cursor, widget::text_editor};
use norito_model::{Document, projection::Projection, text::changed_range};
use std::{ops::Range, sync::Arc};

impl MarkdownEditor {
    /// Selection in displayed-text offsets.
    pub(crate) fn selection(&self) -> Range<usize> {
        let cursor = self.content.cursor();
        let text = self.content.text();
        let end = offset(&text, cursor.position);
        let start = cursor.selection.map_or(end, |p| offset(&text, p));
        start.min(end)..start.max(end)
    }
    pub fn source_selection(&self) -> Range<usize> {
        let selected = self.selection();
        if self.mode == Mode::Source {
            selected
        } else {
            self.projection.source_range(selected)
        }
    }
    pub fn selected_text(&self) -> String {
        self.content.text()[self.selection()].to_owned()
    }
    pub fn set_mode(&mut self, mode: Mode) {
        if self.mode == mode {
            return;
        }
        self.close_table();
        let source = self.source_selection();
        self.mode = mode;
        self.reset_input(source);
    }
    /// Show the current projection with `source` selected.
    pub(crate) fn reset_input(&mut self, source: Range<usize>) {
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
    /// Reproject the current source with the construct at `active` revealed.
    pub(crate) fn reveal(&mut self, active: Option<usize>) {
        self.active = active;
        self.projection = Projection::editable(&self.projection.source, active);
    }
    /// Apply an editor action. Edits return the new source for the caller to commit.
    pub fn perform(&mut self, action: text_editor::Action) -> Option<String> {
        if action.is_edit() || matches!(action, text_editor::Action::Click(_)) {
            self.close_table();
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
        use text_editor::{Action::Edit as E, Edit};
        // Backspace at the start of a hidden block prefix removes the prefix.
        if new == self.projection.text
            && matches!(operation, E(Edit::Backspace))
            && selected.is_empty()
            && self.content.cursor().position.column == 0
        {
            return self.format(Formatting::Heading(0));
        }
        // The cursor, not a text diff, says where repeated characters went.
        let old = &self.projection.text;
        let (removed, added) = match operation {
            E(Edit::Insert(_) | Edit::Paste(_) | Edit::Enter) => {
                let inserted = new.len().saturating_sub(old.len() - selected.len());
                (selected.clone(), selected.start..selected.start + inserted)
            }
            E(Edit::Backspace | Edit::Delete) => {
                let range = if !selected.is_empty() {
                    selected
                } else if matches!(operation, E(Edit::Backspace)) {
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
        let replaced = self.projection.source_range(removed.clone());
        let multiline = new[added.clone()].contains('\n');
        let (source, caret) = self.projection.replace(removed, &new[added]);
        if multiline {
            self.auto_closers.clear();
        } else {
            let delta = source.len() as isize - self.projection.source.len() as isize;
            self.auto_closers.replaced(replaced, delta);
        }
        self.pending_selection = Some(caret..caret);
        Some(source)
    }
    /// Reveal the construct at the end of `source`, keeping the selection.
    pub fn activate(&mut self, source: Range<usize>) {
        let before = std::mem::take(&mut self.projection.text);
        self.reveal(Some(source.end));
        if self.projection.text != before {
            self.reset_input(source);
        }
    }
    /// Reveal the block at `source` (diagram, image, formula) and put the caret there.
    pub fn edit_code(&mut self, source: usize) {
        self.close_table();
        self.activate(source..source);
        self.reset_input(source..source);
    }
    /// Collapse every revealed construct.
    pub fn finish_editing(&mut self) {
        self.close_table();
        let selection = self.source_selection();
        self.reveal(None);
        self.reset_input(selection);
    }
    pub fn select_source(&mut self, range: Range<usize>) {
        self.reveal(Some(range.end));
        self.reset_input(range);
    }
    pub fn jump(&mut self, source: usize) {
        self.close_table();
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
    /// Adopt `document`. `replace_input` forces the editor text to follow it
    /// (undo, another file); otherwise only a new revision is projected.
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
        if let Some(selection) = &self.pending_selection {
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
            let at = previous.min(document.source.len());
            self.reset_input(at..at);
        }
    }
}
