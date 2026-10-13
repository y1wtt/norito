use super::*;
use crate::styled_text::{Format, Styles};
use iced::advanced::text::editor::{Edit, Position, Selection};
use iced::advanced::text::{LineHeight, Wrapping};
use iced::{Font, Point, Size};

fn styles(lines: Vec<Vec<(Range<usize>, Format)>>) -> Styles {
    Styles(Arc::new(lines))
}
fn sized(size: f32, line_height: f32) -> Format {
    Format {
        size: Some(size.into()),
        line_height: Some(line_height.into()),
        ..Default::default()
    }
}
fn laid_out(text: &str, size: Size, text_size: f32, line_height: f32, styles: &Styles) -> Editor {
    let mut editor = Editor::with_text(text);
    editor.update(
        size,
        Font::DEFAULT,
        text_size.into(),
        LineHeight::Relative(line_height),
        Wrapping::WordOrGlyph,
        styles,
    );
    editor
}
fn at(line: usize, column: usize) -> Cursor {
    Cursor {
        position: Position { line, column },
        selection: None,
    }
}

#[test]
fn word_and_line_cursor_ranges_match_selected_text() {
    let mut editor = laid_out(
        "first second\nnext",
        Size::new(400.0, 200.0),
        17.0,
        1.5,
        &Styles::default(),
    );
    editor.move_to(at(0, 8));
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
    let content = Content::with_text(" \nAfter diagram");
    let settings = styles(vec![vec![(0..1, sized(15.0, 180.0))], vec![]]);
    let mut element: iced::Element<'_, (), iced::Theme, iced_tiny_skia::Renderer> =
        rich_editor(&content)
            .styles(settings)
            .size(15)
            .height(400)
            .into();
    let mut tree = Tree::new(&element);
    let renderer = iced_tiny_skia::Renderer::new(Font::DEFAULT, 15.0.into());
    let _ = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(500.0, 400.0)),
    );
    assert!(content.line_bounds(1).unwrap().y >= 180.0);
}

#[test]
fn changed_styles_apply_without_text_edits() {
    let text = "first\nsecond\nthird";
    let mut editor = laid_out(text, Size::new(500.0, 300.0), 15.0, 1.8, &Styles::default());
    let before = [editor.line_bounds(1), editor.line_bounds(2)];
    let mut lines = vec![vec![], vec![], vec![]];
    lines[2].push((0..5, sized(32.0, 48.0)));
    editor.update(
        Size::new(500.0, 300.0),
        Font::DEFAULT,
        15.0.into(),
        LineHeight::Relative(1.8),
        Wrapping::WordOrGlyph,
        &styles(lines),
    );
    assert_eq!(editor.line_bounds(1), before[0]);
    let third = editor.line_bounds(2).unwrap();
    assert_eq!((third.y, third.height), (before[1].unwrap().y, 48.0));
}

