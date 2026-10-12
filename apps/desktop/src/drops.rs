//! Dropped files. Markdown opens through the unsaved check; anything else is
//! copied beside the document and linked at the caret, one file at a time.
use crate::{App, Message, features, services::blocking};
use iced::Task;
use norito_platform::{FileRef, ResourceLink, Result, is_markdown};
use norito_viewmodel::{Action, Command};
use std::{collections::VecDeque, path::PathBuf};

const IMAGES: [&str; 7] = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico"];

#[derive(Default)]
pub struct Drops {
    /// One drop is being resolved or imported; later ones wait in `queue`.
    pub resolving: bool,
    /// A finished import held back until IME composition ends.
    pub deferred: Option<Box<Message>>,
    /// Attachments and the document epoch they were dropped on.
    pub queue: VecDeque<(PathBuf, u64)>,
}

fn is_image(path: &std::path::Path) -> bool {
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    IMAGES.contains(&extension.as_str())
}

impl App {
    pub(crate) fn dropped(&mut self, path: PathBuf) -> Task<Message> {
        if !is_markdown(&path.to_string_lossy()) {
            return self.drop_attachment(path);
        }
        if self.drops.resolving || self.vm.busy() || self.vm.pending.is_some() {
            return Task::none();
        }
        self.context = None;
        self.drops.resolving = true;
        let paths = self.services.paths.clone();
        let epoch = self.vm.session.epoch;
        blocking(
            move || paths.markdown_file(path),
            move |result| Message::DropResolved(result, epoch),
        )
    }
    pub(crate) fn drop_resolved(&mut self, result: Result<FileRef>, epoch: u64) -> Task<Message> {
        self.drops.resolving = false;
        if self.composing
            || self.vm.busy()
            || self.vm.pending.is_some()
            || epoch != self.vm.session.epoch
        {
            return Task::none();
        }
        match result {
            Ok(file) => self.dispatch(Command::Navigate(Action::OpenFile(file)), false),
            Err(error) => {
                self.vm.status.fail(error.message);
                Task::none()
            }
        }
    }
    pub(crate) fn drop_imported(
        &mut self,
        result: Result<ResourceLink>,
        mut placement: features::Placement,
    ) -> Task<Message> {
        if self.composing {
            self.drops.deferred = Some(Box::new(Message::DropAsset(result, placement)));
            return Task::none();
        }
        // Typing since the drop moves the insertion to the current caret.
        if placement.revision != self.vm.session.document.revision {
            placement.selection = self.editor.source_selection();
        }
        placement.revision = self.vm.session.document.revision;
        // Keep the queue locked until the current insertion has completed.
        let outline = self.features.outline;
        let task = self.feature(features::Event::ResourceReady(result.map(Some), placement));
        self.features.outline = outline;
        self.drops.resolving = false;
        Task::batch([task, self.resume_drops()])
    }
    fn drop_attachment(&mut self, path: PathBuf) -> Task<Message> {
        if self.vm.pending.is_some() || self.composing {
            return Task::none();
        }
        self.drops.queue.push_back((path, self.vm.session.epoch));
        if self.vm.session.file.is_none() && !self.vm.busy() {
            // Choose the document location before copying resources beside it.
            return self.dispatch(Command::Save(true), false);
        }
        self.resume_drops()
    }
    /// Import the next queued attachment that still belongs to the open document.
    pub(crate) fn resume_drops(&mut self) -> Task<Message> {
        if self.drops.resolving || self.vm.busy() || self.vm.pending.is_some() {
            return Task::none();
        }
        let Some(file) = self.vm.session.handle() else {
            self.drops.queue.clear();
            return Task::none();
        };
        while let Some((path, epoch)) = self.drops.queue.pop_front() {
            if epoch != self.vm.session.epoch {
                continue;
            }
            let placement = self.placement(is_image(&path));
            let resources = self.services.resources.clone();
            self.drops.resolving = true;
            return blocking(
                move || resources.import_attachment(file, path),
                move |result| Message::DropAsset(result, placement.clone()),
            );
        }
        Task::none()
    }
}
