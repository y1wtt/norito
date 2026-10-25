//! GUI-independent document state. Source text is the canonical representation.
//!
//! - `document`: bytes <-> source, saved state and history of one document.
//! - `syntax`: parser options and line-prefix recognition shared by every reader.
//! - `structure`: navigation-level facts (headings, links, code, tables, math).
//! - `projection`: source <-> editable display text, and edits expressed on it.
//! - `compose`: Markdown writers for generated blocks.
//! - `text`: encoding-level helpers with no Markdown knowledge.
pub mod clipboard;
pub mod compose;
mod document;
mod history;
pub mod html_import;
pub mod projection;
pub mod structure;
pub mod syntax;
pub mod text;

pub use document::Document;