#[test]
fn explicit_jump_reveals_offscreen_line_and_clears_selection() {
    let text = (0..100)
        .map(|i| format!("Line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut editor = laid_out(
        &text,
        Size::new(500.0, 100.0),
        15.0,
        1.8,
        &Styles::default(),
    );
    editor.move_to(Cursor {
        position: Position { line: 0, column: 4 },
        selection: Some(Position { line: 0, column: 0 }),
    });
    editor.move_to(at(90, 0));
    assert!(editor.copy().is_none());
    assert!(editor.buffer().scroll().line > 0);
    let caret = editor.caret_bounds().unwrap();
    assert!(caret.y >= 0.0 && caret.y < 100.0);
}

#[test]
fn heading_metrics_are_ready_before_outline_jump() {
    let source = vec!["Heading"; 60].join("\n");
    let settings = styles(vec![vec![(0..7, sized(24.0, 31.2))]; 60]);
    let mut editor = laid_out(&source, Size::new(500.0, 100.0), 15.0, 1.8, &settings);
    editor.move_to(at(50, 0));
    assert!((editor.caret_height().unwrap() - 31.2).abs() < 0.01);
}

#[test]
fn inline_preedit_matches_commit_geometry_without_mutating_original() {
    for (source, column, width, size) in [
        ("見出し", 0, 500.0, 24.0),
        ("前後の本文", 3, 110.0, 15.0),
        ("", 0, 500.0, 15.0),
    ] {
        let settings = styles(vec![vec![(0..source.len(), sized(size, size * 1.8))]]);
        let mut editor = laid_out(source, Size::new(width, 300.0), 15.0, 1.8, &settings);
        editor.move_to(at(0, column));
        let composing = "日本語入力";
        let preview = editor.with_preedit(composing, Some(composing.len()..composing.len()));
        assert_eq!(editor.line(0), Some(source));
        assert!(!preview.preedit_rectangles().is_empty());
        let expected = preview.caret_bounds().unwrap();
        editor.perform(Action::Edit(Edit::Paste(Arc::new(composing.into()))));
        editor.update(
            Size::new(width, 300.0),
            Font::DEFAULT,
            15.0.into(),
            LineHeight::Relative(1.8),
            Wrapping::WordOrGlyph,
            &settings,
        );
        assert_eq!(editor.caret_bounds().unwrap(), expected, "{source}");
    }
}

#[test]
fn preedit_selection_replaces_text_and_keeps_original_selection() {
    let mut editor = laid_out(
        "before selected after",
        Size::new(500.0, 200.0),
        15.0,
        1.8,
        &Styles::default(),
    );
    editor.move_to(Cursor {
        position: Position {
            line: 0,
            column: 15,
        },
        selection: Some(Position { line: 0, column: 7 }),
    });
    let preview = editor.with_preedit("日本語", Some(0..6));
    assert_eq!(preview.line(0), Some("before 日本語 after"));
    assert_eq!(editor.copy().as_deref(), Some("selected"));
    assert_eq!(preview.copy().as_deref(), Some("日本"));
    drop(preview); // cancel composition: no edit or history entry
    assert_eq!(editor.line(0), Some("before selected after"));
}

#[test]
fn composition_on_a_paragraph_gap_uses_body_metrics() {
    let settings = styles(vec![vec![(0..0, sized(8.0, 16.0))], vec![]]);
    let editor = laid_out(
        "\nnext paragraph",
        Size::new(500.0, 300.0),
        15.0,
        1.8,
        &settings,
    );
    assert_eq!(editor.caret_height(), Some(16.0));
    let preview = editor.with_preedit("あ", Some(3..3));
    assert_eq!(preview.caret_height(), Some(27.0));
    assert_eq!(editor.line(0), Some(""));
}

#[test]
fn composition_on_empty_code_line_preserves_monospace_metrics() {
    let code = Format {
        font: Some(Font::MONOSPACE),
        ..sized(14.0, 23.0)
    };
    let settings = styles(vec![vec![(0..0, code)], vec![]]);
    let editor = laid_out("\ncode", Size::new(500.0, 300.0), 15.0, 1.8, &settings);
    let preview = editor.with_preedit("あ", Some(3..3));
    assert_eq!(preview.caret_height(), Some(23.0));
}

#[test]
fn rich_metrics_keep_caret_hit_testing_and_selection_aligned() {
    let settings = styles(vec![
        vec![(
            0..7,
            Format {
                size: Some(32.0.into()),
                ..Default::default()
            },
        )],
        vec![],
    ]);
    let mut editor = laid_out(
        "Heading\nbody text",
        Size::new(500.0, 300.0),
        17.0,
        1.5,
        &settings,
    );
    editor.move_to(at(1, 0));
    let Selection::Caret(point) = editor.selection() else {
        panic!("caret expected");
    };
    assert!(
        point.y >= 47.0,
        "body must follow the enlarged heading: {point:?}"
    );
    editor.perform(Action::Click(Point::new(1.0, point.y + 8.0)));
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

/// The widget, laid out at `bounds` with no padding on a white background.
fn widget_frame<'a>(
    content: &'a Content,
    settings: Styles,
    bounds: Rectangle,
) -> (
    iced::Element<'a, Action, iced::Theme, iced_tiny_skia::Renderer>,
    iced::advanced::widget::Tree,
    iced::advanced::layout::Node,
) {
    use iced::advanced::{layout, widget::Tree};
    let mut element: iced::Element<'_, Action, iced::Theme, iced_tiny_skia::Renderer> =
        rich_editor(content)
            .styles(settings)
            .size(24)
            .line_height(1.3)
            .padding(0.0)
            .height(bounds.height)
            .on_action(|action| action)
            .style(|_, _| iced::widget::text_editor::Style {
                background: iced::Color::WHITE.into(),
                border: iced::Border::default(),
                placeholder: iced::Color::BLACK,
                value: iced::Color::BLACK,
                selection: iced::Color::BLACK,
            })
            .into();
    let mut tree = Tree::new(&element);
    let renderer = iced_tiny_skia::Renderer::new(Font::DEFAULT, 24.0.into());
    let node = element
        .as_widget_mut()
        .layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, bounds.size()),
        )
        .move_to(bounds.position());
    (element, tree, node)
}

