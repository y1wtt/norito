//! Document features around the editor, each in its own module:
//! `search` (find / replace), `links` (following links and anchors),
//! `insert` (images, page links, code and tables), `media` (rendered
//! diagrams and loaded images) and `outline` (the navigation panel).
mod insert;
mod links;
mod media;
mod outline;
mod search;

use crate::{App, Message};
use iced::{Task, widget};
use norito_editor_widget::Media;
use norito_model::structure::{Diagram, Structure};
use norito_platform::{FileRef, ResourceLink, Result};
use std::collections::HashSet;

pub use insert::Placement;
pub use search::Search;

#[derive(Debug, Clone, Copy)]
pub enum Kind {
    Table,
    Code,
    Mermaid,
}
#[derive(Debug, Clone)]
pub enum Event {
    Find,
    FindClose,
    Query(String),
    Replacement(String),
    FindNext(bool),
    Replace(bool),
    MatchCase(bool),
    Regex(bool),
    Open(Kind),
    ToggleOutline,
    Jump(usize),
    Follow(String),
    ExternalOpened(Result<()>),
    Resource(bool),
    ResourceReady(Result<Option<ResourceLink>>, Placement),
    PageReady(Result<FileRef>, String, u64),
    Diagram(
        Diagram,
        std::result::Result<norito_render::Rendering, String>,
    ),
    Image(String, Result<Vec<u8>>, u64),
}
#[derive(Default)]
pub struct Features {
    pub search: Option<Search>,
    pub outline: bool,
    /// Parsed from the current source on every change.
    pub structure: Structure,
    pub media: Media,
    rendering: HashSet<Diagram>,
    /// Heading to jump to once the linked page has opened.
    pub pending_anchor: Option<String>,
    /// Document epoch the media belong to.
    epoch: u64,
}

pub(crate) fn message(event: Event) -> Message {
    Message::Feature(event)
}

impl App {
    /// Re-derive everything that depends on the source: structure, media, pending jumps.
    pub(crate) fn refresh_features(&mut self) -> Task<Message> {
        self.features.structure = Structure::parse(&self.vm.session.document.source);
        let switched = self.features.epoch != self.vm.session.epoch;
        if switched {
            self.features.epoch = self.vm.session.epoch;
            self.features.media.images.clear();
        }
        let mut tasks: Vec<_> = self
            .features
            .structure
            .diagrams()
            .collect::<Vec<_>>()
            .into_iter()
            .map(|diagram| self.render(diagram))
            .collect();
        tasks.push(self.load_images());
        if let Some(anchor) = switched
            .then(|| self.features.pending_anchor.take())
            .flatten()
        {
            if let Some(heading) = self.features.structure.heading(&anchor) {
                self.editor.jump(heading.offset);
            } else if !anchor.is_empty() {
                self.vm.status.fail("リンク先の見出しが見つかりません。");
            }
        }
        Task::batch(tasks)
    }
    pub(crate) fn feature(&mut self, event: Event) -> Task<Message> {
        if self.composing && !matches!(event, Event::Diagram(..) | Event::Image(..)) {
            return Task::none();
        }
        match event {
            Event::Find => {
                self.editor.finish_editing();
                self.features.search.get_or_insert_with(Search::default);
                widget::operation::focus("find")
            }
            Event::FindClose => {
                self.features.search = None;
                widget::operation::focus("document")
            }
            Event::Query(value) => self.edit_search(|s| s.query = value),
            Event::Replacement(value) => self.edit_search(|s| s.replacement = value),
            Event::MatchCase(value) => self.edit_search(|s| s.match_case = value),
            Event::Regex(value) => self.edit_search(|s| s.regex = value),
            Event::FindNext(backward) => self.find_next(backward),
            Event::Replace(all) => self.replace(all),
            Event::ToggleOutline => {
                self.features.outline = !self.features.outline;
                Task::none()
            }
            Event::Open(kind) => self.open_block(kind),
            Event::Jump(offset) => {
                self.editor.jump(offset);
                widget::operation::focus("document")
            }
            Event::Follow(destination) => self.follow(destination),
            Event::ExternalOpened(result) => {
                if let Err(error) = result {
                    self.vm.status.fail(error.message);
                }
                Task::none()
            }
            Event::PageReady(result, anchor, epoch) => self.page_ready(result, anchor, epoch),
            Event::Resource(image) => self.choose_resource(image),
            Event::ResourceReady(result, placement) => self.resource_ready(result, placement),
            Event::Diagram(diagram, result) => {
                self.rendered(diagram, result);
                Task::none()
            }
            Event::Image(url, result, epoch) => {
                self.loaded(url, result, epoch);
                Task::none()
            }
        }
    }
}
