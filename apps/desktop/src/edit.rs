//! Editor events, clipboard, formatting and display mode: input that edits the document.
use crate::{App, EditAction, Message, features};
use iced::{
    Task,
    widget::{
        operation::focus,
        text_editor::{Action, Edit},
    },
};
use norito_editor_widget::{ClickTarget, EditorEvent, Formatting, Mode, TableAction};
use norito_viewmodel::Command;
use std::sync::Arc;

impl App {
    pub(crate) fn editor_event(&mut self, event: EditorEvent) -> Task<Message> {
        match event {
            EditorEvent::Copy => self.edit_menu(EditAction::Copy),
            EditorEvent::Cut => self.edit_menu(EditAction::Cut),
            EditorEvent::Paste => self.edit_menu(EditAction::Paste),
            EditorEvent::Format(format) => self.update(Message::Format(format)),
            EditorEvent::Undo => self.update(Message::Command(Command::Undo)),
            EditorEvent::Redo => self.update(Message::Command(Command::Redo)),
            EditorEvent::ContextMenu => self.update(Message::Context(true)),
            EditorEvent::EditCode(offset) => {
                self.editor.edit_code(offset);
                focus("document")
            }
            EditorEvent::InsertTable => self.feature(features::Event::Open(features::Kind::Table)),
            EditorEvent::EditTable(offset) => self.table(TableAction::Focus(offset, 0, 0)),
            EditorEvent::Table(action) => self.table(action),
            EditorEvent::FollowLink => {
                let caret = self.editor.source_selection().start;
                match self.features.structure.link_at(caret) {
                    Some(link) => self.feature(features::Event::Follow(link.destination.clone())),
                    None => Task::none(),
                }
            }
            EditorEvent::Action(action) => self.editor_action(action),
        }
    }
    fn editor_action(&mut self, action: Action) -> Task<Message> {
        self.context = None;
        if !self.vm.can_edit() {
            return Task::none();
        }
        if let Action::Click(point) = &action
            && let Some(target) = self.editor.click_target(*point)
        {
            return self.click(target);
        }
        match self.editor.perform(action) {
            Some(source) => self.dispatch(Command::Edit(source), true),
            None => Task::none(),
        }
    }
    /// Links open and tasks toggle on click; with the command key both reveal their source.
    fn click(&mut self, target: ClickTarget) -> Task<Message> {
        let source = match &target {
            ClickTarget::Link { source, .. } | ClickTarget::Task { source } => *source,
        };
        if self.modifiers.control() || self.modifiers.command() {
            self.editor.edit_code(source);
            return Task::none();
        }
        match target {
            ClickTarget::Link { destination, .. } => {
                self.feature(features::Event::Follow(destination))
            }
            ClickTarget::Task { source } => match self.editor.toggle_task_at(source) {
                Some(updated) => {
                    let task = self.dispatch(Command::Edit(updated), true);
                    self.editor.finish_editing();
                    task
                }
                None => Task::none(),
            },
        }
    }
    fn table(&mut self, action: TableAction) -> Task<Message> {
        let typing = matches!(action, TableAction::Cell(..) | TableAction::Hover(..));
        if !self.vm.can_edit() || (self.composing && !typing) {
            return Task::none();
        }
        self.context = None;
        let task = match self.editor.table_action(action) {
            Some(source) => self.dispatch(Command::Edit(source), false),
            None => Task::none(),
        };
        let focus = match self.editor.table_focus_id() {
            Some(id) if !typing => focus(id),
            _ => Task::none(),
        };
        Task::batch([task, focus])
    }
    pub(crate) fn edit_menu(&mut self, action: EditAction) -> Task<Message> {
        self.context = None;
        if !self.vm.can_edit() || self.editor.table_active() {
            return Task::none();
        }
        let task = match action {
            EditAction::Copy | EditAction::Cut => {
                #[cfg(target_os = "windows")]
                {
                    let markdown = self.editor.selected_markdown();
                    if markdown.is_empty() {
                        return Task::none();
                    }
                    let file = self.vm.session.handle();
                    let resources = self.services.resources.clone();
                    let epoch = self.vm.session.epoch;
                    let clipboard = self.services.clipboard.clone();
                    let revision = self.vm.session.document.revision;
                    let cut =
                        matches!(action, EditAction::Cut).then(|| self.editor.source_selection());
                    return crate::services::blocking(
                        move || {
                            clipboard.write_fragment(&crate::clipboard::fragment(
                                markdown,
                                file,
                                resources.as_ref(),
                            ))
                        },
                        move |result| {
                            Message::ClipboardWritten(result, cut.clone(), epoch, revision)
                        },
                    );
                }
                #[cfg(not(target_os = "windows"))]
                {
                    let selected = self.editor.selected_text();
                    if selected.is_empty() {
                        return Task::none();
                    }
                    let copy = iced::clipboard::write(selected);
                    if matches!(action, EditAction::Cut) {
                        Task::batch([copy, self.insert_text(String::new())])
                    } else {
                        copy
                    }
                }
            }
            EditAction::Paste => {
                let epoch = self.vm.session.epoch;
                let revision = self.vm.session.document.revision;
                #[cfg(target_os = "windows")]
                {
                    let clipboard = self.services.clipboard.clone();
                    crate::services::blocking(
                        move || clipboard.read_fragment(),
                        move |result| match result {
                            Ok(text) => Message::Paste(text, epoch, revision),
                            Err(error) => {
                                Message::ClipboardWritten(Err(error), None, epoch, revision)
                            }
                        },
                    )
                }
                #[cfg(not(target_os = "windows"))]
                {
                    iced::clipboard::read().map(move |text| Message::Paste(text, epoch, revision))
                }
            }
            EditAction::SelectAll => self.editor_action(Action::SelectAll),
        };
        Task::batch([task, focus("document")])
    }
    /// Clipboard text, if the document has not changed since it was requested.
    pub(crate) fn paste(
        &mut self,
        text: Option<String>,
        epoch: u64,
        revision: u64,
    ) -> Task<Message> {
        if self.composing
            || epoch != self.vm.session.epoch
            || revision != self.vm.session.document.revision
        {
            return Task::none();
        }
        match text.filter(|text| !text.is_empty()) {
            Some(text) => self.insert_text(text),
            None => Task::none(),
        }
    }
    fn insert_text(&mut self, text: String) -> Task<Message> {
        let task = self.editor_action(Action::Edit(Edit::Paste(Arc::new(text))));
        if self.mode == Mode::Rich {
            self.editor.finish_editing();
        }
        task
    }
    pub(crate) fn clipboard_written(
        &mut self,
        result: norito_platform::Result<()>,
        cut: Option<std::ops::Range<usize>>,
        epoch: u64,
        revision: u64,
    ) -> Task<Message> {
        if let Err(error) = result {
            self.vm.status.fail(error.message);
        } else if cut.is_some_and(|range| range == self.editor.source_selection())
            && !self.composing
            && epoch == self.vm.session.epoch
            && revision == self.vm.session.document.revision
        {
            return self.insert_text(String::new());
        }
        Task::none()
    }
    pub(crate) fn format(&mut self, format: Formatting) -> Task<Message> {
        self.context = None;
        if self.editor.table_active() || !self.vm.can_edit() {
            return Task::none();
        }
        match self.editor.format(format) {
            Some(source) => Task::batch([
                self.dispatch(Command::Edit(source), false),
                focus("document"),
            ]),
            None => Task::none(),
        }
    }
    pub(crate) fn set_mode(&mut self, mode: Mode) -> Task<Message> {
        self.context = None;
        if !self.vm.can_edit() {
            return Task::none();
        }
        self.mode = mode;
        self.editor.set_mode(mode);
        focus("document")
    }
}
