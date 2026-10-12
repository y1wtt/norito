//! DirectoryStore: one-level listing and collision-safe creation.
use super::{NativePlatform, error, exists, files::version, name};
use norito_platform::*;
use std::fs;

#[cfg(windows)]
fn hidden_or_link(meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    // FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_REPARSE_POINT
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
        fs::create_dir(&path).map_err(|e| exists(e, "同じ名前の項目があります。"))?;
        Ok(DirectoryRef {
            name,
            ..self.directory_ref(path)
        })
    }
    fn create_file(
        &self,
        directory: DirectoryHandle,
        name: &str,
    ) -> Result<(FileRef, FileVersion)> {
        let name = markdown_file_name(name)?;
        let directory = self.path(directory.0)?;
        let path = directory.join(&name);
        // persist_noclobber makes name collisions safe even between concurrent creators.
        let temp = tempfile::NamedTempFile::new_in(&directory).map_err(error)?;
        temp.as_file().sync_all().map_err(error)?;
        temp.persist_noclobber(&path)
            .map_err(|e| exists(e.error, "同じ名前のファイルがあります。"))?;
        Ok((
            FileRef {
                name,
                ..self.file_ref(path)
            },
            version(b""),
        ))
    }
    fn list_children(&self, directory: DirectoryHandle) -> Result<Vec<Entry>> {
        let mut entries = Vec::new();
        for item in fs::read_dir(self.path(directory.0)?).map_err(error)? {
            let path = item.map_err(error)?.path();
            let meta = fs::symlink_metadata(&path).map_err(error)?;
            let name = name(&path);
            if name.starts_with('.') || hidden_or_link(&meta) {
                continue;
            }
            if meta.is_dir() {
                entries.push(Entry::Directory(self.directory(path)?));
            } else if meta.is_file() && is_markdown(&name) {
                entries.push(Entry::File(self.file(path)?));
            }
        }
        Entry::sort(&mut entries);
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
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
