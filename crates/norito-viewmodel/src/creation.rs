//! The inline name prompt for a new file or folder.
use crate::Request;
use norito_platform::{DirectoryRef, Result, entry_name, markdown_file_name};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Directory,
}
#[derive(Debug, Clone)]
pub struct Creation {
    pub directory: DirectoryRef,
    pub name: String,
    pub kind: EntryKind,
}

impl Creation {
    pub fn new(directory: DirectoryRef, kind: EntryKind) -> Self {
        Self {
            directory,
            name: String::new(),
            kind,
        }
    }
    /// The create request for the typed name, or why the name is invalid.
    pub(crate) fn request(&self) -> Result<Request> {
        let directory = self.directory.clone();
        Ok(match self.kind {
            EntryKind::File => Request::Create(directory, markdown_file_name(&self.name)?),
            EntryKind::Directory => Request::CreateDirectory(directory, entry_name(&self.name)?),
        })
    }
}
