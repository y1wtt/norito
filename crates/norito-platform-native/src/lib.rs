//! Native adapters. Paths and platform-specific details stay inside this crate.
use norito_platform::*;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

#[derive(Default)]
pub struct NativePlatform {
    paths: Mutex<Vec<PathBuf>>,
}

fn error(e: std::io::Error) -> PlatformError {
    let kind = match e.kind() {
        std::io::ErrorKind::NotFound => ErrorKind::NotFound,
        std::io::ErrorKind::PermissionDenied => ErrorKind::PermissionDenied,
        _ => ErrorKind::Io,
    };
    PlatformError {
        kind,
        message: e.to_string(),
    }
}
fn conflict() -> PlatformError {
    PlatformError {
        kind: ErrorKind::Conflict,
        message: "ファイルが外部で変更されました。別名で保存するか、開き直してください。".into(),
    }
}
fn version(bytes: &[u8]) -> FileVersion {
    FileVersion(blake3::hash(bytes).to_hex().to_string())
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
            .ok_or(PlatformError {
                kind: ErrorKind::NotFound,
                message: "ファイル参照が無効です。".into(),
            })
    }
    pub fn file(&self, path: PathBuf) -> Result<FileRef> {
        let path = fs::canonicalize(path).map_err(error)?;
        Ok(FileRef {
            name: name(&path),
            handle: FileHandle(self.register(path)),
        })
    }
    /// Resolve a file dropped by the desktop shell without copying or moving it.
    pub fn dropped_file(&self, path: PathBuf) -> Result<FileRef> {
        let markdown = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| {
                ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown")
            });
        if !markdown || !path.is_file() {
            return Err(PlatformError {
                kind: ErrorKind::Unsupported,
                message: "Markdownファイル（.md / .markdown）をドロップしてください。".into(),
            });
        }
        self.file(path)
    }
    pub fn directory(&self, path: PathBuf) -> Result<DirectoryRef> {
        let path = fs::canonicalize(path).map_err(error)?;
        Ok(DirectoryRef {
            name: name(&path),
            location: location(&path),
            handle: DirectoryHandle(self.register(path)),
        })
    }
    fn current_version(path: &Path) -> Result<Option<FileVersion>> {
        match fs::read(path) {
            Ok(bytes) => Ok(Some(version(&bytes))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(error(e)),
        }
    }
}

