//! Loss-preserving Markdown -> editable text projection. All positions are UTF-8 bytes.
//!
//! - `build`: decide which source bytes are hidden, replaced or styled.
//! - this module: map positions between source and display.
//! - `edit`: apply display-space typing and deletion to the source.
//! - `format`: toggle inline delimiters over a display selection.
//! - `blocks`: line-level commands (headings, lists, quotes, tasks).
mod blocks;
mod build;
mod edit;
mod format;
use std::ops::Range;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextStyle {
    pub heading: u8,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub block_code: bool,
    pub link: bool,
    pub quote: bool,
    pub strike: bool,
    pub diagram: bool,
    pub syntax: bool,
    pub table: bool,
    pub image: bool,
    pub rule: bool,
    pub math: bool,
    pub highlight: bool,
    pub small: bool,
}
/// One displayed unit and the source bytes it stands for.
#[derive(Debug, Clone)]
pub struct Cell {
    pub display: Range<usize>,
    pub source: Range<usize>,
    pub style: TextStyle,
}
/// An inline construct whose delimiters surround `inner` within `outer`.
#[derive(Debug, Clone)]
struct Wrapper {
    outer: Range<usize>,
    inner: Range<usize>,
    marker: String,
}
#[derive(Debug, Clone)]
pub struct Projection {
    pub source: String,
    pub text: String,
    pub cells: Vec<Cell>,
    /// Block formatting survives even when all characters of a line are hidden.
    pub line_styles: Vec<TextStyle>,
    wrappers: Vec<Wrapper>,
    prefixes: Vec<Range<usize>>,
    fence_ends: Vec<Range<usize>>,
    revealed: Vec<Range<usize>>,
}

impl Projection {
    /// Formatted view with every delimiter hidden.
    pub fn new(source: &str) -> Self {
        Self::build(source, None, false, false)
    }
    /// Formatted view that reveals the construct at `active` and collapses blocks elsewhere.
    pub fn editable(source: &str, active: Option<usize>) -> Self {
        Self::build(source, active, true, false)
    }
    /// Every source byte visible, with formatting styles only.
    pub fn raw(source: &str) -> Self {
        Self::build(source, None, false, true)
    }
    pub fn source_position(&self, display: usize) -> usize {
        if let Some(cell) = self
            .cells
            .iter()
            .find(|c| c.display.start < display && display < c.display.end)
        {
            return cell.source.start;
        }
        let previous = self.cells.iter().rev().find(|c| c.display.end <= display);
        let next = self.cells.iter().find(|c| c.display.start >= display);
        match (previous, next) {
            (Some(prev), Some(next)) if self.text[prev.display.clone()].ends_with('\n') => {
                if self
                    .fence_ends
                    .iter()
                    .any(|fence| fence.start == prev.source.end && fence.end == next.source.start)
                {
                    // The visual spacer represents the closing fence. Typing
                    // here belongs after that complete source line.
                    return next.source.end;
                }
                next.source.start
            }
            (Some(prev), None) if self.text[prev.display.clone()].ends_with('\n') => {
                if let Some(fence) = self
                    .fence_ends
                    .iter()
                    .find(|fence| fence.start == prev.source.end)
                {
                    return fence.end;
                }
                // A trailing empty block has no visible cell after its marker.
                // Keep typing inside that block, rather than before its prefix.
                let mut at = prev.source.end;
                while let Some(prefix) = self.prefixes.iter().find(|prefix| prefix.start == at) {
                    at = prefix.end;
                }
                at
            }
            (Some(prev), _) => prev.source.end,
            (_, Some(next)) => next.source.start,
            _ => self.source.len(),
        }
    }
    pub fn source_range(&self, display: Range<usize>) -> Range<usize> {
        self.source_position(display.start)..self.source_position(display.end)
    }
    pub fn display_position(&self, source: usize) -> usize {
        self.cells
            .iter()
            .find(|c| c.source.end > source)
            .map_or(self.text.len(), |c| c.display.start)
    }
    /// Display line of the cell standing for the block that starts at `source`.
    pub fn block_line(&self, source: usize, block: impl Fn(&TextStyle) -> bool) -> Option<usize> {
        let cell = self
            .cells
            .iter()
            .find(|c| c.source.start == source && block(&c.style))?;
        Some(crate::text::line_index(&self.text, cell.display.start))
    }
    /// Whether the source byte `at` lies inside a code block.
    pub fn in_code_block(&self, at: usize) -> bool {
        self.cells
            .iter()
            .any(|c| c.source.contains(&at) && c.style.block_code)
    }
    /// Whether any code-block cell starts within `source`.
    fn code_block_within(&self, source: &Range<usize>) -> bool {
        self.cells
            .iter()
            .any(|c| source.contains(&c.source.start) && c.style.block_code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_heading_input_stays_after_prefix_on_any_line() {
        for level in 1..=6 {
            for newline in ["\n", "\r\n"] {
                for before in [String::new(), format!("# 見出し{newline}")] {
                    for after in [String::new(), format!("{newline}本文")] {
                        let prefix = format!("{} ", "#".repeat(level));
                        let source = format!("{before}{prefix}{after}");
                        let p = Projection::new(&source);
                        let anchor = before.len() + prefix.len();
                        let display = p.display_position(anchor);
                        assert_eq!(p.source_position(display), anchor, "{source:?}");
                        assert_eq!(
                            p.replace(display..display, "見出し2"),
                            (
                                format!("{before}{prefix}見出し2{after}"),
                                anchor + "見出し2".len()
                            ),
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn nested_quotes_and_closed_fences_keep_new_input_outside_markers() {
        let p = Projection::new("> > text\n> > ");
        assert_eq!(
            p.replace(p.text.len()..p.text.len(), "次").0,
            "> > text\n> > 次"
        );
        let p = Projection::new("```rust\na\n```\n\ntext");
        let at = p.text.find('\n').unwrap() + 1;
        assert_eq!(p.replace(at..at, "次").0, "```rust\na\n```\n次\ntext");
    }
}
