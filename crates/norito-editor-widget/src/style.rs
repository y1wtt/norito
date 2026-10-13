//! Typography: per-line highlight spans for the displayed text, cached by input.
use crate::{MarkdownEditor, Mode, tables::ROW_HEIGHT};
use iced::{Color, Font, Theme, advanced::text::highlighter::Format, font};
use norito_iced_ext::styled_text::{Span, Styles};
use norito_model::{projection::TextStyle, text::line_index};
use std::sync::Arc;

pub(crate) const FONT: &str = "Yu Gothic UI";
const BODY: f32 = 15.0;
const BODY_LINE: f32 = 27.0;
const CODE: f32 = 14.0;
const CODE_LINE: f32 = 23.0;
const SMALL: f32 = 12.0;
const DIAGRAM_LINE: f32 = 180.0;
const IMAGE_LINE: f32 = 220.0;
const RULE_LINE: f32 = 24.0;
const DISPLAY_MATH_LINE: f32 = 90.0;
const INLINE_MATH_LINE: f32 = 27.0;
/// Code inside its background box, which spans the text column.
const CODE_INSET: f32 = 12.0;

/// Lines drawn as code: inside a background box, not folded into a diagram.
pub(crate) fn boxed_code(style: &TextStyle) -> bool {
    style.block_code && !style.diagram
}

fn heading_size(level: u8) -> f32 {
    match level {
        1 => 24.0,
        2 => 20.0,
        3 => 17.0,
        _ => 16.0,
    }
}
fn heading_font() -> Font {
    Font {
        family: font::Family::Name(FONT),
        weight: font::Weight::Semibold,
        ..Font::DEFAULT
    }
}
/// An empty span that only sets the height of its line.
fn spacer(size: f32, line_height: f32, font: Option<Font>) -> Span {
    (
        0..0,
        Format {
            size: Some(size.into()),
            line_height: Some(line_height.into()),
            font,
            ..Default::default()
        },
    )
}

/// The inputs a style computation depends on, and its result.
pub(crate) struct Cache {
    source: String,
    display: String,
    mode: Mode,
    palette: [Color; 3],
    styles: Styles,
}

