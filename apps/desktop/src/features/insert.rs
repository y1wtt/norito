//! Inserting blocks and links: tables, code fences, images and page links.
use super::{Event, Kind, message};
use crate::{App, Message, services::blocking};
use iced::{Task, widget};
use norito_editor_widget::Mode;
use norito_model::compose;
use norito_platform::{ResourceLink, Result};
use norito_viewmodel::Command;
use std::ops::Range;

/// Where a link chosen asynchronously goes, and the document state it was chosen for.
#[derive(Debug, Clone)]
pub struct Placement {
    pub image: bool,
    pub epoch: u64,
    pub revision: u64,
    pub selection: Range<usize>,
}

impl App {
    pub(crate) fn placement(&self, image: bool) -> Placement {
        Placement {
            image,
            epoch: self.vm.session.epoch,
            revision: self.vm.session.document.revision,
            selection: self.editor.source_selection(),
        }
    }
    /// Edit the table or code block at the caret, or insert a new one.
    pub(super) fn open_block(&mut self, kind: Kind) -> Task<Message> {
        if !self.vm.can_edit() {
            return Task::none();
        }
        self.context = None;
        self.mode = Mode::Rich;
        self.editor.set_mode(self.mode);
        if matches!(kind, Kind::Table) {
            self.editor.open_table();
            return self
                .editor
                .table_focus_id()
                .map_or_else(Task::none, widget::operation::focus);
        }
        let caret = self.editor.source_selection().start;
        if let Some(code) = self
            .features
            .structure
            .code
            .iter()
            .find(|c| c.range.contains(&caret))
        {
            self.editor.edit_code(code.range.start);
            return widget::operation::focus("document");
        }
        let language = if matches!(kind, Kind::Mermaid) {
            "mermaid"
        } else {
            "text"
        };
        let source = self.editor.insert_code(language);
        Task::batch([
            self.dispatch(Command::Edit(source), false),
            widget::operation::focus("document"),
        ])
    }
    /// Pick an image (copied beside the document) or a page to link to.
    pub(super) fn choose_resource(&mut self, image: bool) -> Task<Message> {
        if !self.vm.can_edit() {
            return Task::none();
        }
        let Some(file) = self.vm.session.handle() else {
            self.vm
                .status
                .fail("画像・ページリンクを挿入する前に文書を保存してください。");
            return Task::none();
        };
        let picker = self.services.picker.clone();
        let placement = self.placement(image);
        blocking(
            move || {
                if image {
                    picker.choose_image(file)
                } else {
                    picker.choose_page(file)
                }
            },
            move |result| message(Event::ResourceReady(result, placement.clone())),
        )
    }
    /// Insert the chosen link if the document has not changed since it was requested.
    pub(super) fn resource_ready(
        &mut self,
        result: Result<Option<ResourceLink>>,
        placement: Placement,
    ) -> Task<Message> {
        if placement.epoch != self.vm.session.epoch
            || placement.revision != self.vm.session.document.revision
        {
            return Task::none();
        }
        let link = match result {
            Ok(Some(link)) => link,
            Ok(None) => return Task::none(),
            Err(e) => {
                self.vm.status.fail(e.message);
                return Task::none();
            }
        };
        let selected = self.editor.selected_text();
        // A page link takes the selected words as its label.
        let label = if !placement.image
            && placement.selection == self.editor.source_selection()
            && !selected.is_empty()
        {
            selected
        } else {
            link.label
        };
        let markdown = compose::link(&label, &link.destination, placement.image);
        let source = self
            .editor
            .insert_markdown(placement.selection, &markdown, placement.image);
        self.features.outline = true;
        Task::batch([
            self.dispatch(Command::Edit(source), true),
            widget::operation::focus("document"),
        ])
    }
}
