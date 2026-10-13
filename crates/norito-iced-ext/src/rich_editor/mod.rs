//! A continuous text editor whose spans set their own size and line height,
//! so headings are edited in the same text as the body.
//!
//! Built on Iced's public API only: cosmic-text through `iced::advanced::graphics`,
//! drawn with `fill_raw`, and Iced's editor actions, key bindings and styles.
//! IME composition is laid out inline, in the typography it will commit to.
//! Editing and input handling are adapted from Iced 0.14's `TextEditor`
//! (MIT, see `LICENSE-ICED`).
mod editor;
#[cfg(test)]
mod tests;
mod widget;

pub use editor::Editor;
pub use widget::{RichEditor, rich_editor};

use iced::Rectangle;
use iced::advanced::graphics::text::cosmic_text;
use iced::advanced::text::editor::{Action, Cursor};
use std::{cell::RefCell, ops::Range, sync::Arc};

/// The text of a [`RichEditor`], also read by widgets placed on its lines.
pub struct Content(RefCell<Internal>);

struct Internal {
    editor: Editor,
    /// The text with the IME composition inserted, and what it was built from.
    preedit: Option<(Composition, Editor)>,
    /// What was last drawn. Renderers reference it weakly until it is presented.
    frame: Option<Arc<cosmic_text::Buffer>>,
}

#[derive(Clone, PartialEq)]
struct Composition {
    content: String,
    selection: Option<Range<usize>>,
}

impl Content {
    pub fn with_text(text: &str) -> Self {
        Self(RefCell::new(Internal {
            editor: Editor::with_text(text),
            preedit: None,
            frame: None,
        }))
    }
    pub fn perform(&mut self, action: Action) {
        let internal = self.0.get_mut();
        internal.preedit = None;
        internal.editor.perform(action);
    }
    pub fn move_to(&mut self, cursor: Cursor) {
        let internal = self.0.get_mut();
        internal.preedit = None;
        internal.editor.move_to(cursor);
    }
    pub fn cursor(&self) -> Cursor {
        self.0.borrow().editor.cursor()
    }
    pub fn text(&self) -> String {
        self.0.borrow().editor.text()
    }
    /// The selected text.
    pub fn selection(&self) -> Option<String> {
        self.0.borrow().editor.copy()
    }
    pub fn line_count(&self) -> usize {
        self.0.borrow().editor.line_count()
    }
    /// The caret in text-area coordinates.
    pub fn caret_bounds(&self) -> Option<Rectangle> {
        self.0.borrow().editor.caret_bounds()
    }
    /// Visible bounds of a logical line, relative to the text area.
    pub fn line_bounds(&self, line: usize) -> Option<Rectangle> {
        self.0.borrow().editor.line_bounds(line)
    }
    /// Visible rectangles for a UTF-8 byte range on one logical line.
    pub fn span_bounds(&self, line: usize, range: Range<usize>) -> Vec<Rectangle> {
        self.0.borrow().editor.span_bounds(line, range)
    }
    /// Baseline of the first run holding a UTF-8 byte range of one logical line.
    pub fn span_baseline(&self, line: usize, range: Range<usize>) -> Option<f32> {
        self.0.borrow().editor.span_baseline(line, range)
    }
}

impl Default for Content {
    fn default() -> Self {
        Self::with_text("")
    }
}

impl Internal {
    /// Lay out `composition` over the text, reusing the last layout of it.
    fn compose(&mut self, composition: Option<Composition>) {
        let composition = composition.filter(|c| !c.content.is_empty());
        if self.preedit.as_ref().map(|(c, _)| c) != composition.as_ref() {
            self.preedit = composition.map(|c| {
                let editor = self.editor.with_preedit(&c.content, c.selection.clone());
                (c, editor)
            });
        }
    }
    /// The editor to show: with the composition inserted, while composing.
    fn shown(&self) -> &Editor {
        self.preedit
            .as_ref()
            .map_or(&self.editor, |(_, editor)| editor)
    }
}
