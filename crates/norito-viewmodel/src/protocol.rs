//! Commands in, effects out. The Host executes each `Request` and returns its
//! `Response` as `Command::Completed` with the same id.
use norito_platform::*;

/// Navigation that may discard the current document, so it passes the unsaved check.
#[derive(Debug, Clone)]
pub enum Action {
    New,
    Open,
    OpenFile(FileRef),
    Exit,
}
#[derive(Debug, Clone)]
pub enum Command {
    NewDirectory,
    NewName(String),
    CreateFile,
    CancelCreate,
    Edit(String),
    Undo,
    Redo,
    Navigate(Action),
    OpenFolder,
    Refresh,
    ToggleDirectory(DirectoryRef),
    Save(bool),
    ConfirmSave,
    ConfirmDiscard,
    Cancel,
    Completed(u64, Result<Response>),
}
#[derive(Debug, Clone)]
pub struct Effect {
    pub id: u64,
    pub request: Request,
}
#[derive(Debug, Clone)]
pub enum Request {
    CreateDirectory(DirectoryRef, String),
    Create(DirectoryRef, String),
    Open(Option<FileRef>),
    OpenFolder,
    List(DirectoryRef),
    Save {
        target: Option<SaveTarget>,
        initial: Option<DirectoryHandle>,
        name: String,
        source: String,
        bytes: Vec<u8>,
    },
    Exit,
}
/// `None` payloads mean the user cancelled a dialog.
#[derive(Debug, Clone)]
pub enum Response {
    DirectoryCreated(DirectoryRef, DirectoryRef),
    Created(DirectoryRef, FileRef, FileVersion),
    Opened(Option<(FileRef, ReadResult)>),
    Folder(Option<(DirectoryRef, Vec<Entry>)>),
    Listed(DirectoryRef, Vec<Entry>),
    Saved(Option<(SaveTarget, FileVersion, String)>),
}
