use super::*;
use crate::{features::Placement, menu::MenuAction};
use iced::widget::text_editor::{Action as EditAction, Edit};
use norito_editor_widget::{EditorEvent, Mode};
use norito_platform::*;
use norito_viewmodel::{Action, Command};
use std::sync::Arc;

#[test]
fn drop_result_survives_typing_and_waits_for_ime() {
    let (mut app, _) = App::boot();
    insert(&mut app, "before ");
    let epoch = app.vm.session.epoch;
    let revision = app.vm.session.document.revision;
    let selection = app.editor.source_selection();
    insert(&mut app, "typed");
    app.composing = true;
    let _ = app.update(Message::DropAsset(
        Ok(ResourceLink {
            label: "attachment".into(),
            destination: "assets/test.pdf".into(),
        }),
        Placement {
            image: false,
            epoch,
            revision,
            selection,
        },
    ));
    assert_eq!(app.vm.session.document.source, "before typed");
    assert!(app.drops.deferred.is_some());
    app.composing = false;
    let result = app.drops.deferred.take().unwrap();
    let _ = app.update(*result);
    assert!(app.vm.session.document.source.contains("before typed"));
    assert!(
        app.vm
            .session
            .document
            .source
            .contains("[attachment](<assets/test.pdf>)")
    );
}

#[test]
fn directory_chevron_animates_and_stops() {
    let (mut app, _) = App::boot();
    let id = DirectoryHandle(99);
    let start = std::time::Instant::now();
    app.vm.workspace.expanded.insert(id);
    app.chevrons.motion.insert(id, (0.0, 90.0, start));
    let _ = app.update(Message::AnimateTree(
        start + std::time::Duration::from_millis(75),
    ));
    assert!(app.tree_angle(id) > 0.0 && app.tree_angle(id) < 90.0);
    let _ = app.update(Message::AnimateTree(
        start + std::time::Duration::from_millis(160),
    ));
    assert_eq!(app.tree_angle(id), 90.0);
    assert!(app.chevrons.motion.is_empty());
}

#[test]
fn dropped_images_and_attachments_insert_markdown_and_undo() {
    let (mut app, _) = App::boot();
    insert(&mut app, "本文\n");
    let original = app.vm.session.document.source.clone();
    let epoch = app.vm.session.epoch;
    let revision = app.vm.session.document.revision;
    let selection = app.editor.source_selection();
    let _ = app.update(Message::DropAsset(
        Ok(ResourceLink {
            label: "画像[1]".into(),
            destination: "assets/image.png".into(),
        }),
        Placement {
            image: true,
            epoch,
            revision,
            selection,
        },
    ));
    assert!(
        app.vm
            .session
            .document
            .source
            .contains("![画像\\[1\\]](<assets/image.png>)")
    );
    assert!(!app.features.outline);
    let revision = app.vm.session.document.revision;
    let selection = app.editor.source_selection();
    let _ = app.update(Message::DropAsset(
        Ok(ResourceLink {
            label: "資料".into(),
            destination: "assets/file.pdf".into(),
        }),
        Placement {
            image: false,
            epoch,
            revision,
            selection,
        },
    ));
    assert!(
        app.vm
            .session
            .document
            .source
            .contains("[資料](<assets/file.pdf>)")
    );
    let _ = app.dispatch(Command::Undo, false);
    assert!(!app.vm.session.document.source.contains("file.pdf"));
    let _ = app.dispatch(Command::Undo, false);
    assert_eq!(app.vm.session.document.source, original);
    let _ = app.update(Message::DropAsset(
        Ok(ResourceLink {
            label: "stale".into(),
            destination: "assets/stale.png".into(),
        }),
        Placement {
            image: true,
            epoch: epoch + 1,
            revision: app.vm.session.document.revision,
            selection: 0..0,
        },
    ));
    assert_eq!(app.vm.session.document.source, original);
}

#[test]
fn multiple_attachment_drops_queue_without_losing_paths() {
    let (mut app, _) = App::boot();
    app.drops.resolving = true;
    // A saved document avoids triggering a save dialog in this queue test.
    app.vm.session.file = Some(SaveTarget {
        file: FileRef {
            handle: FileHandle(999),
            name: "note.md".into(),
        },
        expected: None,
    });
    for name in ["one.png", "two.pdf"] {
        let _ = app.update(Message::Dropped(name.into()));
    }
    assert_eq!(app.drops.queue.len(), 2);
    assert_eq!(app.drops.queue[0].0, std::path::PathBuf::from("one.png"));
    assert_eq!(app.drops.queue[1].0, std::path::PathBuf::from("two.pdf"));
    app.vm.session.epoch += 1;
    app.drops.resolving = false;
    let _ = app.resume_drops();
    assert!(app.drops.queue.is_empty());
}

