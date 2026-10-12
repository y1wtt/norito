//! Native adapters. Paths and platform-specific details stay inside this crate.
//!
//! `NativePlatform` owns the handle registry; each module implements one port:
//! `files` (FileStore), `directories` (DirectoryStore), `dialogs` (FileDialog,
//! ResourcePicker), `paths` (Paths), `resources` (DocumentResources) and
//! `shell` (ExternalOpener).
mod dialogs;
mod directories;
mod files;
mod paths;
mod resources;
mod shell;

use norito_platform::*;
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

/// Opaque handles index this registry; the same path always gets the same handle.
#[derive(Default)]
pub struct NativePlatform {
    paths: Mutex<Vec<PathBuf>>,
}

impl NativePlatform {
    fn register(&self, path: PathBuf) -> u64 {
        let mut paths = self.paths.lock().unwrap();
        if let Some(index) = paths.iter().position(|p| p == &path) {
            return index as u64;
        }
        paths.push(path);
        (paths.len() - 1) as u64
    }
    fn path(&self, id: u64) -> Result<PathBuf> {
        self.paths
            .lock()
            .unwrap()
            .get(id as usize)
            .cloned()
            .ok_or(PlatformError::new(
                ErrorKind::NotFound,
                "ファイル参照が無効です。",
            ))
    }
    fn file_ref(&self, path: PathBuf) -> FileRef {
        FileRef {
            name: name(&path),
            handle: FileHandle(self.register(path)),
        }
    }
    fn directory_ref(&self, path: PathBuf) -> DirectoryRef {
        DirectoryRef {
            name: name(&path),
            location: location(&path),
            handle: DirectoryHandle(self.register(path)),
        }
    }
    /// Directory containing the document `file`.
    fn parent(&self, file: FileHandle) -> Result<PathBuf> {
        Ok(self.path(file.0)?.parent().ok_or_else(conflict)?.to_owned())
    }
}

fn error(e: std::io::Error) -> PlatformError {
    let kind = match e.kind() {
        std::io::ErrorKind::NotFound => ErrorKind::NotFound,
        std::io::ErrorKind::PermissionDenied => ErrorKind::PermissionDenied,
        _ => ErrorKind::Io,
    };
    PlatformError::new(kind, e.to_string())
}
fn conflict() -> PlatformError {
    PlatformError::new(
        ErrorKind::Conflict,
        "ファイルが外部で変更されました。別名で保存するか、開き直してください。",
    )
}
/// `AlreadyExists` becomes a conflict with `message`; other errors map as usual.
fn exists(e: std::io::Error, message: &str) -> PlatformError {
    if e.kind() == std::io::ErrorKind::AlreadyExists {
        PlatformError::new(ErrorKind::Conflict, message)
    } else {
        error(e)
    }
}
/// Display text without the verbatim prefix added by Windows canonicalization.
fn location(path: &Path) -> String {
    let path = path.to_string_lossy();
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else {
        path.strip_prefix(r"\\?\").unwrap_or(&path).to_owned()
    }
}
fn name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}
fn stem(path: &Path) -> String {
    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}
fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}
