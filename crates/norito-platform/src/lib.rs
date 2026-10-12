//! Ports: contracts only. No GUI types or OS paths cross this boundary.
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileHandle(pub u64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DirectoryHandle(pub u64);
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
}
#[derive(Debug, Clone)]
pub struct ReadResult {
    pub bytes: Vec<u8>,
    pub version: FileVersion,
}
#[derive(Debug, Clone)]
pub struct SaveTarget {
    pub file: FileRef,
    pub expected: Option<FileVersion>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    Io,
    Conflict,
    NotFound,
    PermissionDenied,
    Unsupported,
}
#[derive(Debug, Clone)]
pub struct PlatformError {
    pub kind: ErrorKind,
    pub message: String,
}
impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(f)
    }
}
impl std::error::Error for PlatformError {}
pub type Result<T> = std::result::Result<T, PlatformError>;

// Synchronous ports run on Host-owned blocking workers, never on the UI thread.
pub trait FileStore: Send + Sync {
    fn read(&self, file: FileHandle) -> Result<ReadResult>;
    fn write(&self, target: &SaveTarget, bytes: &[u8]) -> Result<FileVersion>;
}
pub trait DirectoryStore: Send + Sync {
    fn create_directory(&self, parent: DirectoryHandle, name: &str) -> Result<DirectoryRef>;
    fn list_children(&self, directory: DirectoryHandle) -> Result<Vec<Entry>>;
    /// Create an empty Markdown file. Existing files must never be overwritten.
    fn create_file(&self, directory: DirectoryHandle, name: &str)
    -> Result<(FileRef, FileVersion)>;
}

pub fn entry_name(name: &str) -> Result<String> {
    let name = name.trim();
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if name.is_empty()
        || name.starts_with('.')
        || name.ends_with('.')
        || name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
    {
        return Err(PlatformError {
            kind: ErrorKind::Unsupported,
            message: "有効な名前を入力してください。".into(),
        });
    }
    Ok(name.into())
}

pub fn markdown_file_name(name: &str) -> Result<String> {
    let name = entry_name(name)?;
    let lower = name.to_lowercase();
    Ok(if lower.ends_with(".md") || lower.ends_with(".markdown") {
        name
    } else {
        format!("{name}.md")
    })
}
pub trait FileDialog: Send + Sync {
    fn choose_open(&self) -> Result<Option<FileRef>>;
    fn choose_directory(&self) -> Result<Option<DirectoryRef>>;
    fn choose_save(
        &self,
        initial: Option<DirectoryHandle>,
        name: &str,
    ) -> Result<Option<SaveTarget>>;
}

#[derive(Debug, Clone)]
pub struct ResourceLink {
    pub label: String,
    pub destination: String,
}
/// Document-relative resources; OS paths remain in the concrete adapter.
pub trait DocumentResources: Send + Sync {
    fn open_attachment(&self, document: FileHandle, destination: &str) -> Result<()>;
    fn import_attachment(
        &self,
        document: FileHandle,
        path: std::path::PathBuf,
    ) -> Result<ResourceLink>;
    fn open_external(&self, destination: &str) -> Result<()>;
    fn choose_image(&self, document: FileHandle) -> Result<Option<ResourceLink>>;
    fn choose_page(&self, document: FileHandle) -> Result<Option<ResourceLink>>;
    fn resolve_page(&self, document: FileHandle, destination: &str) -> Result<FileRef>;
    fn image_bytes(&self, document: FileHandle, destination: &str) -> Result<Vec<u8>>;
}
