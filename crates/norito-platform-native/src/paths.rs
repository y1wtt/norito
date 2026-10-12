//! Paths: canonicalize shell-provided paths into registered handles.
use super::{NativePlatform, error, name};
use norito_platform::*;
use std::{fs, path::PathBuf};

impl Paths for NativePlatform {
    fn file(&self, path: PathBuf) -> Result<FileRef> {
        Ok(self.file_ref(fs::canonicalize(path).map_err(error)?))
    }
    /// Resolve a dropped or linked file without copying or moving it.
    fn markdown_file(&self, path: PathBuf) -> Result<FileRef> {
        if !is_markdown(&name(&path)) || !path.is_file() {
            return Err(PlatformError::new(
                ErrorKind::Unsupported,
                "Markdownファイル（.md / .markdown）をドロップしてください。",
            ));
        }
        self.file(path)
    }
    fn directory(&self, path: PathBuf) -> Result<DirectoryRef> {
        Ok(self.directory_ref(fs::canonicalize(path).map_err(error)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dropped_markdown_resolves_without_modifying_the_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("日本語.MD");
        fs::write(&path, "# 見出し").unwrap();
        let native = NativePlatform::default();
        let file = native.markdown_file(path.clone()).unwrap();
        assert_eq!(
            native.read(file.handle).unwrap().bytes,
            "# 見出し".as_bytes()
        );
        assert_eq!(fs::read_to_string(path).unwrap(), "# 見出し");
        assert!(native.markdown_file(temp.path().to_owned()).is_err());
        let other = temp.path().join("image.png");
        fs::write(&other, b"not markdown").unwrap();
        assert!(native.markdown_file(other).is_err());
        assert!(
            native
                .markdown_file(temp.path().join("missing.md"))
                .is_err()
        );
    }
}
