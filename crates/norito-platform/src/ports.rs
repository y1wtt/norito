//! One trait per OS service. Synchronous ports run on Host-owned blocking
//! workers, never on the UI thread.
use crate::*;
use std::path::PathBuf;

pub trait FileStore: Send + Sync {
    fn read(&self, file: FileHandle) -> Result<ReadResult>;
    /// Replace the file only if it still matches `target.expected`.
    fn write(&self, target: &SaveTarget, bytes: &[u8]) -> Result<FileVersion>;
}
pub trait DirectoryStore: Send + Sync {
    fn create_directory(&self, parent: DirectoryHandle, name: &str) -> Result<DirectoryRef>;
    /// Direct children only: Markdown files and directories, sorted, hidden items omitted.
    fn list_children(&self, directory: DirectoryHandle) -> Result<Vec<Entry>>;
    /// Create an empty Markdown file. Existing files must never be overwritten.
    fn create_file(&self, directory: DirectoryHandle, name: &str)
    -> Result<(FileRef, FileVersion)>;
}
/// Cancellation is `Ok(None)`, never an error.
pub trait FileDialog: Send + Sync {
    fn choose_open(&self) -> Result<Option<FileRef>>;
    fn choose_directory(&self) -> Result<Option<DirectoryRef>>;
    fn choose_save(
        &self,
        initial: Option<DirectoryHandle>,
        name: &str,
    ) -> Result<Option<SaveTarget>>;
}
/// OS paths handed over by the shell: command line and drag and drop.
pub trait Paths: Send + Sync {
    fn file(&self, path: PathBuf) -> Result<FileRef>;
    /// A file that must be Markdown; anything else is `Unsupported`.
    fn markdown_file(&self, path: PathBuf) -> Result<FileRef>;
    fn directory(&self, path: PathBuf) -> Result<DirectoryRef>;
}
/// Resources addressed relative to a document; OS paths remain in the adapter.
pub trait DocumentResources: Send + Sync {
    /// Copy a file beside the document and return a link to the copy.
    fn import_attachment(&self, document: FileHandle, path: PathBuf) -> Result<ResourceLink>;
    fn resolve_page(&self, document: FileHandle, destination: &str) -> Result<FileRef>;
    fn image_bytes(&self, document: FileHandle, destination: &str) -> Result<Vec<u8>>;
}
/// Dialogs that produce a document-relative link. Cancellation is `Ok(None)`.
pub trait ResourcePicker: Send + Sync {
    fn choose_image(&self, document: FileHandle) -> Result<Option<ResourceLink>>;
    fn choose_page(&self, document: FileHandle) -> Result<Option<ResourceLink>>;
}
/// Hand a link to the OS. Only allow-listed schemes and attachment types open.
pub trait ExternalOpener: Send + Sync {
    fn open_external(&self, destination: &str) -> Result<()>;
    fn open_attachment(&self, document: FileHandle, destination: &str) -> Result<()>;
}