impl FileStore for NativePlatform {
    fn read(&self, file: FileHandle) -> Result<ReadResult> {
        let bytes = fs::read(self.path(file.0)?).map_err(error)?;
        Ok(ReadResult {
            version: version(&bytes),
            bytes,
        })
    }
    fn write(&self, target: &SaveTarget, bytes: &[u8]) -> Result<FileVersion> {
        let path = self.path(target.file.handle.0)?;
        if Self::current_version(&path)? != target.expected {
            return Err(conflict());
        }
        let parent = path.parent().ok_or_else(conflict)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(error)?;
        temp.write_all(bytes).map_err(error)?;
        if path.exists() {
            let metadata = fs::metadata(&path).map_err(error)?;
            if metadata.permissions().readonly() {
                return Err(PlatformError {
                    kind: ErrorKind::PermissionDenied,
                    message: "読み取り専用ファイルには保存できません。".into(),
                });
            }
            temp.as_file()
                .set_permissions(metadata.permissions())
                .map_err(error)?;
        }
        temp.as_file().sync_all().map_err(error)?;
        if Self::current_version(&path)? != target.expected {
            return Err(conflict());
        }
        if target.expected.is_none() {
            temp.persist_noclobber(&path).map_err(|e| error(e.error))?;
        } else {
            temp.persist(&path).map_err(|e| error(e.error))?;
        }
        // Directory synchronization is platform-specific, unlike file contents.
        sync_parent(parent)?;
        Ok(version(bytes))
    }
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> Result<()> {
    fs::File::open(path)
        .and_then(|f| f.sync_all())
        .map_err(error)
}
#[cfg(not(unix))]
fn sync_parent(_: &Path) -> Result<()> {
    Ok(())
}

#[cfg(windows)]
fn hidden_or_link(meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes() & (0x2 | 0x400) != 0
}
#[cfg(not(windows))]
fn hidden_or_link(meta: &fs::Metadata) -> bool {
    meta.file_type().is_symlink()
}

impl DirectoryStore for NativePlatform {
    fn create_directory(&self, parent: DirectoryHandle, name: &str) -> Result<DirectoryRef> {
        let name = entry_name(name)?;
        let path = self.path(parent.0)?.join(&name);
        fs::create_dir(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                PlatformError {
                    kind: ErrorKind::Conflict,
                    message: "同じ名前の項目があります。".into(),
                }
            } else {
                error(e)
            }
        })?;
        Ok(DirectoryRef {
            name,
            location: location(&path),
            handle: DirectoryHandle(self.register(path)),
        })
    }
    fn create_file(
        &self,
        directory: DirectoryHandle,
        name: &str,
    ) -> Result<(FileRef, FileVersion)> {
        let name = markdown_file_name(name)?;
        let path = self.path(directory.0)?.join(&name);
        // persist_noclobber makes name collisions safe even between concurrent creators.
        let temp = tempfile::NamedTempFile::new_in(self.path(directory.0)?).map_err(error)?;
        temp.as_file().sync_all().map_err(error)?;
        temp.persist_noclobber(&path).map_err(|e| {
            if e.error.kind() == std::io::ErrorKind::AlreadyExists {
                PlatformError {
                    kind: ErrorKind::Conflict,
                    message: "同じ名前のファイルがあります。".into(),
                }
            } else {
                error(e.error)
            }
        })?;
        Ok((
            FileRef {
                name,
                handle: FileHandle(self.register(path)),
            },
            version(b""),
        ))
    }
    fn list_children(&self, directory: DirectoryHandle) -> Result<Vec<Entry>> {
        let mut entries = Vec::new();
        for item in fs::read_dir(self.path(directory.0)?).map_err(error)? {
            let item = item.map_err(error)?;
            let path = item.path();
            let meta = fs::symlink_metadata(&path).map_err(error)?;
            if name(&path).starts_with('.') || hidden_or_link(&meta) {
                continue;
            }
            if meta.is_dir() {
                entries.push(Entry::Directory(self.directory(path)?));
            } else if meta.is_file()
                && path.extension().is_some_and(|e| {
                    let e = e.to_string_lossy();
                    e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown")
                })
            {
                entries.push(Entry::File(self.file(path)?));
            }
        }
        entries.sort_by_key(|e| (!matches!(e, Entry::Directory(_)), e.name().to_lowercase()));
        Ok(entries)
    }
}

impl FileDialog for NativePlatform {
    fn choose_open(&self) -> Result<Option<FileRef>> {
        rfd::FileDialog::new()
            .add_filter("Markdown", &["md", "markdown"])
            .pick_file()
            .map(|p| self.file(p))
            .transpose()
    }
    fn choose_directory(&self) -> Result<Option<DirectoryRef>> {
        rfd::FileDialog::new()
            .pick_folder()
            .map(|p| self.directory(p))
            .transpose()
    }
    fn choose_save(
        &self,
        initial: Option<DirectoryHandle>,
        file_name: &str,
    ) -> Result<Option<SaveTarget>> {
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Markdown", &["md", "markdown"])
            .set_file_name(file_name);
        if let Some(dir) = initial {
            dialog = dialog.set_directory(self.path(dir.0)?);
        }
        let Some(mut path) = dialog.save_file() else {
            return Ok(None);
        };
        if path.extension().is_none() {
            path.set_extension("md");
        }
        if path.exists() {
            path = fs::canonicalize(path).map_err(error)?;
        }
        let expected = Self::current_version(&path)?;
        Ok(Some(SaveTarget {
            file: FileRef {
                name: name(&path),
                handle: FileHandle(self.register(path)),
            },
            expected,
        }))
    }
}

