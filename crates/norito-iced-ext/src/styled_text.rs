//! Generic rich-text spans on Iced's continuous text editor.
use iced::{
    Font,
    advanced::text::{Highlighter, highlighter::Format},
};
use std::{ops::Range, sync::Arc};

pub type Span = (Range<usize>, Format<Font>);
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Styles(pub Arc<Vec<Vec<Span>>>);
pub struct StyledText {
    settings: Styles,
    line: usize,
}
impl Highlighter for StyledText {
    type Settings = Styles;
    type Highlight = Format<Font>;
    type Iterator<'a> = std::vec::IntoIter<Span>;
    fn new(settings: &Styles) -> Self {
        Self {
            settings: settings.clone(),
            line: 0,
        }
    }
    fn update(&mut self, settings: &Styles) {
        let unchanged = self
            .settings
            .0
            .iter()
            .zip(settings.0.iter())
            .take_while(|(old, new)| old == new)
            .count();
        self.settings = settings.clone();
        self.line = self.line.min(unchanged);
    }
    fn change_line(&mut self, line: usize) {
        self.line = self.line.min(line);
    }
    fn highlight_line(&mut self, _line: &str) -> Self::Iterator<'_> {
        let spans = self.settings.0.get(self.line).cloned().unwrap_or_default();
        self.line += 1;
        spans.into_iter()
    }
    fn current_line(&self) -> usize {
        self.line
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn word_and_line_cursor_ranges_match_selected_text() {
        let mut editor = Editor::with_text("first second\nnext");
        let mut highlighter = StyledText::new(&Styles::default());
        editor.update(
            iced::Size::new(400.0, 200.0),
            Font::DEFAULT,
            17.0.into(),
            LineHeight::Relative(1.5),
            Wrapping::WordOrGlyph,
            &mut highlighter,
        );
        editor.move_to(Cursor {
            position: Position { line: 0, column: 8 },
            selection: None,
        });
        editor.perform(Action::SelectWord);
        let cursor = editor.cursor();
        assert_eq!(cursor.selection, Some(Position { line: 0, column: 6 }));
        assert_eq!(
            cursor.position,
            Position {
                line: 0,
                column: 12
            }
        );
        editor.perform(Action::SelectLine);
        let cursor = editor.cursor();
        assert_eq!(cursor.selection, Some(Position { line: 0, column: 0 }));
        assert_eq!(
            cursor.position,
            Position {
                line: 0,
                column: 12
            }
        );
    }
    #[test]
    fn block_metrics_are_ready_before_decoration_layout() {
        use iced::advanced::{layout, widget::Tree};
        let content = iced::widget::text_editor::Content::<iced_tiny_skia::Renderer>::with_text(
            " \nAfter diagram",
        );
        let settings = Styles(Arc::new(vec![
            vec![(
                0..1,
                Format {
                    line_height: Some(180.0.into()),
                    ..Default::default()
                },
            )],
            vec![],
        ]));
        let widget =
            iced::widget::text_editor::<(), iced::Theme, iced_tiny_skia::Renderer>(&content)
                .layout_theme(iced::Theme::Light)
                .highlight_with::<StyledText>(settings, |format, _| *format)
                .size(15)
                .height(400);
        let mut element: iced::Element<'_, (), iced::Theme, iced_tiny_skia::Renderer> =
            widget.into();
        let mut tree = Tree::new(&element);
        let renderer = iced_tiny_skia::Renderer::new(Font::DEFAULT, 15.0.into());
        let _ = element.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(iced::Size::ZERO, iced::Size::new(500.0, 400.0)),
        );
        assert!(content.line_bounds(1).unwrap().y >= 180.0);
    }
    #[test]
    fn editor_ink_stays_inside_viewport_on_full_and_partial_redraws() {
        use iced::advanced::text::Renderer as _;
        use iced::{Color, Point, Rectangle, Size};
        let mut editor = Editor::with_text("Heading\nSecond heading\nThird heading");
        let mut highlighter = StyledText::new(&Styles::default());
        let bounds = Rectangle::new(Point::new(20.0, 20.0), Size::new(200.0, 28.0));
        editor.update(
            bounds.size(),
            Font::DEFAULT,
            24.0.into(),
            LineHeight::Relative(1.3),
            Wrapping::WordOrGlyph,
            &mut highlighter,
        );
        let mut snapshots = Vec::new();
        for damage in [
            Rectangle::new(Point::ORIGIN, Size::new(250.0, 100.0)),
            bounds,
        ] {
            let mut renderer = iced_tiny_skia::Renderer::new(Font::DEFAULT, 24.0.into());
            renderer.fill_editor(&editor, bounds.position(), Color::BLACK, bounds);
            // A relayout can occur after a render group captures this editor.
            // It must not turn that frame into an empty paragraph.
            editor.move_to(Cursor {
                position: Position { line: 0, column: 0 },
                selection: None,
            });
            let mut pixmap = tiny_skia::Pixmap::new(250, 100).unwrap();
            pixmap.fill(tiny_skia::Color::WHITE);
            let mut mask = tiny_skia::Mask::new(250, 100).unwrap();
            renderer.draw(
                &mut pixmap.as_mut(),
                &mut mask,
                &iced::advanced::graphics::Viewport::with_physical_size(Size::new(250, 100), 1.0),
                &[damage],
                Color::WHITE,
            );
            assert!(
                pixmap.pixels().iter().any(|p| p.red() != 255),
                "test must render ink"
            );
            for y in 0..100 {
                for x in 0..250 {
                    if !bounds.contains(Point::new(x as f32 + 0.5, y as f32 + 0.5)) {
                        assert_eq!(
                            pixmap.pixel(x, y).unwrap().red(),
                            255,
                            "ink leaked at {x},{y}"
                        );
                    }
                }
            }
            snapshots.push(pixmap.data().to_vec());
        }
        assert_eq!(
            snapshots[0], snapshots[1],
            "damage rectangles must not change the output"
        );
    }
    #[test]
    fn aligned_and_overflowing_text_stays_inside_its_clip() {
        use iced::advanced::text::{self, Renderer as _};
        use iced::{Color, Point, Rectangle, Size};
        // Cells partially scrolled above the viewport, as in a table decoration.
        let viewport = Rectangle::new(Point::new(0.0, 20.0), Size::new(250.0, 80.0));
        let cells = [
            Rectangle::new(Point::new(30.0, 8.0), Size::new(90.0, 34.0)),
            Rectangle::new(Point::new(120.0, 8.0), Size::new(90.0, 34.0)),
        ];
        let draw = |renderer: &mut iced_tiny_skia::Renderer| {
            for (cell, align_x, content) in [
                (cells[0], text::Alignment::Center, "Center"),
                (cells[1], text::Alignment::Left, "Overflowing cell text"),
            ] {
                let inner = cell.shrink(7.0);
                renderer.fill_text(
                    text::Text {
                        content: content.into(),
                        bounds: inner.size(),
                        size: 14.0.into(),
                        line_height: text::LineHeight::Relative(1.4),
                        font: Font::DEFAULT,
                        align_x,
                        align_y: iced::alignment::Vertical::Top,
                        shaping: text::Shaping::Advanced,
                        wrapping: text::Wrapping::None,
                    },
                    // A centered anchor at the left edge must not leak either.
                    Point::new(inner.x, inner.y),
                    Color::BLACK,
                    cell.intersection(&viewport).unwrap(),
                );
            }
        };
        let mut snapshots = Vec::new();
        for damage in [
            Rectangle::new(Point::ORIGIN, Size::new(250.0, 100.0)),
            viewport,
        ] {
            let mut renderer = iced_tiny_skia::Renderer::new(Font::DEFAULT, 14.0.into());
            draw(&mut renderer);
            let mut pixmap = tiny_skia::Pixmap::new(250, 100).unwrap();
            pixmap.fill(tiny_skia::Color::WHITE);
            let mut mask = tiny_skia::Mask::new(250, 100).unwrap();
            renderer.draw(
                &mut pixmap.as_mut(),
                &mut mask,
                &iced::advanced::graphics::Viewport::with_physical_size(Size::new(250, 100), 1.0),
                &[damage],
                Color::WHITE,
            );
            assert!(
                pixmap.pixels().iter().any(|p| p.red() != 255),
                "test must render ink"
            );
            for y in 0..100 {
                for x in 0..250 {
                    let point = Point::new(x as f32 + 0.5, y as f32 + 0.5);
                    if !cells
                        .iter()
                        .any(|cell| cell.contains(point) && viewport.contains(point))
                    {
                        assert_eq!(
                            pixmap.pixel(x, y).unwrap().red(),
                            255,
                            "ink leaked at {x},{y}"
                        );
                    }
                }
            }
            snapshots.push(pixmap.data().to_vec());
        }
        assert_eq!(
            snapshots[0], snapshots[1],
            "damage rectangles must not change the output"
        );
    }
    #[test]
    fn changed_styles_restart_at_first_changed_line() {
        let first = Styles(Arc::new(vec![vec![], vec![], vec![]]));
        let mut h = StyledText::new(&first);
        for _ in 0..3 {
            let _ = h.highlight_line("");
        }
        let mut lines = (*first.0).clone();
        lines[2].push((
            0..1,
            Format {
                size: Some(20.0.into()),
                ..Default::default()
            },
        ));
        h.update(&Styles(Arc::new(lines)));
        assert_eq!(h.current_line(), 2);
        h.change_line(1);
        assert_eq!(h.current_line(), 1);
    }

    #[test]
    fn oblique_font_sets_synthetic_slant_for_japanese_glyphs() {
        let attrs = iced::advanced::graphics::text::to_attributes(Font {
            family: iced::font::Family::Name("Yu Gothic UI"),
            style: iced::font::Style::Oblique,
            ..Font::DEFAULT
        });
        assert!(
            attrs
                .cache_key_flags
                .contains(iced::advanced::graphics::text::cosmic_text::CacheKeyFlags::FAKE_ITALIC)
        );
        assert_eq!(
            attrs.style,
            iced::advanced::graphics::text::cosmic_text::Style::Normal
        );
    }
    use iced::advanced::{
        graphics::text::Editor,
        text::{
            Editor as _, LineHeight, Wrapping,
            editor::{Action, Cursor, Position, Selection},
        },
    };
    #[test]
    fn explicit_jump_reveals_offscreen_line_and_clears_selection() {
        let text = (0..100)
            .map(|i| format!("Line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut editor = Editor::with_text(&text);
        let mut highlighter = StyledText::new(&Styles::default());
        editor.update(
            iced::Size::new(500.0, 100.0),
            Font::DEFAULT,
            15.0.into(),
            LineHeight::Relative(1.8),
            Wrapping::WordOrGlyph,
            &mut highlighter,
        );
        editor.move_to(Cursor {
            position: Position { line: 0, column: 4 },
            selection: Some(Position { line: 0, column: 0 }),
        });
        editor.move_to(Cursor {
            position: Position {
                line: 90,
                column: 0,
            },
            selection: None,
        });
        assert!(editor.copy().is_none());
        assert!(editor.buffer().scroll().line > 0);
        let caret = editor.caret_bounds().unwrap();
        assert!(caret.y >= 0.0 && caret.y < 100.0);
    }
    #[test]
    fn heading_metrics_are_ready_before_outline_jump() {
        let source = (0..60).map(|_| "Heading").collect::<Vec<_>>().join("\n");
        let styles = Styles(Arc::new(
            (0..60)
                .map(|_| {
                    vec![(
                        0..7,
                        Format {
                            size: Some(24.0.into()),
                            line_height: Some(31.2.into()),
                            ..Default::default()
                        },
                    )]
                })
                .collect(),
        ));
        let mut editor = Editor::with_text(&source);
        let mut highlighter = StyledText::new(&styles);
        editor.update(
            iced::Size::new(500.0, 100.0),
            Font::DEFAULT,
            15.0.into(),
            LineHeight::Relative(1.8),
            Wrapping::WordOrGlyph,
            &mut highlighter,
        );
        editor.highlight(Font::DEFAULT, &mut highlighter, |format| *format);
        editor.move_to(Cursor {
            position: Position {
                line: 50,
                column: 0,
            },
            selection: None,
        });
        assert!((editor.caret_height().unwrap() - 31.2).abs() < 0.01);
    }
    #[test]
    fn empty_ime_commit_keeps_selection_while_nonempty_commit_replaces_it() {
        let mut editor = Editor::with_text("元の本文です");
        editor.move_to(Cursor {
            position: Position {
                line: 0,
                column: "元の本文です".len(),
            },
            selection: Some(Position { line: 0, column: 0 }),
        });
        if let Some(action) = Action::from_ime_commit(String::new()) {
            editor.perform(action);
        }
        assert_eq!(editor.copy().as_deref(), Some("元の本文です"));
        editor.perform(Action::from_ime_commit("あ".into()).unwrap());
        assert_eq!(&*editor.line(0).unwrap().text, "あ");
    }
    #[test]
    fn inline_preedit_matches_commit_geometry_without_mutating_original() {
        for (source, column, width, size) in [
            ("見出し", 0, 500.0, 24.0),
            ("前後の本文", 3, 110.0, 15.0),
            ("", 0, 500.0, 15.0),
        ] {
            let mut editor = Editor::with_text(source);
            let settings = Styles(Arc::new(vec![vec![(
                0..source.len(),
                Format {
                    size: Some(size.into()),
                    line_height: Some((size * 1.8).into()),
                    ..Default::default()
                },
            )]]));
            let mut highlighter = StyledText::new(&settings);
            editor.update(
                iced::Size::new(width, 300.0),
                Font::DEFAULT,
                15.0.into(),
                LineHeight::Relative(1.8),
                Wrapping::WordOrGlyph,
                &mut highlighter,
            );
            editor.highlight(Font::DEFAULT, &mut highlighter, |format| *format);
            editor.move_to(Cursor {
                position: Position { line: 0, column },
                selection: None,
            });
            let composing = "日本語入力";
            let preview = editor
                .with_preedit(composing, Some(composing.len()..composing.len()))
                .unwrap();
            assert_eq!(&*editor.line(0).unwrap().text, source);
            assert!(!preview.preedit_rectangles().is_empty());
            let expected = preview.caret_bounds().unwrap();
            editor.perform(Action::Edit(iced::advanced::text::editor::Edit::Paste(
                Arc::new(composing.into()),
            )));
            editor.update(
                iced::Size::new(width, 300.0),
                Font::DEFAULT,
                15.0.into(),
                LineHeight::Relative(1.8),
                Wrapping::WordOrGlyph,
                &mut highlighter,
            );
            assert_eq!(editor.caret_bounds().unwrap(), expected, "{source}");
        }
    }

    #[test]
    fn preedit_selection_replaces_text_and_keeps_original_selection() {
        let mut editor = Editor::with_text("before selected after");
        editor.update(
            iced::Size::new(500.0, 200.0),
            Font::DEFAULT,
            15.0.into(),
            LineHeight::Relative(1.8),
            Wrapping::WordOrGlyph,
            &mut iced::advanced::text::highlighter::PlainText,
        );
        editor.move_to(Cursor {
            position: Position {
                line: 0,
                column: 15,
            },
            selection: Some(Position { line: 0, column: 7 }),
        });
        let preview = editor.with_preedit("日本語", Some(0..6)).unwrap();
        assert_eq!(&*preview.line(0).unwrap().text, "before 日本語 after");
        assert_eq!(editor.copy().as_deref(), Some("selected"));
        assert_eq!(preview.copy().as_deref(), Some("日本"));
        drop(preview); // cancel composition: no edit or history entry
        assert_eq!(&*editor.line(0).unwrap().text, "before selected after");
    }
    #[test]
    fn composition_on_a_paragraph_gap_uses_body_metrics() {
        let mut editor = Editor::with_text("\nnext paragraph");
        let settings = Styles(Arc::new(vec![
            vec![(
                0..0,
                Format {
                    size: Some(8.0.into()),
                    line_height: Some(16.0.into()),
                    ..Default::default()
                },
            )],
            vec![],
        ]));
        let mut highlighter = StyledText::new(&settings);
        editor.update(
            iced::Size::new(500.0, 300.0),
            Font::DEFAULT,
            15.0.into(),
            LineHeight::Relative(1.8),
            Wrapping::WordOrGlyph,
            &mut highlighter,
        );
        editor.highlight(Font::DEFAULT, &mut highlighter, |format| *format);
        assert_eq!(editor.caret_height(), Some(16.0));
        let preview = editor.with_preedit("あ", Some(3..3)).unwrap();
        assert_eq!(preview.caret_height(), Some(27.0));
        assert_eq!(&*editor.line(0).unwrap().text, "");
    }
    #[test]
    fn composition_on_empty_code_line_preserves_monospace_metrics() {
        let mut editor = Editor::with_text("\ncode");
        let settings = Styles(Arc::new(vec![
            vec![(
                0..0,
                Format {
                    font: Some(Font::MONOSPACE),
                    size: Some(14.0.into()),
                    line_height: Some(23.0.into()),
                    ..Default::default()
                },
            )],
            vec![],
        ]));
        let mut highlighter = StyledText::new(&settings);
        editor.update(
            iced::Size::new(500.0, 300.0),
            Font::DEFAULT,
            15.0.into(),
            LineHeight::Relative(1.8),
            Wrapping::WordOrGlyph,
            &mut highlighter,
        );
        editor.highlight(Font::DEFAULT, &mut highlighter, |format| *format);
        let preview = editor.with_preedit("あ", Some(3..3)).unwrap();
        assert_eq!(preview.caret_height(), Some(23.0));
    }
    #[test]
    fn rich_metrics_keep_caret_hit_testing_and_selection_aligned() {
        let mut editor = Editor::with_text("Heading\nbody text");
        let settings = Styles(Arc::new(vec![
            vec![(
                0..7,
                Format {
                    size: Some(32.0.into()),
                    ..Default::default()
                },
            )],
            vec![],
        ]));
        let mut highlighter = StyledText::new(&settings);
        editor.update(
            iced::Size::new(500.0, 300.0),
            Font::DEFAULT,
            17.0.into(),
            LineHeight::Relative(1.5),
            Wrapping::WordOrGlyph,
            &mut highlighter,
        );
        editor.highlight(Font::DEFAULT, &mut highlighter, |format| *format);
        editor.move_to(Cursor {
            position: Position { line: 1, column: 0 },
            selection: None,
        });
        let Selection::Caret(point) = editor.selection() else {
            panic!("caret expected");
        };
        assert!(
            point.y >= 47.0,
            "body must follow the enlarged heading: {point:?}"
        );
        editor.perform(Action::Click(iced::Point::new(1.0, point.y + 8.0)));
        assert_eq!(editor.cursor().position.line, 1);
        editor.move_to(Cursor {
            position: Position { line: 1, column: 4 },
            selection: Some(Position { line: 0, column: 0 }),
        });
        let Selection::Range(rects) = editor.selection() else {
            panic!("selection expected");
        };
        assert!(rects[0].height >= 47.0);
        assert!(rects[1].y >= 47.0);
    }
}
