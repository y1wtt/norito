//! FileStore: whole-file reads, and conflict-checked replacement through a temporary file.
use super::{NativePlatform, conflict, error};
use norito_platform::*;
use std::{fs, io::Write, path::Path};

pub(crate) fn version(bytes: &[u8]) -> FileVersion {
    FileVersion(blake3::hash(bytes).to_hex().to_string())
}
pub(crate) fn current_version(path: &Path) -> Result<Option<FileVersion>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(version(&bytes))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(error(e)),
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
        if current_version(&path)? != target.expected {
            return Err(conflict());
        }
        let parent = path.parent().ok_or_else(conflict)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(error)?;
        temp.write_all(bytes).map_err(error)?;
        if path.exists() {
            let metadata = fs::metadata(&path).map_err(error)?;
            if metadata.permissions().readonly() {
                return Err(PlatformError::new(
                    ErrorKind::PermissionDenied,
                    "読み取り専用ファイルには保存できません。",
                ));
            }
            temp.as_file()
                .set_permissions(metadata.permissions())
                .map_err(error)?;
        }
        temp.as_file().sync_all().map_err(error)?;
        // Narrow the race with external writers: check again just before replacing.
        if current_version(&path)? != target.expected {
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

#[cfg(test)]
mod tests {
    use super::*;
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
}
