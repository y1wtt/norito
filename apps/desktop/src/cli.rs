//! Command-line options. They also drive repeatable, read-only UI smoke tests.
use crate::services::Services;
use norito_model::Document;
use norito_platform::SaveTarget;
use norito_viewmodel::EditorViewModel;

#[derive(Debug, Default)]
pub struct Options {
    pub folder: Option<String>,
    pub file: Option<String>,
    /// Save a PNG of the window here, then exit.
    pub screenshot: Option<String>,
    pub light: bool,
    pub outline: bool,
    pub new_file: bool,
    pub new_directory: bool,
    pub table_edit: bool,
    pub table_picker: bool,
}

impl Options {
    pub fn parse(args: &[String]) -> Self {
        let value = |name: &str| args.windows(2).find(|a| a[0] == name).map(|a| a[1].clone());
        let flag = |name: &str| args.iter().any(|a| a == name);
        Self {
            folder: value("--folder"),
            file: value("--file"),
            screenshot: value("--screenshot"),
            light: flag("--light"),
            outline: flag("--outline"),
            new_file: flag("--new-file"),
            new_directory: flag("--new-directory"),
            table_edit: flag("--table-edit"),
            table_picker: flag("--table-picker"),
        }
    }
    /// Open the requested folder and file. Failures are reported in the status line.
    pub fn load(&self, services: &Services, vm: &mut EditorViewModel) {
        if let Some(path) = &self.folder {
            match services.paths.directory(path.into()).and_then(|root| {
                services
                    .directories
                    .list_children(root.handle)
                    .map(|entries| (root, entries))
            }) {
                Ok((root, entries)) => vm.workspace.open(root, entries),
                Err(e) => vm.status.fail(e.message),
            }
        }
        if let Some(path) = &self.file {
            let read = services
                .paths
                .file(path.into())
                .and_then(|file| services.files.read(file.handle).map(|read| (file, read)))
                .map_err(|e| e.message);
            match read.and_then(|(file, read)| {
                Document::from_bytes(&read.bytes).map(|document| (file, read.version, document))
            }) {
                Ok((file, version, document)) => vm.session.replace(
                    document,
                    Some(SaveTarget {
                        file,
                        expected: Some(version),
                    }),
                ),
                Err(message) => vm.status.fail(message),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn options_take_values_and_flags() {
        let args: Vec<String> = [
            "norito",
            "--folder",
            "notes",
            "--light",
            "--screenshot",
            "a.png",
        ]
        .map(String::from)
        .into();
        let options = super::Options::parse(&args);
        assert_eq!(options.folder.as_deref(), Some("notes"));
        assert_eq!(options.screenshot.as_deref(), Some("a.png"));
        assert!(options.light && !options.outline && options.file.is_none());
    }
}
