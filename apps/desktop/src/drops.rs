use super::*;
use std::path::PathBuf;

impl App {
    pub(super) fn drop_attachment(&mut self, path: PathBuf) -> Task<Message> {
        if self.vm.pending.is_some() || self.composing {
            return Task::none();
        }
        self.drop_queue.push_back((path, self.vm.document_epoch));
        if self.vm.file.is_none() && !self.vm.busy() {
            // Choose the document location before copying resources beside it.
            return self.dispatch(Command::Save(true), false);
        }
        self.resume_drops()
    }

    pub(super) fn resume_drops(&mut self) -> Task<Message> {
        if self.resolving_drop || self.vm.busy() || self.vm.pending.is_some() {
            return Task::none();
        }
        let Some(file) = self.vm.file.as_ref().map(|file| file.file.handle) else {
            self.drop_queue.clear();
            return Task::none();
        };
        while let Some((path, epoch)) = self.drop_queue.pop_front() {
            if epoch != self.vm.document_epoch {
                continue;
            }
            let extension = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            let image = matches!(
                extension.as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico"
            );
            let revision = self.vm.document.revision;
            let selection = self.editor.source_selection();
            let resources = self.resources.clone();
            self.resolving_drop = true;
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || resources.import_attachment(file, path))
                        .await
                        .unwrap_or_else(|e| {
                            Err(PlatformError {
                                kind: ErrorKind::Io,
                                message: e.to_string(),
                            })
                        })
                },
                move |result| Message::DropAsset(result, image, epoch, revision, selection.clone()),
            );
        }
        Task::none()
    }
}
