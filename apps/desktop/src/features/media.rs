//! Rendered diagrams and loaded images, requested as the source changes.
use super::{Event, message};
use crate::{
    App, Message,
    services::{blocking, off_thread},
};
use iced::{Task, widget};
use norito_editor_widget::Rendered;
use norito_model::structure::Diagram;
use norito_platform::Result;

/// Renderings kept before the cache is dropped and refilled from the current document.
const DIAGRAM_CACHE: usize = 64;

impl App {
    /// Render `diagram` once, unless it is cached or already rendering.
    pub(super) fn render(&mut self, diagram: Diagram) -> Task<Message> {
        if self.features.media.diagrams.contains_key(&diagram)
            || !self.features.rendering.insert(diagram.clone())
        {
            return Task::none();
        }
        Task::perform(
            async move {
                let input = diagram.clone();
                let result = off_thread(move || norito_render::svg(&input), Err).await;
                (diagram, result)
            },
            |(diagram, result)| message(Event::Diagram(diagram, result)),
        )
    }
    pub(super) fn rendered(
        &mut self,
        diagram: Diagram,
        result: std::result::Result<norito_render::Rendering, String>,
    ) {
        self.features.rendering.remove(&diagram);
        let diagrams = &mut self.features.media.diagrams;
        if diagrams.len() > DIAGRAM_CACHE {
            diagrams.clear();
        }
        diagrams.insert(
            diagram,
            result.map(|rendering| Rendered {
                handle: widget::svg::Handle::from_memory(rendering.svg.into_bytes()),
                extent: rendering.extent,
            }),
        );
    }
    /// Load every image the saved document links to that is not loaded yet.
    pub(super) fn load_images(&mut self) -> Task<Message> {
        let Some(file) = self.vm.session.handle() else {
            return Task::none();
        };
        let epoch = self.vm.session.epoch;
        let mut tasks = vec![];
        for link in self.features.structure.links.iter().filter(|l| l.image) {
            let images = &mut self.features.media.images;
            if images.contains_key(&link.destination) {
                continue;
            }
            let destination = link.destination.clone();
            images.insert(destination.clone(), Err("読み込み中…".into()));
            let resources = self.services.resources.clone();
            let url = destination.clone();
            tasks.push(blocking(
                move || resources.image_bytes(file, &url),
                move |result| message(Event::Image(destination.clone(), result, epoch)),
            ));
        }
        Task::batch(tasks)
    }
    pub(super) fn loaded(&mut self, url: String, result: Result<Vec<u8>>, epoch: u64) {
        if epoch == self.vm.session.epoch {
            self.features.media.images.insert(
                url,
                result
                    .map(widget::image::Handle::from_bytes)
                    .map_err(|e| e.message),
            );
        }
    }
}