fn document_click_point(app: &App, task: bool) -> iced::Point {
    use iced::advanced::{layout, widget::Tree};
    let mut element = app
        .editor
        .view(&theme::theme(app.light), true, &app.features.media);
    let mut tree = Tree::new(&element);
    let renderer = iced::Renderer::new(iced::Font::DEFAULT, 15.0.into());
    element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(iced::Size::ZERO, iced::Size::new(700.0, 500.0)),
    );
    assert_eq!(app.editor.click_target(iced::Point::new(650.0, 12.0)), None);
    for y in (0..150).step_by(3) {
        for x in (0..300).step_by(3) {
            let point = iced::Point::new(x as f32, y as f32);
            if let Some(target) = app.editor.click_target(point)
                && matches!(target, norito_editor_widget::ClickTarget::Task { .. }) == task
            {
                return point;
            }
        }
    }
    panic!("interactive glyph was not hit");
}

#[test]
fn checkbox_click_toggles_and_ctrl_click_edits_without_toggling() {
    let (mut app, _) = App::boot();
    insert(&mut app, "- [ ] todo\n");
    app.editor.finish_editing();
    let point = document_click_point(&app, true);
    let _ = app.update(Message::Editor(EditorEvent::Action(EditAction::Click(
        point,
    ))));
    assert_eq!(app.vm.session.document.source, "- [x] todo\n");
    assert!(!app.editor.displayed().contains("[x]"));
    let point = document_click_point(&app, true);
    let _ = app.update(Message::Editor(EditorEvent::Action(EditAction::Click(
        point,
    ))));
    assert_eq!(app.vm.session.document.source, "- [ ] todo\n");
    let point = document_click_point(&app, true);
    let _ = app.update(Message::Modifiers(iced::keyboard::Modifiers::CTRL));
    let _ = app.update(Message::Editor(EditorEvent::Action(EditAction::Click(
        point,
    ))));
    assert_eq!(app.vm.session.document.source, "- [ ] todo\n");
    assert!(app.editor.displayed().contains("[ ]"));
    let _ = app.dispatch(Command::Undo, false);
    assert_eq!(app.vm.session.document.source, "- [x] todo\n");
}

#[test]
fn link_click_navigates_and_ctrl_click_reveals_source() {
    let (mut app, _) = App::boot();
    let source = "[go](#target)\n\n# target\n";
    insert(&mut app, source);
    app.editor.finish_editing();
    let point = document_click_point(&app, false);
    let _ = app.update(Message::Editor(EditorEvent::Action(EditAction::Click(
        point,
    ))));
    assert_eq!(
        app.editor.source_selection().start,
        source.find("target\n").unwrap()
    );
    assert!(!app.editor.displayed().contains("[go]"));
    let point = document_click_point(&app, false);
    let _ = app.update(Message::Modifiers(iced::keyboard::Modifiers::CTRL));
    let _ = app.update(Message::Editor(EditorEvent::Action(EditAction::Click(
        point,
    ))));
    assert_eq!(app.vm.session.document.source, source);
    assert!(app.editor.displayed().contains("[go](#target)"));
    assert!(app.editor.source_selection().start < source.find("\n").unwrap());
    app.editor.set_mode(Mode::Source);
    assert_eq!(app.editor.click_target(point), None);
}

#[test]
fn menu_hover_switches_only_after_open_and_escape_closes() {
    let (mut app, _) = App::boot();
    let _ = app.update(Message::MenuHover(1));
    assert_eq!(app.menu.open, None);
    let _ = app.update(Message::MenuToggle(0));
    let _ = app.update(Message::MenuHover(1));
    assert_eq!(app.menu.open, Some(1));
    let _ = app.update(Message::MenuHover(3));
    assert_eq!(app.menu.open, Some(3));
    let _ = app.update(Message::Menu(MenuAction::DisplayMode));
    assert!(app.menu.modes);
    assert_eq!(app.menu.open, Some(3));
    let _ = app.update(Message::Menu(MenuAction::Source));
    assert_eq!(app.mode, Mode::Source);
    assert!(!app.menu.modes);
    assert_eq!(app.menu.open, None);
    let _ = app.update(Message::MenuToggle(3));
    let _ = app.update(Message::Escape);
    assert_eq!(app.menu.open, None);
    let _ = app.update(Message::MenuToggle(0));
    let _ = app.update(Message::MenuClose);
    assert_eq!(app.menu.open, None);
}

