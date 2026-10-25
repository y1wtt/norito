//! Opaque handles and the values exchanged with adapters.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileHandle(pub u64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DirectoryHandle(pub u64);
/// Content fingerprint used to detect external changes before saving.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileVersion(pub String);
#[derive(Debug, Clone)]
pub struct FileRef {
    pub handle: FileHandle,
    pub name: String,
}
#[derive(Debug, Clone)]
pub struct DirectoryRef {
    pub handle: DirectoryHandle,
    pub name: String,
    /// Human-readable location for display only. Never parsed or used for I/O.
    pub location: String,
}
#[derive(Debug, Clone)]
pub enum Entry {
    File(FileRef),
    Directory(DirectoryRef),
}
impl Entry {
    pub fn name(&self) -> &str {
        match self {
            Self::File(f) => &f.name,
            Self::Directory(d) => &d.name,
        }
    }
    /// Directories first, then case-insensitive by name.
    pub fn sort(entries: &mut [Entry]) {
        entries.sort_by_key(|e| (!matches!(e, Entry::Directory(_)), e.name().to_lowercase()));
    }
}
#[derive(Debug, Clone)]
pub struct ReadResult {
    pub bytes: Vec<u8>,
    pub version: FileVersion,
}
/// Where a save goes, and the version it must replace (`None`: must not exist).
#[derive(Debug, Clone)]
pub struct SaveTarget {
    pub file: FileRef,
    pub expected: Option<FileVersion>,
}
/// A document-relative link produced by an adapter.
#[derive(Debug, Clone)]
pub struct ResourceLink {
    pub label: String,
    pub destination: String,
}
#[derive(Debug, Clone)]
pub struct ClipboardFragment {
    pub text: String,
    pub markdown: String,
    pub html: String,
}