fn encode_link(path: &str) -> String {
    path.replace('\\', "/")
        .replace('%', "%25")
        .replace(' ', "%20")
        .replace('#', "%23")
        .replace('(', "%28")
        .replace(')', "%29")
        .replace('<', "%3C")
        .replace('>', "%3E")
}
fn decode_link(value: &str) -> Result<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Some(n) = value
                .get(i + 1..i + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            decoded.push(n);
            i += 3;
            continue;
        }
        decoded.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(decoded).map_err(|_| PlatformError {
        kind: ErrorKind::Unsupported,
        message: "リンクの文字コードが不正です。".into(),
    })
}
impl NativePlatform {
    fn resource_path(&self, document: FileHandle, destination: &str) -> Result<PathBuf> {
        let destination = decode_link(destination.split('#').next().unwrap_or_default())?;
        let path = Path::new(&destination);
        if path.is_absolute() || destination.contains(':') || destination.starts_with(['/', '\\']) {
            return Err(PlatformError {
                kind: ErrorKind::Unsupported,
                message: "相対パスのページ・画像リンクに対応しています。".into(),
            });
        }
        let current = self.path(document.0)?;
        Ok(current.parent().ok_or_else(conflict)?.join(path))
    }
    fn relative_link(&self, document: FileHandle, target: &Path) -> Result<String> {
        let target = fs::canonicalize(target).map_err(error)?;
        let current = self.path(document.0)?;
        let base: Vec<_> = current
            .parent()
            .ok_or_else(conflict)?
            .components()
            .collect();
        let target: Vec<_> = target.components().collect();
        let shared = base.iter().zip(&target).take_while(|(a, b)| a == b).count();
        if shared == 0 {
            return Err(PlatformError {
                kind: ErrorKind::Unsupported,
                message: "同じドライブのファイルを選んでください。".into(),
            });
        }
        let mut relative = PathBuf::new();
        for _ in shared..base.len() {
            relative.push("..");
        }
        for part in &target[shared..] {
            relative.push(part.as_os_str());
        }
        Ok(encode_link(&relative.to_string_lossy()))
    }
}
impl DocumentResources for NativePlatform {
    fn open_attachment(&self, document: FileHandle, destination: &str) -> Result<()> {
        let path = fs::canonicalize(self.resource_path(document, destination)?).map_err(error)?;
        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !path.is_file()
            || !matches!(
                extension.as_str(),
                "pdf"
                    | "txt"
                    | "csv"
                    | "zip"
                    | "doc"
                    | "docx"
                    | "xls"
                    | "xlsx"
                    | "ppt"
                    | "pptx"
                    | "odt"
                    | "ods"
                    | "png"
                    | "jpg"
                    | "jpeg"
                    | "gif"
                    | "webp"
                    | "bmp"
                    | "svg"
                    | "mp3"
                    | "mp4"
                    | "wav"
                    | "ogg"
                    | "mov"
                    | "json"
                    | "html"
            )
        {
            return Err(PlatformError {
                kind: ErrorKind::Unsupported,
                message: "この形式の添付ファイルは自動で開けません。".into(),
            });
        }
        open_external(&path.to_string_lossy()).map_err(error)
    }
    fn open_external(&self, destination: &str) -> Result<()> {
        let scheme = destination
            .split_once(':')
            .map(|(scheme, _)| scheme.to_ascii_lowercase());
        if !matches!(scheme.as_deref(), Some("https" | "http" | "mailto"))
            || destination.chars().any(char::is_control)
        {
            return Err(PlatformError {
                kind: ErrorKind::Io,
                message: "このリンクは開けません。".into(),
            });
        }
        open_external(destination).map_err(error)
    }
    fn choose_image(&self, document: FileHandle) -> Result<Option<ResourceLink>> {
        let Some(selected) = rfd::FileDialog::new()
            .add_filter("画像", &["png", "jpg", "jpeg", "gif", "webp"])
            .pick_file()
        else {
            return Ok(None);
        };
        self.import_attachment(document, selected).map(Some)
    }
    fn import_attachment(&self, document: FileHandle, selected: PathBuf) -> Result<ResourceLink> {
        let bytes = fs::read(&selected).map_err(error)?;
        let current = self.path(document.0)?;
        let directory = current.parent().ok_or_else(conflict)?.join("assets");
        fs::create_dir_all(&directory).map_err(error)?;
        let extension = selected
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("png")
            .to_lowercase();
        let target = directory.join(format!("{}.{}", blake3::hash(&bytes).to_hex(), extension));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
        {
            Ok(mut file) => {
                file.write_all(&bytes).map_err(error)?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if fs::read(&target).map_err(error)? != bytes {
                    return Err(conflict());
                }
            }
            Err(e) => return Err(error(e)),
        }
        Ok(ResourceLink {
            label: selected
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            destination: self.relative_link(document, &target)?,
        })
    }
    fn choose_page(&self, document: FileHandle) -> Result<Option<ResourceLink>> {
        let current = self.path(document.0)?;
        let Some(path) = rfd::FileDialog::new()
            .set_directory(current.parent().ok_or_else(conflict)?)
            .add_filter("Markdown", &["md", "markdown"])
            .pick_file()
        else {
            return Ok(None);
        };
        Ok(Some(ResourceLink {
            label: path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            destination: self.relative_link(document, &path)?,
        }))
    }
    fn resolve_page(&self, document: FileHandle, destination: &str) -> Result<FileRef> {
        self.dropped_file(self.resource_path(document, destination)?)
    }
    fn image_bytes(&self, document: FileHandle, destination: &str) -> Result<Vec<u8>> {
        fs::read(self.resource_path(document, destination)?).map_err(error)
    }
}

