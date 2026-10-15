//! Editing commands. Each returns the new source and records the selection to restore.
use crate::{Formatting, MarkdownEditor};
use norito_model::{compose, projection::Projection};
use std::ops::Range;

fn marker(formatting: Formatting) -> Option<&'static str> {
    match formatting {
        Formatting::Bold => Some("**"),
        Formatting::Italic => Some("*"),
        Formatting::Code => Some("`"),
        Formatting::Strike => Some("~~"),
        Formatting::Heading(_)
        | Formatting::Bullet
        | Formatting::Quote
        | Formatting::Indent
        | Formatting::Outdent => None,
    }
}

impl MarkdownEditor {
    pub fn format(&mut self, formatting: Formatting) -> Option<String> {
        let selected = self.selection();
        let marker = marker(formatting);
        if selected.is_empty()
            && let Some(marker) = marker
        {
            // No selection: insert an empty pair and type inside it.
            let at = self.source_selection().start;
            if self.projection.in_code_block(at) {
                return None;
            }
            let mut source = self.projection.source.clone();
            source.insert_str(at, &marker.repeat(2));
            let caret = at + marker.len();
            for closer in caret..caret + marker.len() {
                self.auto_closers.push(closer);
            }
            self.pending_selection = Some(caret..caret);
            return Some(source);
        }
        let caret = self.source_selection().start;
        let (source, selection) = match (formatting, marker) {
            (_, Some(marker)) => self.shown().format(selected, marker),
            (Formatting::Heading(level), _) => self
                .projection
                .line_prefix(caret, &"#".repeat(level as usize)),
            (Formatting::Bullet, _) => self.projection.line_prefix(caret, "-"),
            (Formatting::Indent | Formatting::Outdent, _) => self
                .projection
                .indent_list(self.source_selection(), formatting == Formatting::Outdent),
            (_, None) => self.projection.toggle_quote(self.source_selection()),
        }?;
        if source == self.projection.source {
            return None;
        }
        self.pending_selection = Some(selection);
        Some(source)
    }
    /// Insert `markdown` at `range`, replacing the selection when `range` is it.
    /// A `block` goes on its own paragraph after the current line.
    pub fn insert_markdown(
        &mut self,
        mut range: Range<usize>,
        markdown: &str,
        block: bool,
    ) -> String {
        let mut source = self.projection.source.clone();
        if self.mode == crate::Mode::Rich
            && range == self.source_selection()
            && !self.selection().is_empty()
        {
            let (cleaned, caret) = self.projection.replace(self.selection(), "");
            source = cleaned;
            range = caret..caret;
        }
        let (source, end) = compose::insert(&source, range, markdown, block);
        self.pending_selection = Some(end..end);
        source
    }
    /// Wrap the selected text in a fence and put the caret on its first line.
    pub fn insert_code(&mut self, language: &str) -> String {
        self.close_table();
        let selected = Projection::new(&self.selected_text()).text;
        let markdown = compose::fenced_code(language, &selected);
        let source = self.insert_markdown(self.source_selection(), &markdown, true);
        let start = self.pending_selection.as_ref().unwrap().end - markdown.len();
        let caret = start + markdown.find('\n').unwrap() + 1;
        self.pending_selection = Some(caret..caret);
        source
    }
    /// Flip the task marker at `at`, keeping the current caret.
    pub fn toggle_task_at(&mut self, at: usize) -> Option<String> {
        let source = self.projection.toggle_task(at)?;
        self.pending_selection = Some(self.source_selection());
        Some(source)
    }
}