impl MarkdownEditor {
    pub(crate) fn styles(&self, theme: &Theme) -> Styles {
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
        *self.style_cache.borrow_mut() = Some(Cache {
            source: self.projection.source.clone(),
            display: self.projection.text.clone(),
            mode: self.mode,
            palette,
            styles: styles.clone(),
        });
        styles
    }
    fn span_format(&self, style: TextStyle, source: usize, theme: &Theme) -> Format<Font> {
        let size = if style.heading > 0 {
            heading_size(style.heading)
        } else if style.small {
            SMALL
        } else if style.code {
            CODE
        } else {
            BODY
        };
        let font = Font {
            family: if style.code {
                font::Family::Monospace
            } else {
                font::Family::Name(FONT)
            },
            weight: if style.bold || style.heading > 0 {
                font::Weight::Semibold
            } else {
                font::Weight::Normal
            },
            style: if style.italic {
                font::Style::Oblique
            } else {
                font::Style::Normal
            },
            ..Font::DEFAULT
        };
        let background = theme.extended_palette().background;
        let color = if style.block_code {
            self.code_colors
                .get(source)
                .copied()
                .flatten()
                .unwrap_or(Color::from_rgb8(210, 216, 225))
        } else if style.heading > 0 && !style.syntax {
            background.strongest.text
        } else if style.syntax {
            background.strong.text.scale_alpha(0.6)
        } else if style.quote {
            background.strong.text
        } else if style.link {
            theme.palette().primary
        } else {
            theme.palette().text
        };
        Format {
            line_height: Some(
                if style.heading > 0 {
                    size * 1.3
                } else if style.block_code {
                    CODE_LINE
                } else {
                    BODY_LINE
                }
                .into(),
            ),
            font: Some(font),
            size: Some(size.into()),
            inset: None,
            color: Some(color),
        }
    }
    pub(crate) fn compute_styles(&self, theme: &Theme) -> Styles {
        let projection = self.shown();
        let mut lines: Vec<Vec<Span>> = vec![vec![]];
        let mut line_start = 0;
        for cell in &projection.cells {
            if projection.text[cell.display.clone()].contains('\n') {
                lines.push(vec![]);
                line_start = cell.display.end;
                continue;
            }
            let format = self.span_format(cell.style, cell.source.start, theme);
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
        // Empty lines: keep the heading size, otherwise space paragraphs.
        let kinds: Vec<_> = projection
            .line_styles
            .iter()
            .map(|style| (style.heading, style.block_code))
            .collect();
        let last = lines.len().saturating_sub(1);
        for (index, line) in lines.iter_mut().enumerate() {
            let (heading, code) = kinds[index];
            if line.is_empty() && heading > 0 {
                let size = heading_size(heading);
                line.push(spacer(size, size * 1.3, Some(heading_font())));
            }
            if line.is_empty() && index != last {
                let gap = if code {
                    CODE_LINE
                } else if kinds.get(index + 1).is_some_and(|kind| kind.0 > 0) {
                    24.0
                } else if index > 0 && kinds[index - 1].0 > 0 {
                    12.0
                } else {
                    16.0
                };
                let size = if code { CODE } else { gap / 1.8 };
                line.push(spacer(size, gap, code.then_some(Font::MONOSPACE)));
            }
        }
        if self.mode == Mode::Rich {
            self.reserve_block_lines(&mut lines);
        }
        for (spans, style) in lines.iter_mut().zip(&projection.line_styles) {
            if boxed_code(style) {
                for (_, format) in spans {
                    format.inset = Some(CODE_INSET.into());
                }
            }
        }
        Styles(Arc::new(lines))
    }
    /// Collapsed blocks occupy one line, tall enough for what is drawn over it.
    fn reserve_block_lines(&self, lines: &mut [Vec<Span>]) {
        for (line, _, _) in self.diagram_slots() {
            if let Some(spans) = lines.get_mut(line) {
                *spans = vec![spacer(BODY, DIAGRAM_LINE, None)];
            }
        }
        let structure = self.structure();
        for cell in &self.projection.cells {
            let style = cell.style;
            let height = if style.table {
                structure
                    .tables
                    .iter()
                    .find(|t| t.range.start == cell.source.start)
                    .map_or(2.0 * ROW_HEIGHT, |t| {
                        t.cells.len() as f32 * ROW_HEIGHT + 8.0
                    })
            } else if style.image {
                IMAGE_LINE
            } else if style.rule {
                RULE_LINE
            } else if style.math && style.diagram {
                DISPLAY_MATH_LINE
            } else if style.math {
                INLINE_MATH_LINE
            } else {
                continue;
            };
            let line = line_index(&self.projection.text, cell.display.start);
            if style.math && !style.diagram {
                for (_, format) in &mut lines[line] {
                    format.line_height = Some(height.into());
                }
            } else {
                lines[line] = vec![spacer(BODY, height, None)];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use norito_model::Document;
    #[test]
    fn style_cache_reuses_spans_and_invalidates_for_theme_mode_and_edits() {
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
    fn only_boxed_code_lines_are_inset() {
        let doc = Document::new("text\n\n```rust\na\n\nb\n```\n\n```mermaid\nA\n```\n".into());
        let mut editor = MarkdownEditor::new(&doc);
        for mode in [Mode::Rich, Mode::Source] {
            editor.set_mode(mode);
            let styles = editor.styles(&Theme::Dark);
            let projection = editor.shown();
            let mut inset = 0;
            for (spans, style) in styles.0.iter().zip(&projection.line_styles) {
                let expected = boxed_code(style).then(|| CODE_INSET.into());
                assert!(spans.iter().all(|(_, format)| format.inset == expected));
                inset += usize::from(expected.is_some() && !spans.is_empty());
            }
            // a, the empty line and b; source adds the fences and the Mermaid block.
            assert_eq!(inset, if mode == Mode::Rich { 3 } else { 8 }, "{mode:?}");
        }
    }
    #[test]
    fn source_heading_metrics_are_shared() {
        let doc = Document::new("# Heading".into());
        let mut editor = MarkdownEditor::new(&doc);
        let rich_size = editor.styles(&Theme::Dark).0[0][0].1.size;
        editor.set_mode(Mode::Source);
        assert_eq!(editor.styles(&Theme::Dark).0[0][0].1.size, rich_size);
    }
    #[test]
    #[ignore = "manual redraw timing"]
    fn measure_cached_styles() {
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
}
