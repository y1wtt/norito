//! MVVM application state; no Iced or native adapter dependencies.
//!
//! - `protocol`: commands in, effects out, and the I/O requests between them.
//! - `session`: the open document and where it is saved.
//! - `workspace`: the folder tree and the directory that receives new entries.
//! - `creation`: the inline name prompt for a new file or folder.
//! - `status`: the one-line progress or error message.
//! - `editor`: `EditorViewModel`, which sequences the above.
mod creation;
mod editor;
mod protocol;
mod session;
mod status;
mod workspace;

pub use creation::{Creation, EntryKind};
pub use editor::EditorViewModel;
pub use protocol::*;
pub use session::Session;
pub use status::Status;
pub use workspace::Workspace;