#[cfg(target_os = "windows")]
fn open_external(destination: &str) -> std::io::Result<()> {
    use std::ptr::null;
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn ShellExecuteW(
            window: *const std::ffi::c_void,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show: i32,
        ) -> isize;
    }
    let operation: Vec<u16> = "open\0".encode_utf16().collect();
    let file: Vec<u16> = destination.encode_utf16().chain(Some(0)).collect();
    // Pass the validated URL directly to the OS, without a command shell.
    let result =
        unsafe { ShellExecuteW(null(), operation.as_ptr(), file.as_ptr(), null(), null(), 1) };
    if result > 32 {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "リンクを開けませんでした ({result})"
        )))
    }
}

#[cfg(not(target_os = "windows"))]
fn open_external(destination: &str) -> std::io::Result<()> {
    let status = std::process::Command::new(if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    })
    .arg(destination)
    .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("リンクを開けませんでした。"))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn imported_attachments_are_copied_deduplicated_and_keep_originals() {
        use super::*;
        let dir = tempfile::tempdir().unwrap();
        let document = dir.path().join("note.md");
        fs::write(&document, "本文").unwrap();
        let native = NativePlatform::default();
        let file = native.dropped_file(document.clone()).unwrap();
        for name in ["画像 # [1].png", "資料.pdf"] {
            let original = dir.path().join(name);
            fs::write(&original, name.as_bytes()).unwrap();
            let link = native
                .import_attachment(file.handle, original.clone())
                .unwrap();
            assert!(link.destination.starts_with("assets/"));
            assert_eq!(
                native.image_bytes(file.handle, &link.destination).unwrap(),
                name.as_bytes()
            );
            assert_eq!(
                native
                    .import_attachment(file.handle, original.clone())
                    .unwrap()
                    .destination,
                link.destination
            );
            assert_eq!(fs::read(original).unwrap(), name.as_bytes());
        }
        assert_eq!(fs::read_to_string(document).unwrap(), "本文");
        assert!(
            native
                .import_attachment(file.handle, dir.path().to_owned())
                .is_err()
        );
        assert!(
            native
                .import_attachment(file.handle, dir.path().join("missing.png"))
                .is_err()
        );
    }
    #[test]
    fn relative_resources_resolve_unicode_spaces_and_encoded_symbols() {
        use norito_platform::{DocumentResources, FileStore};
        let dir = tempfile::tempdir().unwrap();
        let document = dir.path().join("document.md");
        let other = dir.path().join("日本語 # 2.md");
        let image = dir.path().join("image space.png");
        std::fs::write(&document, "").unwrap();
        std::fs::write(&other, "# target").unwrap();
        std::fs::write(&image, b"image bytes").unwrap();
        let native = super::NativePlatform::default();
        let current = native.file(document).unwrap();
        let url = native.relative_link(current.handle, &other).unwrap();
        let resolved = native
            .resolve_page(current.handle, &format!("{url}#target"))
            .unwrap();
        assert_eq!(native.read(resolved.handle).unwrap().bytes, b"# target");
        assert_eq!(
            native
                .image_bytes(current.handle, "image%20space.png")
                .unwrap(),
            b"image bytes"
        );
        assert!(
            native
                .resolve_page(current.handle, "https://example.com/file.md")
                .is_err()
        );
        assert_eq!(super::decode_link("%日本").unwrap(), "%日本");
    }
    #[test]
    fn dropped_markdown_resolves_without_modifying_the_file() {
        use norito_platform::FileStore;
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("日本語.MD");
        std::fs::write(&path, "# 見出し").unwrap();
        let native = super::NativePlatform::default();
        let file = native.dropped_file(path.clone()).unwrap();
        assert_eq!(
            native.read(file.handle).unwrap().bytes,
            "# 見出し".as_bytes()
        );
        assert_eq!(std::fs::read_to_string(path).unwrap(), "# 見出し");
        assert!(native.dropped_file(temp.path().to_owned()).is_err());
        let other = temp.path().join("image.png");
        std::fs::write(&other, b"not markdown").unwrap();
        assert!(native.dropped_file(other).is_err());
        assert!(native.dropped_file(temp.path().join("missing.md")).is_err());
    }
    use super::*;
    #[test]
    fn creates_directory_without_extension_and_rejects_collisions() {
        let temp = tempfile::tempdir().unwrap();
        let adapter = NativePlatform::default();
        let root = adapter.directory(temp.path().into()).unwrap();
        let child = adapter.create_directory(root.handle, "資料").unwrap();
        assert!(temp.path().join("資料").is_dir());
        assert_eq!(child.name, "資料");
        assert!(adapter.create_directory(root.handle, "資料").is_err());
        assert!(adapter.create_directory(root.handle, "../outside").is_err());
        adapter.create_file(child.handle, "議事録").unwrap();
        assert!(temp.path().join("資料/議事録.md").is_file());
    }
    #[test]
    fn creates_markdown_without_overwriting_or_escaping_directory() {
        let dir = tempfile::tempdir().unwrap();
        let adapter = NativePlatform::default();
        let root = adapter.directory(dir.path().into()).unwrap();
        let (file, _) = adapter.create_file(root.handle, "新しいノート").unwrap();
        assert_eq!(file.name, "新しいノート.md");
        let path = dir.path().join(&file.name);
        fs::write(&path, "keep").unwrap();
        assert_eq!(
            adapter
                .create_file(root.handle, &file.name)
                .unwrap_err()
                .kind,
            ErrorKind::Conflict
        );
        assert_eq!(fs::read_to_string(path).unwrap(), "keep");
        for name in [
            "",
            "../escape",
            "C:\\escape",
            "a/b",
            "NUL",
            "CON.txt",
            "foo.",
        ] {
            assert!(adapter.create_file(root.handle, name).is_err(), "{name}");
        }
        assert_eq!(adapter.list_children(root.handle).unwrap().len(), 1);
    }
    #[test]
    fn atomic_save_and_conflict_preserve_external_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.md");
        fs::write(&path, "old").unwrap();
        let adapter = NativePlatform::default();
        let file = adapter.file(path.clone()).unwrap();
        let old = adapter.read(file.handle).unwrap();
        let target = SaveTarget {
            file,
            expected: Some(old.version),
        };
        adapter.write(&target, b"new").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert_eq!(
            adapter.write(&target, b"lost").unwrap_err().kind,
            ErrorKind::Conflict
        );
        assert_eq!(fs::read(&path).unwrap(), b"new");
    }
    #[test]
    fn lists_only_markdown_and_directories() {
        let dir = tempfile::tempdir().unwrap();
        for file in ["a.MD", "b.markdown", "image.png", ".hidden.md"] {
            fs::write(dir.path().join(file), "").unwrap();
        }
        fs::create_dir(dir.path().join("notes")).unwrap();
        let adapter = NativePlatform::default();
        let root = adapter.directory(dir.path().into()).unwrap();
        let entries = adapter.list_children(root.handle).unwrap();
        assert_eq!(
            entries.iter().map(Entry::name).collect::<Vec<_>>(),
            ["notes", "a.MD", "b.markdown"]
        );
    }
}
