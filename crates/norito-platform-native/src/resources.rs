//! DocumentResources: relative links <-> paths beside the document, and attachment import.
use super::{NativePlatform, conflict, error, stem};
use norito_platform::*;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

fn unsupported(message: &str) -> PlatformError {
    PlatformError::new(ErrorKind::Unsupported, message)
}

/// Percent-encode the characters that would end or split a Markdown link destination.
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
    String::from_utf8(decoded).map_err(|_| unsupported("リンクの文字コードが不正です。"))
}

impl NativePlatform {
    /// The path a relative `destination` names. Absolute paths and URLs are refused.
    pub(crate) fn resource_path(&self, document: FileHandle, destination: &str) -> Result<PathBuf> {
        let destination = decode_link(destination.split('#').next().unwrap_or_default())?;
        let path = Path::new(&destination);
        if path.is_absolute() || destination.contains(':') || destination.starts_with(['/', '\\']) {
            return Err(unsupported(
                "相対パスのページ・画像リンクに対応しています。",
            ));
        }
        Ok(self.parent(document)?.join(path))
    }
    /// An encoded link from the document's directory to `target`.
    pub(crate) fn relative_link(&self, document: FileHandle, target: &Path) -> Result<String> {
        let target = fs::canonicalize(target).map_err(error)?;
        let parent = self.parent(document)?;
        let base: Vec<_> = parent.components().collect();
        let target: Vec<_> = target.components().collect();
        let shared = base.iter().zip(&target).take_while(|(a, b)| a == b).count();
        if shared == 0 {
            return Err(unsupported("同じドライブのファイルを選んでください。"));
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
    /// Content-addressed copy into `assets/`; identical files share one copy.
    fn import_attachment(&self, document: FileHandle, selected: PathBuf) -> Result<ResourceLink> {
        let bytes = fs::read(&selected).map_err(error)?;
        let directory = self.parent(document)?.join("assets");
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
            Ok(mut file) => file.write_all(&bytes).map_err(error)?,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if fs::read(&target).map_err(error)? != bytes {
                    return Err(conflict());
                }
            }
            Err(e) => return Err(error(e)),
        }
        Ok(ResourceLink {
            label: stem(&selected),
            destination: self.relative_link(document, &target)?,
        })
    }
    fn resolve_page(&self, document: FileHandle, destination: &str) -> Result<FileRef> {
        self.markdown_file(self.resource_path(document, destination)?)
    }
    fn image_bytes(&self, document: FileHandle, destination: &str) -> Result<Vec<u8>> {
        fs::read(self.resource_path(document, destination)?).map_err(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imported_attachments_are_copied_deduplicated_and_keep_originals() {
        let dir = tempfile::tempdir().unwrap();
        let document = dir.path().join("note.md");
        fs::write(&document, "本文").unwrap();
        let native = NativePlatform::default();
        let file = native.markdown_file(document.clone()).unwrap();
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
        let dir = tempfile::tempdir().unwrap();
        let document = dir.path().join("document.md");
        let other = dir.path().join("日本語 # 2.md");
        let image = dir.path().join("image space.png");
        fs::write(&document, "").unwrap();
        fs::write(&other, "# target").unwrap();
        fs::write(&image, b"image bytes").unwrap();
        let native = NativePlatform::default();
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
        assert_eq!(decode_link("%日本").unwrap(), "%日本");
    }
}
