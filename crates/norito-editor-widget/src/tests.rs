//! Editing behaviour through the public widget API, as the desktop app drives it.
use super::*;
use iced::{
    Font, Theme,
    advanced::text::editor::{Cursor, Position},
};

#[test]
#[ignore = "manual continuous input timing"]
fn measure_continuous_input() {
    let source = "本文 **太字** and text.\n\n".repeat(400);
    for rebuild in [true, false] {
        let mut doc = Document::new(source.clone());
        let mut editor = MarkdownEditor::new(&doc);
        editor.jump(source.len());
        let start = std::time::Instant::now();
        for _ in 0..200 {
            let edited = editor
                .perform(text_editor::Action::Edit(text_editor::Edit::Insert('a')))
                .unwrap();
            doc.edit(edited);
            editor.synchronize(&doc, false);
            std::hint::black_box(editor.styles(&Theme::Dark, &Media::default()));
            if rebuild {
                let caret = editor.content.cursor();
                editor.content = rich_editor::Content::with_text(editor.displayed());
                editor.content.move_to(caret);
            }
        }
        assert!(doc.source.ends_with(&"a".repeat(200)));
        eprintln!(
            "200 keys, {} bytes, force rebuild={rebuild}: {:?}",
            source.len(),
            start.elapsed()
        );
    }
}
#[test]
fn format_shortcut_works_without_selection_and_in_source_mode() {
    let mut doc = Document::new(String::new());
    let mut editor = MarkdownEditor::new(&doc);
    doc.edit(editor.format(Formatting::Bold).unwrap());
    editor.synchronize(&doc, true);
    doc.edit(
        editor
            .perform(text_editor::Action::Edit(text_editor::Edit::Paste(
                Arc::new("本文".into()),
            )))
            .unwrap(),
    );
    editor.synchronize(&doc, false);
    assert_eq!(doc.source, "**本文**");
    editor.set_mode(Mode::Source);
    editor.select_source(2..8);
    doc.edit(editor.format(Formatting::Bold).unwrap());
    editor.synchronize(&doc, true);
    assert_eq!(doc.source, "本文");
    assert_eq!(editor.selected_text(), "本文");
}
#[test]
fn multiline_formatting_through_editor_preserves_trailing_newline() {
    for mode in [Mode::Rich, Mode::Source] {
        let mut doc = Document::new("一行目\n二行目\n".into());
        let mut editor = MarkdownEditor::new(&doc);
        editor.set_mode(mode);
        editor.select_source(0..doc.source.len());
        doc.edit(editor.format(Formatting::Bold).unwrap());
        editor.synchronize(&doc, true);
        assert_eq!(doc.source, "**一行目**\n**二行目**\n");
        assert!(!editor.selected_text().ends_with('\n'));
    }
}
#[test]
fn bold_over_mixed_selection_can_be_toggled_again() {
    for mode in [Mode::Rich, Mode::Source] {
        for original in ["before **bold** after", "**one**\ntwo\n"] {
            let mut doc = Document::new(original.into());
            let mut editor = MarkdownEditor::new(&doc);
            editor.set_mode(mode);
            editor.select_source(0..doc.source.len());
            doc.edit(editor.format(Formatting::Bold).unwrap());
            editor.synchronize(&doc, true);
            assert_eq!(
                doc.source,
                if original.contains('\n') {
                    "**one**\n**two**\n"
                } else {
                    "**before bold after**"
                }
            );
            doc.edit(editor.format(Formatting::Bold).unwrap());
            editor.synchronize(&doc, true);
            assert_eq!(
                doc.source,
                if original.contains('\n') {
                    "one\ntwo\n"
                } else {
                    "before bold after"
                }
            );
        }
    }
}
#[test]
fn backspace_removes_only_generated_empty_pairs() {
    let mut doc = Document::new(String::new());
    let mut editor = MarkdownEditor::new(&doc);
    for edit in [
        text_editor::Edit::Insert('*'),
        text_editor::Edit::Insert('*'),
        text_editor::Edit::Backspace,
        text_editor::Edit::Backspace,
    ] {
        let source = editor.perform(text_editor::Action::Edit(edit)).unwrap();
        doc.edit(source);
        editor.synchronize(&doc, false);
    }
    assert_eq!(doc.source, "");
    assert_eq!(editor.source_selection(), 0..0);
}
#[test]
fn generated_bold_closers_survive_ime_commit_and_are_skipped() {
    let mut doc = Document::new(String::new());
    let mut editor = MarkdownEditor::new(&doc);
    for edit in [
        text_editor::Edit::Insert('*'),
        text_editor::Edit::Insert('*'),
        text_editor::Edit::Paste(Arc::new("日本語".into())),
        text_editor::Edit::Insert('*'),
        text_editor::Edit::Insert('*'),
    ] {
        if let Some(source) = editor.perform(text_editor::Action::Edit(edit)) {
            doc.edit(source);
            editor.synchronize(&doc, false);
        }
    }
    assert_eq!(doc.source, "**日本語**");
    assert_eq!(
        editor.source_selection(),
        doc.source.len()..doc.source.len()
    );
    doc.edit(
        editor
            .perform(text_editor::Action::Edit(text_editor::Edit::Insert('外')))
            .unwrap(),
    );
    assert_eq!(doc.source, "**日本語**外");
}
#[test]
fn diagram_reveals_in_place_and_collapses_without_changing_source() {
    let source = "```mermaid\nflowchart LR; A-->B\n```\n\n本文";
    let doc = Document::new(source.into());
    let mut editor = MarkdownEditor::new(&doc);
    assert_eq!(editor.diagram_slots().len(), 1);
    assert!(!editor.displayed().contains("flowchart"));
    editor.edit_code(0);
    assert!(editor.displayed().contains("```mermaid"));
    assert_eq!(editor.diagram_slots().len(), 0);
    editor.finish_editing();
    assert_eq!(editor.diagram_slots().len(), 1);
    assert_eq!(editor.projection.source, source);
}
#[test]
fn consecutive_headings_keep_ime_commits_after_their_markers() {
    let mut doc = Document::new(String::new());
    let mut editor = MarkdownEditor::new(&doc);
    let actions = [
        text_editor::Edit::Insert('#'),
        text_editor::Edit::Insert(' '),
        text_editor::Edit::Paste(Arc::new("見出し".into())),
        text_editor::Edit::Enter,
        text_editor::Edit::Insert('#'),
        text_editor::Edit::Insert('#'),
        text_editor::Edit::Insert(' '),
        text_editor::Edit::Paste(Arc::new("見出し".into())),
        text_editor::Edit::Insert('2'),
    ];
    for action in actions {
        if let Some(source) = editor.perform(text_editor::Action::Edit(action)) {
            doc.edit(source);
            editor.synchronize(&doc, false);
        }
    }
    assert_eq!(doc.source, "# 見出し\n## 見出し2");
    assert_eq!(editor.content.text(), "見出し\n見出し2");
    assert_eq!(
        editor.content.cursor().position,
        Position {
            line: 1,
            column: 10
        }
    );
}
#[test]
fn empty_heading_composition_matches_committed_layout_at_every_level() {
    use iced::advanced::text::{LineHeight, Wrapping};
    use norito_iced_ext::rich_editor::Editor;
    fn layout(source: &str, line: usize, column: usize) -> Editor {
        let document = Document::new(source.into());
        let widget = MarkdownEditor::new(&document);
        let mut editor = Editor::with_text(&widget.projection.text);
        editor.update(
            iced::Size::new(720.0, 500.0),
            Font::with_name("Yu Gothic UI"),
            15.0.into(),
            LineHeight::Relative(1.8),
            Wrapping::WordOrGlyph,
            &widget.styles(&Theme::Dark, &Media::default()),
        );
        editor.move_to(Cursor {
            position: Position { line, column },
            selection: None,
        });
        editor
    }
    for level in 1..=6 {
        for (before, line, after) in [("", 0, ""), ("本文\n\n", 2, "\n\n後続の本文")] {
            let marker = "#".repeat(level);
            let source = format!("{before}{marker} {after}");
            let original = layout(&source, line, 0);
            let content = "日本語の見出し";
            let preview = original.with_preedit(content, Some(content.len()..content.len()));
            let committed = layout(
                &format!("{before}{marker} {content}{after}"),
                line,
                content.len(),
            );
            assert_eq!(
                preview.caret_bounds(),
                committed.caret_bounds(),
                "H{level}: {source:?}"
            );
            let geometry = |editor: &Editor| {
                editor
                    .buffer()
                    .layout_runs()
                    .map(|run| {
                        (
                            run.line_y,
                            run.line_height,
                            run.glyphs
                                .iter()
                                .map(|glyph| (glyph.font_id, glyph.font_size, glyph.x, glyph.y))
                                .collect::<Vec<_>>(),
                        )
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                geometry(&preview),
                geometry(&committed),
                "H{level}: {source:?}"
            );
        }
    }
}
#[test]
fn typing_inline_markdown_shortcuts_preserves_source() {
    for input in [
        "**日本語**",
        "*italic*",
        "[link](https://example.com)",
        "`code`",
        "> quote",
        "- list",
    ] {
        let mut doc = Document::new(String::new());
        let mut editor = MarkdownEditor::new(&doc);
        for c in input.chars() {
            if let Some(source) =
                editor.perform(text_editor::Action::Edit(text_editor::Edit::Insert(c)))
            {
                doc.edit(source);
                editor.synchronize(&doc, false);
            }
        }
        assert_eq!(doc.source, input, "typing {input}");
    }
}
#[test]
fn enter_typing_backspace_and_undo_preserve_bold() {
    let mut doc = Document::new("**太字**".into());
    let mut editor = MarkdownEditor::new(&doc);
    editor.jump(doc.source.len());
    for edit in [
        text_editor::Edit::Enter,
        text_editor::Edit::Insert('次'),
        text_editor::Edit::Enter,
        text_editor::Edit::Insert('行'),
    ] {
        doc.edit(editor.perform(text_editor::Action::Edit(edit)).unwrap());
        editor.synchronize(&doc, false);
    }
    assert_eq!(doc.source, "**太字**\n次\n行");
    doc.undo();
    editor.synchronize(&doc, true);
    assert_eq!(editor.content.text(), "太字\n次\n");
    let mut doc = Document::new("**太字**".into());
    let mut editor = MarkdownEditor::new(&doc);
    editor.jump(5);
    doc.edit(
        editor
            .perform(text_editor::Action::Edit(text_editor::Edit::Enter))
            .unwrap(),
    );
    editor.synchronize(&doc, false);
    assert_eq!(doc.source, "**太**\n**字**");
    editor.perform(text_editor::Action::Move(text_editor::Motion::Home));
    doc.edit(
        editor
            .perform(text_editor::Action::Edit(text_editor::Edit::Backspace))
            .unwrap(),
    );
    editor.synchronize(&doc, false);
    assert_eq!(doc.source, "**太字**");
}
#[test]
fn fenced_mermaid_typing_and_paste_preserve_markdown() {
    let input = "```mermaid\nflowchart LR\nA-->B\n```\n\n本文";
    for paste in [false, true] {
        let mut doc = Document::new(String::new());
        let mut editor = MarkdownEditor::new(&doc);
        let edits = if paste {
            vec![text_editor::Edit::Paste(Arc::new(input.into()))]
        } else {
            input
                .chars()
                .map(|c| {
                    if c == '\n' {
                        text_editor::Edit::Enter
                    } else {
                        text_editor::Edit::Insert(c)
                    }
                })
                .collect()
        };
        for edit in edits {
            if let Some(source) = editor.perform(text_editor::Action::Edit(edit)) {
                doc.edit(source);
            }
            editor.synchronize(&doc, false);
        }
        assert_eq!(doc.source, input, "paste={paste}");
        assert_eq!(editor.diagram_slots().len(), 1);
    }
}
#[test]
fn quote_formats_all_selected_lines_and_toggles_off() {
    let mut doc = Document::new("first\nsecond".into());
    let mut editor = MarkdownEditor::new(&doc);
    editor.perform(text_editor::Action::SelectAll);
    doc.edit(editor.format(Formatting::Quote).unwrap());
    editor.synchronize(&doc, false);
    assert_eq!(doc.source, "> first\n> second");
    assert!(editor.projection.line_styles.iter().all(|s| s.quote));
    doc.edit(editor.format(Formatting::Quote).unwrap());
    assert_eq!(doc.source, "first\nsecond");
}
#[test]
fn leaving_quote_or_list_keeps_next_paragraph_outside() {
    for initial in ["> 引用", "- item", "1. item"] {
        let mut doc = Document::new(initial.into());
        let mut editor = MarkdownEditor::new(&doc);
        editor.jump(doc.source.len());
        for edit in [
            text_editor::Edit::Enter,
            text_editor::Edit::Enter,
            text_editor::Edit::Paste(Arc::new("通常の本文".into())),
        ] {
            doc.edit(editor.perform(text_editor::Action::Edit(edit)).unwrap());
            editor.synchronize(&doc, false);
        }
        assert_eq!(doc.source, format!("{initial}\n\n通常の本文"));
        assert!(!editor.projection.line_styles.last().unwrap().quote);
    }
}
#[test]
fn tab_nests_list_items_that_keep_their_bullets() {
    use text_editor::Edit::{Enter, Insert};
    let mut doc = Document::new("- a".into());
    let mut editor = MarkdownEditor::new(&doc);
    editor.jump(doc.source.len());
    let mut edit = |editor: &mut MarkdownEditor, source: Option<String>| {
        doc.edit(source.unwrap());
        editor.synchronize(&doc, false);
        doc.source.clone()
    };
    let enter = |editor: &mut MarkdownEditor| editor.perform(text_editor::Action::Edit(Enter));
    let typed =
        |editor: &mut MarkdownEditor, c| editor.perform(text_editor::Action::Edit(Insert(c)));
    let source = enter(&mut editor);
    assert_eq!(edit(&mut editor, source), "- a\n- ");
    let source = editor.format(Formatting::Indent);
    assert_eq!(edit(&mut editor, source), "- a\n  - ");
    let depths: Vec<_> = editor
        .projection
        .cells
        .iter()
        .filter_map(|c| c.style.bullet)
        .collect();
    assert_eq!(depths, [0, 1]);
    let source = typed(&mut editor, 'b');
    assert_eq!(edit(&mut editor, source), "- a\n  - b");
    let source = enter(&mut editor);
    assert_eq!(edit(&mut editor, source), "- a\n  - b\n  - ");
    let source = editor.format(Formatting::Outdent);
    assert_eq!(edit(&mut editor, source), "- a\n  - b\n- ");
    let source = typed(&mut editor, 'c');
    assert_eq!(edit(&mut editor, source), "- a\n  - b\n- c");
    // Enter at the very start of an item adds an item above, with one marker each.
    editor.content.move_to(Cursor {
        position: Position { line: 2, column: 0 },
        selection: None,
    });
    let source = enter(&mut editor);
    assert_eq!(edit(&mut editor, source), "- a\n  - b\n- \n- c");
    assert!(editor.format(Formatting::Indent).is_some());
}
#[test]
fn repeated_character_insertion_uses_cursor_not_ambiguous_text_diff() {
    let mut doc = Document::new("**aa**aa".into());
    let mut editor = MarkdownEditor::new(&doc);
    editor.content.move_to(Cursor {
        position: Position { line: 0, column: 1 },
        selection: None,
    });
    doc.edit(
        editor
            .perform(text_editor::Action::Edit(text_editor::Edit::Insert('a')))
            .unwrap(),
    );
    editor.synchronize(&doc, false);
    assert_eq!(doc.source, "**aaa**aa");
}
#[test]
fn cross_block_selection_replacement_and_undo() {
    let mut doc = Document::new("# Heading\n\n**body** text".into());
    let mut editor = MarkdownEditor::new(&doc);
    editor.perform(text_editor::Action::SelectAll);
    doc.edit(
        editor
            .perform(text_editor::Action::Edit(text_editor::Edit::Paste(
                Arc::new("新しい本文".into()),
            )))
            .unwrap(),
    );
    editor.synchronize(&doc, false);
    assert_eq!(doc.source, "新しい本文");
    doc.undo();
    editor.synchronize(&doc, true);
    assert_eq!(editor.content.text(), "Heading\n\nbody text");
}
#[test]
fn heading_command_keeps_inline_formatting() {
    let mut doc = Document::new("**text**".into());
    let mut editor = MarkdownEditor::new(&doc);
    doc.edit(editor.format(Formatting::Heading(2)).unwrap());
    editor.synchronize(&doc, false);
    assert_eq!(doc.source, "## **text**");
}
#[test]
fn edits_rich_text_and_saves_markdown_not_plain_text() {
    let mut doc = Document::new("# Heading\n\n**日本語**".into());
    let mut editor = MarkdownEditor::new(&doc);
    assert_eq!(editor.content.text(), "Heading\n\n日本語");
    editor.content.move_to(Cursor {
        position: Position { line: 2, column: 3 },
        selection: None,
    });
    let source = editor
        .perform(text_editor::Action::Edit(text_editor::Edit::Insert('本')))
        .unwrap();
    doc.edit(source);
    editor.synchronize(&doc, false);
    assert_eq!(doc.source, "# Heading\n\n**日本本語**");
    assert_eq!(editor.content.text(), "Heading\n\n**日本本語**");
    doc.undo();
    editor.synchronize(&doc, true);
    assert_eq!(editor.content.text(), "Heading\n\n日本語");
}
#[test]
fn markdown_shortcut_becomes_rich_heading_without_losing_cursor() {
    let mut doc = Document::new(String::new());
    let mut editor = MarkdownEditor::new(&doc);
    for c in "# Title".chars() {
        let source = editor
            .perform(text_editor::Action::Edit(text_editor::Edit::Insert(c)))
            .unwrap();
        doc.edit(source);
        editor.synchronize(&doc, false);
    }
    assert_eq!(doc.source, "# Title");
    assert_eq!(editor.content.text(), "Title");
}
#[test]
fn source_switch_preserves_original_bytes() {
    let doc = Document::new("# Heading\r\n\r\n**日本語**\r\n".into());
    let mut editor = MarkdownEditor::new(&doc);
    editor.set_mode(Mode::Source);
    assert_eq!(editor.content.text(), doc.source);
    editor.set_mode(Mode::Rich);
    assert_eq!(editor.content.text(), "Heading\r\n\r\n日本語\r\n");
}
#[test]
fn mode_switch_preserves_selected_text() {
    let doc = Document::new("# Heading\n\n日本語".into());
    let mut editor = MarkdownEditor::new(&doc);
    editor.content.move_to(Cursor {
        position: Position { line: 2, column: 9 },
        selection: Some(Position { line: 2, column: 0 }),
    });
    editor.set_mode(Mode::Source);
    assert_eq!(&editor.content.text()[editor.selection()], "日本語");
    editor.set_mode(Mode::Rich);
    assert_eq!(&editor.content.text()[editor.selection()], "日本語");
}
