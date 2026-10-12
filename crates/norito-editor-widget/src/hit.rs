//! Screen-space queries against the laid-out editor text.
use crate::cursor::position;
use crate::{ClickTarget, MarkdownEditor, Mode};

impl MarkdownEditor {
    /// Resolve interactive glyphs before changing the cursor or revealing syntax.
    /// Hit testing the glyph bounds avoids activating links from trailing whitespace.
    pub fn click_target(&self, point: iced::Point) -> Option<ClickTarget> {
        if self.mode != Mode::Rich {
            return None;
        }
        let p = &self.projection;
        for cell in &p.cells {
            let checkbox = matches!(&p.text[cell.display.clone()], "☐" | "☑")
                && matches!(&p.source[cell.source.clone()], "[ ]" | "[x]" | "[X]");
            if !cell.style.link && !checkbox {
                continue;
            }
            let pos = position(&p.text, cell.display.start);
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
    /// Top-left of the first selected glyph run, in widget coordinates.
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
                let padding = norito_iced_ext::EDITOR_PADDING;
                return Some(rect.position() + iced::Vector::new(padding.left, padding.top));
            }
            offset += value.len();
        }
        None
    }
}
