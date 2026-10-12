//! The open document and where it is saved.
use crate::Request;
use norito_model::Document;
use norito_platform::{DirectoryHandle, FileHandle, FileVersion, SaveTarget};

pub struct Session {
    pub document: Document,
    pub file: Option<SaveTarget>,
    /// Changes whenever another document replaces this one; stale async results compare it.
    pub epoch: u64,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            document: Document::new(String::new()),
            file: None,
            epoch: 0,
        }
    }
}

impl Session {
    pub fn name(&self) -> &str {
        self.file
            .as_ref()
            .map_or("無題.md", |t| t.file.name.as_str())
    }
    pub fn handle(&self) -> Option<FileHandle> {
        self.file.as_ref().map(|t| t.file.handle)
    }
    pub fn is(&self, file: FileHandle) -> bool {
        self.handle() == Some(file)
    }
    /// Replace the document. Everything tied to the previous one becomes stale.
    pub fn replace(&mut self, document: Document, file: Option<SaveTarget>) {
        self.document = document;
        self.file = file;
        self.epoch += 1;
    }
    pub(crate) fn save_request(&self, save_as: bool, initial: Option<DirectoryHandle>) -> Request {
        Request::Save {
            target: if save_as { None } else { self.file.clone() },
            initial,
            name: self.name().into(),
            source: self.document.source.clone(),
            bytes: self.document.bytes(),
        }
    }
    /// `source` is the snapshot that was written; later edits stay dirty.
    pub(crate) fn saved(&mut self, mut target: SaveTarget, version: FileVersion, source: String) {
        target.expected = Some(version);
        self.file = Some(target);
        self.document.mark_saved(source);
    }
}