fn insert(app: &mut App, text: &str) {
    let _ = app.update(Message::Editor(EditorEvent::Action(EditAction::Edit(
        Edit::Paste(Arc::new(text.into())),
    ))));
}

#[test]
fn table_cell_edit_preserves_alignment_and_undo() {
    let (mut app, _) = App::boot();
    let original = "| 左 | 右 |\n| :--- | ---: |\n| a | b |\n";
    insert(&mut app, original);
    app.editor.select_source(0..0);
    let _ = app.feature(features::Event::Open(features::Kind::Table));
    let _ = app.update(Message::Editor(EditorEvent::Table(
        norito_editor_widget::TableAction::Cell(1, 0, "変更".into()),
    )));

    assert!(app.vm.session.document.source.contains("| :--- | ---: |"));
    assert!(app.vm.session.document.source.contains("変更"));
    let _ = app.dispatch(Command::Undo, false);
    assert_eq!(app.vm.session.document.source, original);
}

#[test]
fn dropped_file_uses_unsaved_confirmation_and_cancel_preserves_document() {
    let (mut app, _) = App::boot();
    insert(&mut app, "保存前の本文");
    let file = FileRef {
        handle: FileHandle(42),
        name: "drop.md".into(),
    };
    let _ = app.update(Message::DropResolved(Ok(file), app.vm.session.epoch));
    assert!(matches!(app.vm.pending, Some(Action::OpenFile(_))));
    assert_eq!(app.vm.session.document.source, "保存前の本文");
    let _ = app.update(Message::Command(Command::Cancel));
    assert!(app.vm.pending.is_none());
    assert_eq!(app.vm.session.document.source, "保存前の本文");
}

#[test]
fn context_cut_paste_and_undo_use_document_history() {
    let (mut app, _) = App::boot();
    insert(&mut app, "日本語");
    let _ = app.update(Message::EditMenu(super::EditAction::SelectAll));
    let _ = app.update(Message::Context(true));
    assert_eq!(app.editor.selected_text(), "日本語");
    let _ = app.update(Message::EditMenu(super::EditAction::Cut));
    assert_eq!(app.vm.session.document.source, "");
    let _ = app.update(Message::Command(Command::Undo));
    assert_eq!(app.vm.session.document.source, "日本語");
    let _ = app.update(Message::EditMenu(super::EditAction::SelectAll));
    let epoch = app.vm.session.epoch;
    let revision = app.vm.session.document.revision;
    let _ = app.update(Message::Paste(Some(String::new()), epoch, revision));
    assert_eq!(app.vm.session.document.source, "日本語");
    let _ = app.update(Message::Paste(Some("置換".into()), epoch, revision));
    assert_eq!(app.vm.session.document.source, "置換");
    let _ = app.update(Message::Paste(Some("遅延した結果".into()), epoch, revision));
    assert_eq!(app.vm.session.document.source, "置換");
}

#[test]
fn menu_edit_actions_do_not_interrupt_composition() {
    let (mut app, _) = App::boot();
    insert(&mut app, "本文");
    let _ = app.update(Message::EditMenu(super::EditAction::SelectAll));
    let _ = app.update(Message::Composing(true));
    let _ = app.update(Message::Context(true));
    let _ = app.update(Message::EditMenu(super::EditAction::Cut));
    assert!(app.context.is_none());
    assert_eq!(app.vm.session.document.source, "本文");
}

#[test]
fn inline_table_inserts_edits_and_undoes_one_block() {
    use norito_editor_widget::TableAction as T;
    let (mut app, _) = App::boot();
    let _ = app.feature(features::Event::Open(features::Kind::Table));
    let _ = app.update(Message::Editor(EditorEvent::Table(T::Create(2, 2))));
    let _ = app.update(Message::Editor(EditorEvent::Table(T::Cell(
        1,
        0,
        "日本語 | 値".into(),
    ))));
    assert_eq!(app.features.structure.tables[0].cells[1][0], "日本語 | 値");
    let _ = app.update(Message::Editor(EditorEvent::Table(T::Column(1))));
    assert_eq!(app.features.structure.tables[0].cells[0].len(), 3);
    let _ = app.update(Message::Command(Command::Undo));
    assert_eq!(app.features.structure.tables[0].cells[0].len(), 2);
    assert!(!app.editor.table_active());
}

