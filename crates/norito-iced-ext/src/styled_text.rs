//! Generic rich-text spans for the rich editor, one list per logical line.
use iced::{Color, Font, Pixels};
use std::{ops::Range, sync::Arc};

/// How a span of text looks. `None` keeps the editor's own setting.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Format {
    pub color: Option<Color>,
    pub font: Option<Font>,
    pub size: Option<Pixels>,
    /// Defaults to the editor's line-height ratio at `size`.
    pub line_height: Option<Pixels>,
}

/// UTF-8 byte range within a line. An empty range at the start of a line
/// only sets the line's default typography, such as an empty heading's.
pub type Span = (Range<usize>, Format);

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Styles(pub Arc<Vec<Vec<Span>>>);
