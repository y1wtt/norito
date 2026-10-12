//! FileDialog and ResourcePicker: native pickers, mapped to registered handles.
use super::{NativePlatform, error, files::current_version, stem};
use norito_platform::*;
use std::fs;

fn markdown() -> rfd::FileDialog {
    rfd::FileDialog::new().add_filter("Markdown", &["md", "markdown"])
}

impl FileDialog for NativePlatform {
    fn choose_open(&self) -> Result<Option<FileRef>> {
        markdown().pick_file().map(|p| self.file(p)).transpose()
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
        let mut dialog = markdown().set_file_name(file_name);
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
        let expected = current_version(&path)?;
        Ok(Some(SaveTarget {
            file: self.file_ref(path),
            expected,
        }))
    }
}

impl ResourcePicker for NativePlatform {
    fn choose_image(&self, document: FileHandle) -> Result<Option<ResourceLink>> {
        let Some(selected) = rfd::FileDialog::new()
            .add_filter("画像", &["png", "jpg", "jpeg", "gif", "webp"])
            .pick_file()
        else {
            return Ok(None);
        };
        self.import_attachment(document, selected).map(Some)
    }
    fn choose_page(&self, document: FileHandle) -> Result<Option<ResourceLink>> {
        let Some(path) = markdown().set_directory(self.parent(document)?).pick_file() else {
            return Ok(None);
        };
        Ok(Some(ResourceLink {
            label: stem(&path),
            destination: self.relative_link(document, &path)?,
        }))
    }
}