#[test]
fn editor_ink_stays_inside_viewport_on_full_and_partial_redraws() {
    use iced::advanced::{Layout, mouse, renderer};
    let bounds = Rectangle::new(Point::new(20.0, 20.0), Size::new(200.0, 28.0));
    let mut snapshots = Vec::new();
    for damage in [
        Rectangle::new(Point::ORIGIN, Size::new(250.0, 100.0)),
        bounds,
    ] {
        let content = Content::with_text("Heading\nSecond heading\nThird heading");
        let (element, tree, node) = widget_frame(&content, Styles::default(), bounds);
        let mut renderer = iced_tiny_skia::Renderer::new(Font::DEFAULT, 24.0.into());
        element.as_widget().draw(
            &tree,
            &mut renderer,
            &iced::Theme::Light,
            &renderer::Style::default(),
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &Rectangle::new(Point::ORIGIN, Size::new(250.0, 100.0)),
        );
        // Edits handled after drawing must not empty the frame being presented.
        let mut internal = content.0.borrow_mut();
        internal.editor.perform(Action::SelectAll);
        internal.editor.perform(Action::Edit(Edit::Delete));
        drop(internal);
        let mut pixmap = tiny_skia::Pixmap::new(250, 100).unwrap();
        pixmap.fill(tiny_skia::Color::WHITE);
        let mut mask = tiny_skia::Mask::new(250, 100).unwrap();
        renderer.draw(
            &mut pixmap.as_mut(),
            &mut mask,
            &iced::advanced::graphics::Viewport::with_physical_size(Size::new(250, 100), 1.0),
            &[damage],
            iced::Color::WHITE,
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
fn ime_cancel_keeps_selection_while_commit_replaces_it() {
    use iced::advanced::{Layout, Shell, clipboard, input_method, mouse};
    let content = Content::with_text("元の本文です");
    let bounds = Rectangle::new(Point::ORIGIN, Size::new(300.0, 100.0));
    let (mut element, mut tree, node) = widget_frame(&content, Styles::default(), bounds);
    let renderer = iced_tiny_skia::Renderer::new(Font::DEFAULT, 24.0.into());
    let mut send = |event: iced::Event| {
        let mut messages = vec![];
        element.as_widget_mut().update(
            &mut tree,
            &event,
            Layout::new(&node),
            mouse::Cursor::Available(Point::new(10.0, 10.0)),
            &renderer,
            &mut clipboard::Null,
            &mut Shell::new(&mut messages),
            &bounds,
        );
        messages
    };
    let focus = iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
    assert!(matches!(send(focus)[..], [Action::Click(_)]));
    let commit = |text: &str| iced::Event::InputMethod(input_method::Event::Commit(text.into()));
    assert!(send(commit("")).is_empty());
    let pasted = send(commit("あ"));
    assert!(matches!(&pasted[..], [Action::Edit(Edit::Paste(text))] if text.as_str() == "あ"));
}
