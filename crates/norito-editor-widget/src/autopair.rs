//! Generated closing delimiters: inserted with their opener, stepped over when
//! typed, removed with an empty pair, and forgotten on any structural change.
use crate::{MarkdownEditor, Mode};
use iced::widget::text_editor::{Action, Edit};
use std::ops::Range;

/// Source offsets of closing delimiters the editor generated.
#[derive(Debug, Default)]
pub(crate) struct AutoClosers(Vec<usize>);

impl AutoClosers {
    pub(crate) fn clear(&mut self) {
        self.0.clear();
    }
    pub(crate) fn contains(&self, at: usize) -> bool {
        self.0.contains(&at)
    }
    pub(crate) fn push(&mut self, at: usize) {
        self.0.push(at);
    }
    pub(crate) fn remove(&mut self, at: usize) {
        self.0.retain(|p| *p != at);
    }
    /// Move closers at or after `from` by `delta` bytes.
    pub(crate) fn shift_from(&mut self, from: usize, delta: isize) {
        for p in &mut self.0 {
            if *p >= from {
                *p = p.saturating_add_signed(delta);
            }
        }
    }
    /// `range` of the source was replaced, changing its length by `delta`.
    pub(crate) fn replaced(&mut self, range: Range<usize>, delta: isize) {
        self.0.retain(|p| *p < range.start || *p >= range.end);
        self.shift_from(range.end, delta);
    }
}

fn closer(opening: char) -> Option<char> {
    match opening {
        '*' | '_' | '`' | '~' => Some(opening),
        '[' => Some(']'),
        '(' => Some(')'),
        _ => None,
    }
}

impl MarkdownEditor {
    /// `Some(result)` when the action was handled here; `Some(None)` handled without an edit.
    pub(crate) fn auto_pair(&mut self, action: &Action) -> Option<Option<String>> {
        if self.mode != Mode::Rich {
            return None;
        }
        let selection = self.source_selection();
        let at = selection.start;
        let mut source = self.projection.source.clone();
        if matches!(action, Action::Edit(Edit::Backspace))
            && selection.is_empty()
            && self.auto_closers.contains(at)
        {
            let opening = source[..at].chars().next_back()?;
            let closing = source[at..].chars().next()?;
            if closer(opening) == Some(closing) {
                let start = at - opening.len_utf8();
                let end = at + closing.len_utf8();
                source.replace_range(start..end, "");
                self.auto_closers.remove(at);
                self.auto_closers.shift_from(end, -((end - start) as isize));
                self.pending_selection = Some(start..start);
                return Some(Some(source));
            }
        }
        let Action::Edit(Edit::Insert(ch)) = action else {
            return None;
        };
        let ch = *ch;
        if self.projection.in_code_block(at) {
            return None;
        }
        let width = ch.len_utf8();
        if selection.is_empty() && source[at..].starts_with(ch) && self.auto_closers.contains(at) {
            let prev = source[..at].chars().next_back();
            // A second opening star creates **|**, not *|* followed by a close.
            if matches!(ch, '*' | '_' | '~')
                && prev == Some(ch)
                && source[..at]
                    .rsplit(char::is_whitespace)
                    .next()
                    .is_some_and(|run| run.chars().all(|c| c == ch))
                && source[..at].chars().rev().take_while(|c| *c == ch).count() < 2
            {
                source.insert_str(at, &format!("{ch}{ch}"));
                self.auto_closers.shift_from(at, 2);
                self.auto_closers.push(at + 1);
                self.pending_selection = Some(at + 1..at + 1);
                return Some(Some(source));
            }
            // Typing the generated closer steps over it.
            self.auto_closers.remove(at);
            self.activate(at + width..at + width);
            self.reset_input(at + width..at + width);
            return Some(None);
        }
        let line = source[..at].rsplit('\n').next().unwrap_or("");
        let fence_start = ch == '`' && line.chars().all(|c| c == '`' || c.is_whitespace());
        let closing_inline_code = ch == '`' && line.chars().filter(|c| *c == '`').count() % 2 == 1;
        if let Some(paired) = closer(ch)
            && !fence_start
            && !closing_inline_code
            && (source[at..].starts_with(char::is_whitespace)
                || at == source.len()
                || !selection.is_empty())
        {
            source.insert(selection.end, paired);
            source.insert(at, ch);
            self.auto_closers.push(selection.end + width);
            self.pending_selection = Some(at + width..selection.end + width);
            return Some(Some(source));
        }
        // Keep generated closing markers aligned while text is inserted.
        if selection.is_empty() && !self.auto_closers.0.is_empty() {
            source.insert(at, ch);
            self.auto_closers.shift_from(at, width as isize);
            self.pending_selection = Some(at + width..at + width);
            return Some(Some(source));
        }
        None
    }
}