#[test]
fn table_tab_ime_spaces_and_escape_preserve_the_document() {
    use norito_editor_widget::TableAction as T;
    let (mut app, _) = App::boot();
    let _ = app.feature(features::Event::Open(features::Kind::Table));
    let _ = app.update(Message::Editor(EditorEvent::Table(T::Create(2, 2))));
    let _ = app.update(Message::Editor(EditorEvent::Table(T::Focus(0, 1, 1))));
    let _ = app.update(Message::Composing(true));
    let _ = app.update(Message::Editor(EditorEvent::Table(T::Move(false))));
    assert_eq!(app.editor.table_focus_id().as_deref(), Some("table-1-1"));
    let _ = app.update(Message::Editor(EditorEvent::Table(T::Cell(
        1,
        1,
        "日本語 ".into(),
    ))));
    let _ = app.update(Message::Composing(false));
    assert!(app.vm.session.document.source.contains("日本語  |"));
    let _ = app.update(Message::Editor(EditorEvent::Table(T::Move(false))));
    assert_eq!(app.features.structure.tables[0].cells.len(), 3);
    assert_eq!(app.editor.table_focus_id().as_deref(), Some("table-2-0"));
    let _ = app.update(Message::Editor(EditorEvent::Table(T::Move(true))));
    assert_eq!(app.editor.table_focus_id().as_deref(), Some("table-1-1"));
    let before = app.vm.session.document.source.clone();
    let _ = app.update(Message::Escape);
    assert!(!app.editor.table_active());
    assert_eq!(app.vm.session.document.source, before);
}

#[test]
fn table_column_alignment_and_document_replacement_clear_stale_editors() {
    use norito_editor_widget::TableAction as T;
    let (mut app, _) = App::boot();
    let _ = app.feature(features::Event::Open(features::Kind::Table));
    let _ = app.update(Message::Editor(EditorEvent::Table(T::Create(2, 2))));
    let _ = app.update(Message::Editor(EditorEvent::Table(T::Align(2))));
    assert_eq!(app.features.structure.tables[0].alignment[0], 2);
    let _ = app.dispatch(Command::Edit("別の本文".into()), false);
    assert!(!app.editor.table_active());
    let _ = app.update(Message::Editor(EditorEvent::Table(T::Cell(
        0,
        0,
        "古い入力".into(),
    ))));
    assert_eq!(app.vm.session.document.source, "別の本文");
}
#[test]
fn code_helper_replaces_formatted_selection_without_leaving_delimiters() {
    use features::{Event, Kind};
    let (mut app, _) = App::boot();
    insert(&mut app, "**本文**");
    let _ = app.update(Message::EditMenu(super::EditAction::SelectAll));
    let _ = app.feature(Event::Open(Kind::Code));

    assert_eq!(app.vm.session.document.source, "```text\n本文\n```\n");
    assert_eq!(app.features.structure.code.len(), 1);
}

#[test]
fn search_replace_all_is_one_undoable_edit() {
    use features::Event;
    let (mut app, _) = App::boot();
    insert(&mut app, "本文A 本文B");
    let _ = app.feature(Event::Find);
    let _ = app.feature(Event::Query("本文".into()));
    let _ = app.feature(Event::Replacement("文".into()));
    let _ = app.feature(Event::Replace(true));
    assert_eq!(app.vm.session.document.source, "文A 文B");
    let _ = app.update(Message::Command(Command::Undo));
    assert_eq!(app.vm.session.document.source, "本文A 本文B");
}
#[test]
fn page_anchor_waits_for_navigation_and_cancel_clears_it() {
    let (mut app, _) = App::boot();
    insert(&mut app, "# 元の文書");
    let file = FileRef {
        handle: FileHandle(42),
        name: "next.md".into(),
    };
    let _ = app.feature(features::Event::PageReady(
        Ok(file),
        "target".into(),
        app.vm.session.epoch,
    ));
    assert!(app.vm.pending.is_some());
    assert_eq!(app.features.pending_anchor.as_deref(), Some("target"));
    let _ = app.update(Message::Command(Command::Cancel));
    assert!(app.features.pending_anchor.is_none());
    assert_eq!(app.vm.session.document.source, "# 元の文書");
}
