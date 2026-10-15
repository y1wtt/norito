//! The text editor element, with backgrounds and the blocks drawn over collapsed lines.
use crate::{
    EditorEvent, MarkdownEditor, Media, Mode, TableAction, keymap,
    style::{DISPLAY_MATH_EM, boxed_code},
};
use iced::{Color, Element, Font, Length, Size, Theme, widget::text_editor::Style};
use norito_iced_ext::{
    editor_decorations::{Decoration, Figure, decorate},
    fonts,
    rich_editor::rich_editor,
};
use norito_model::projection::Projection;
use std::ops::Range;

impl MarkdownEditor {
    pub fn view<'a>(
        &'a self,
        theme: &Theme,
        enabled: bool,
        media: &Media,
    ) -> Element<'a, EditorEvent> {
        let projection = self.shown();
        let mut editor = rich_editor(&self.content)
            .key_binding(keymap::binding)
            .id("document")
            .font(if self.mode == Mode::Source {
                Font::MONOSPACE
            } else {
                fonts::TEXT
            })
            .styles(self.styles(theme, media))
            .block_backgrounds(code_backgrounds(&projection, theme))
            .span_backgrounds(highlights(&projection))
            .size(15)
            .line_height(1.8)
            .padding(norito_iced_ext::EDITOR_PADDING)
            .height(Length::Fill)
            .style(|theme: &Theme, _| Style {
                background: theme.palette().background.into(),
                border: iced::Border::default(),
                value: theme.palette().text,
                placeholder: theme.palette().text,
                selection: theme.extended_palette().primary.weak.color,
            });
        if enabled {
            editor = editor.on_action(EditorEvent::Action);
        }
        let decorations = if self.mode == Mode::Rich {
            self.decorations(theme, media)
        } else {
            vec![]
        };
        let base = decorate(editor.into(), &self.content, decorations);
        if self.mode == Mode::Rich
            && let Some((line, child, clicks)) = self.table_inline()
        {
            norito_iced_ext::inline_editor::inline(base, &self.content, line, child, clicks)
        } else {
            base
        }
    }

    /// What replaces each collapsed line, plus strike-through, checkboxes, bullets and quote rules.
    fn decorations(&self, theme: &Theme, media: &Media) -> Vec<Decoration<EditorEvent>> {
        let structure = self.structure();
        let mut decorations = vec![];
        let mut line = 0;
        let mut line_start = 0;
        for cell in &self.projection.cells {
            if self.projection.text[cell.display.clone()].contains('\n') {
                line += 1;
                line_start = cell.display.end;
                continue;
            }
            let at = cell.source.start;
            let range = cell.display.start - line_start..cell.display.end - line_start;
            if cell.style.table
                && let Some(table) = structure.tables.iter().find(|t| t.range.start == at)
            {
                decorations.push(Decoration::Table {
                    line,
                    alignment: table.alignment.clone(),
                    cells: table
                        .cells
                        .iter()
                        .map(|r| r.iter().map(|c| Projection::new(c).text).collect())
                        .collect(),
                    on_cell: focus_events(at, &table.cells),
                });
            }
            if cell.style.image
                && let Some(link) = structure
                    .links
                    .iter()
                    .find(|l| l.image && l.range.start == at)
            {
                decorations.push(Decoration::Image {
                    line,
                    handle: media
                        .images
                        .get(&link.destination)
                        .and_then(|r| r.as_ref().ok())
                        .cloned(),
                    on_press: EditorEvent::EditCode(at),
                });
            }
            if cell.style.math
                && let Some(math) = structure.math.iter().find(|m| m.range.start == at)
            {
                let rendered = media.diagrams.get(&math.diagram());
                // Sized in ems, with the baseline below the top.
                let figure = rendered.and_then(|r| r.as_ref().ok()).and_then(|r| {
                    let extent = r.extent?;
                    Some(Figure {
                        handle: r.handle.clone(),
                        size: Size::new(extent.width, extent.height + extent.depth),
                        baseline: extent.height,
                    })
                });
                let on_press = EditorEvent::EditCode(at);
                decorations.push(if math.display {
                    Decoration::Diagram {
                        line,
                        size: figure.as_ref().map(|f| f.size * DISPLAY_MATH_EM),
                        handle: figure.map(|f| f.handle),
                        status: rendered
                            .and_then(|r| r.as_ref().err())
                            .cloned()
                            .unwrap_or_else(|| "数式を描画中…".into()),
                        on_press,
                    }
                } else {
                    Decoration::InlineDiagram {
                        line,
                        range: range.clone(),
                        figure,
                        on_press,
                    }
                });
            }
            if cell.style.rule {
                decorations.push(Decoration::Separator { line });
            }
            if let Some(checked) = cell.style.task {
                decorations.push(Decoration::Checkbox {
                    line,
                    range: range.clone(),
                    checked,
                });
            }
            if let Some(depth) = cell.style.bullet {
                decorations.push(Decoration::Bullet {
                    line,
                    range: range.clone(),
                    depth,
                });
            }
            if cell.style.strike && !cell.style.syntax {
                decorations.push(Decoration::Strike { line, range });
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
        for (line, source, diagram) in self.diagram_slots() {
            let rendered = media.diagrams.get(&diagram);
            decorations.push(Decoration::Diagram {
                line,
                status: match rendered {
                    Some(Err(error)) => format!("Mermaid: {error}\nクリックして編集"),
                    _ => "Mermaidを描画中…".into(),
                },
                handle: rendered
                    .and_then(|r| r.as_ref().ok())
                    .map(|r| r.handle.clone()),
                size: None,
                on_press: EditorEvent::EditCode(source),
            });
        }
        decorations
    }
}

/// One focus event per table cell.
pub(crate) fn focus_events(table: usize, cells: &[Vec<String>]) -> Vec<Vec<EditorEvent>> {
    cells
        .iter()
        .enumerate()
        .map(|(r, row)| {
            (0..row.len())
                .map(|c| EditorEvent::Table(TableAction::Focus(table, r, c)))
                .collect()
        })
        .collect()
}

/// One continuous rectangle behind each run of code lines.
fn code_backgrounds(projection: &Projection, theme: &Theme) -> Vec<(Range<usize>, Color, f32)> {
    let color = if theme.extended_palette().is_dark {
        norito_iced_ext::oklch(0.245, 0.018, 58.0)
    } else {
        norito_iced_ext::oklch(0.190, 0.014, 55.0)
    };
    let mut backgrounds: Vec<(Range<usize>, Color, f32)> = Vec::new();
    for (line, style) in projection.line_styles.iter().enumerate() {
        if boxed_code(style) {
            if let Some((range, _, _)) = backgrounds.last_mut()
                && range.end == line
            {
                range.end += 1;
            } else {
                backgrounds.push((line..line + 1, color, 8.0));
            }
        }
    }
    backgrounds
}

/// `==highlight==` spans, as line-relative ranges.
fn highlights(projection: &Projection) -> Vec<(usize, Range<usize>, Color)> {
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
                Color::from_rgba(0.8, 0.6, 0.15, 0.35),
            ));
        }
    }
    marks
}
