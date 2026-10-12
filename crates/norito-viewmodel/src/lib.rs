//! MVVM application state; no Iced or native adapter dependencies.
use norito_model::Document;
use norito_platform::*;
use std::collections::{HashMap, HashSet};

pub const DEFAULT_DOCUMENT: &str = "";

#[derive(Debug, Clone)]
pub enum Action {
    New,
    Open,
    OpenFile(FileRef),
    Exit,
}
#[derive(Debug, Clone)]
pub enum Command {
    NewDirectory,
    NewName(String),
    CreateFile,
    CancelCreate,
    Edit(String),
    Undo,
    Redo,
    Navigate(Action),
    OpenFolder,
    Refresh,
    ToggleDirectory(DirectoryRef),
    Save(bool),
    ConfirmSave,
    ConfirmDiscard,
    Cancel,
    Completed(u64, Result<Response>),
}
#[derive(Debug, Clone)]
pub struct Effect {
    pub id: u64,
    pub request: Request,
}
#[derive(Debug, Clone)]
pub enum Request {
    CreateDirectory(DirectoryRef, String),
    Create(DirectoryRef, String),
    Open(Option<FileRef>),
    OpenFolder,
    List(DirectoryRef),
    Save {
        target: Option<SaveTarget>,
        initial: Option<DirectoryHandle>,
        name: String,
        source: String,
        bytes: Vec<u8>,
    },
    Exit,
}
#[derive(Debug, Clone)]
pub enum Response {
    DirectoryCreated(DirectoryRef, DirectoryRef),
    Created(DirectoryRef, FileRef, FileVersion),
    Opened(Option<(FileRef, ReadResult)>),
    Folder(Option<(DirectoryRef, Vec<Entry>)>),
    Listed(DirectoryRef, Vec<Entry>),
    Saved(Option<(SaveTarget, FileVersion, String)>),
}

pub struct EditorViewModel {
    pub creating_directory: bool,
    pub creation: Option<(DirectoryRef, String)>,
    pub selected_directory: Option<DirectoryRef>,
    pub document: Document,
    pub file: Option<SaveTarget>,
    pub root: Option<DirectoryRef>,
    pub children: HashMap<DirectoryHandle, Vec<Entry>>,
    pub expanded: HashSet<DirectoryHandle>,
    pub pending: Option<Action>,
    pub status: String,
    pub error: bool,
    pub document_epoch: u64,
    active: Option<u64>,
    saving: bool,
    next_id: u64,
}

impl Default for EditorViewModel {
    fn default() -> Self {
        Self {
            creating_directory: false,
            creation: None,
            selected_directory: None,
            document: Document::new(DEFAULT_DOCUMENT.into()),
            file: None,
            root: None,
            children: HashMap::new(),
            expanded: HashSet::new(),
            pending: None,
            status: "".into(),
            error: false,
            document_epoch: 0,
            active: None,
            saving: false,
            next_id: 0,
        }
    }
}

impl EditorViewModel {
    pub fn busy(&self) -> bool {
        self.active.is_some()
    }
    pub fn can_edit(&self) -> bool {
        self.creation.is_none() && self.pending.is_none() && (!self.busy() || self.saving)
    }
    /// Directory that receives new files and folders.
    pub fn creation_directory(&self) -> Option<&DirectoryRef> {
        self.selected_directory.as_ref().or(self.root.as_ref())
    }
    pub fn name(&self) -> &str {
        self.file
            .as_ref()
            .map_or("無題.md", |t| t.file.name.as_str())
    }
    fn issue(&mut self, request: Request) -> Vec<Effect> {
        self.next_id += 1;
        self.active = Some(self.next_id);
        self.saving = matches!(request, Request::Save { .. });
        self.error = false;
        self.status = if self.saving {
            "保存中…"
        } else {
            "読み込み中…"
        }
        .into();
        vec![Effect {
            id: self.next_id,
            request,
        }]
    }
    fn navigate(&mut self, action: Action) -> Vec<Effect> {
        match action {
            Action::New => {
                if let Some(dir) = self.creation_directory().cloned() {
                    self.expanded.insert(dir.handle);
                    self.creating_directory = false;
                    self.creation = Some((dir, String::new()));
                    self.status.clear();
                    self.error = false;
                    return vec![];
                }
                self.document = Document::new(String::new());
                self.file = None;
                self.document_epoch += 1;
                self.status = "".into();
                vec![]
            }
            Action::Open => self.issue(Request::Open(None)),
            Action::OpenFile(file) => self.issue(Request::Open(Some(file))),
            Action::Exit => self.issue(Request::Exit),
        }
    }
    fn save(&mut self, save_as: bool) -> Vec<Effect> {
        self.issue(Request::Save {
            target: if save_as { None } else { self.file.clone() },
            initial: self.root.as_ref().map(|d| d.handle),
            name: self.name().into(),
            source: self.document.source.clone(),
            bytes: self.document.bytes(),
        })
    }
    pub fn dispatch(&mut self, command: Command) -> Vec<Effect> {
        if let Command::Completed(id, result) = command {
            if self.active != Some(id) {
                return vec![];
            }
            self.active = None;
            self.saving = false;
            return self.complete(result);
        }
        match command {
            Command::Edit(source) if self.can_edit() => self.document.edit(source),
            Command::Undo if self.can_edit() => self.document.undo(),
            Command::Redo if self.can_edit() => self.document.redo(),
            _ if self.busy() => return vec![],
            Command::NewName(name) => {
                if let Some((_, draft)) = &mut self.creation {
                    *draft = name;
                }
                self.error = false;
                self.status.clear();
            }
            Command::CancelCreate => {
                self.creation = None;
                self.error = false;
                self.status.clear();
            }
            Command::CreateFile => {
                if let Some((dir, name)) = self.creation.clone() {
                    let name = if self.creating_directory {
                        entry_name(&name)
                    } else {
                        markdown_file_name(&name)
                    };
                    match name {
                        Ok(name) => {
                            return self.issue(if self.creating_directory {
                                Request::CreateDirectory(dir, name)
                            } else {
                                Request::Create(dir, name)
                            });
                        }
                        Err(e) => {
                            self.error = true;
                            self.status = e.message;
                        }
                    }
                }
            }
            Command::Navigate(Action::Exit) if self.creation.is_some() => {
                self.creation = None;
                return self.dispatch(Command::Navigate(Action::Exit));
            }
            _ if self.creation.is_some() => return vec![],
            Command::Cancel => {
                self.pending = None;
                self.status = "キャンセルしました。".into();
            }
            Command::ConfirmDiscard => {
                if let Some(action) = self.pending.take() {
                    return self.navigate(action);
                }
            }
            Command::ConfirmSave if self.pending.is_some() => return self.save(false),
            _ if self.pending.is_some() => return vec![],
            Command::NewDirectory => {
                if let Some(dir) = self.creation_directory().cloned() {
                    self.expanded.insert(dir.handle);
                    self.creating_directory = true;
                    self.creation = Some((dir, String::new()));
                    self.status.clear();
                    self.error = false;
                } else {
                    self.error = true;
                    self.status = "フォルダを開いてから作成してください。".into();
                }
            }
            Command::Navigate(action) => {
                if self.document.dirty() {
                    self.pending = Some(action);
                } else {
                    return self.navigate(action);
                }
            }
            Command::Save(save_as) => return self.save(save_as),
            Command::OpenFolder => return self.issue(Request::OpenFolder),
            Command::Refresh => {
                if let Some(root) = self.root.clone() {
                    return self.issue(Request::List(root));
                }
            }
            Command::ToggleDirectory(dir) => {
                self.selected_directory = Some(dir.clone());
                // The root is always listed; selecting it only sets the creation target.
                if self.root.as_ref().is_some_and(|r| r.handle == dir.handle) {
                    return vec![];
                }
                if !self.expanded.remove(&dir.handle) {
                    self.expanded.insert(dir.handle);
                    if !self.children.contains_key(&dir.handle) {
                        return self.issue(Request::List(dir));
                    }
                }
            }
            _ => {}
        }
        vec![]
    }
    fn complete(&mut self, result: Result<Response>) -> Vec<Effect> {
        let response = match result {
            Ok(response) => response,
            Err(e) => {
                self.error = true;
                self.status = e.message;
                return vec![];
            }
        };
        match response {
            Response::DirectoryCreated(parent, directory) => {
                let entries = self.children.entry(parent.handle).or_default();
                entries.push(Entry::Directory(directory.clone()));
                entries
                    .sort_by_key(|e| (!matches!(e, Entry::Directory(_)), e.name().to_lowercase()));
                self.children.insert(directory.handle, vec![]);
                self.expanded.insert(directory.handle);
                self.selected_directory = Some(directory);
                self.creation = None;
                self.status.clear();
            }
            Response::Created(dir, file, version) => {
                let entries = self.children.entry(dir.handle).or_default();
                entries.push(Entry::File(file.clone()));
                entries
                    .sort_by_key(|e| (!matches!(e, Entry::Directory(_)), e.name().to_lowercase()));
                self.document = Document::new(String::new());
                self.document_epoch += 1;
                self.file = Some(SaveTarget {
                    file,
                    expected: Some(version),
                });
                self.creation = None;
                self.status.clear();
            }
            Response::Opened(Some((file, read))) => match Document::from_bytes(&read.bytes) {
                Ok(document) => {
                    self.selected_directory = self.children.iter().find_map(|(handle, entries)| {
                        entries
                            .iter()
                            .any(|e| matches!(e, Entry::File(f) if f.handle == file.handle))
                            .then(|| DirectoryRef {
                                handle: *handle,
                                name: String::new(),
                                location: String::new(),
                            })
                    });
                    self.document = document;
                    self.file = Some(SaveTarget {
                        file,
                        expected: Some(read.version),
                    });
                    self.document_epoch += 1;
                    self.status = "".into();
                }
                Err(e) => {
                    self.error = true;
                    self.status = e;
                }
            },
            Response::Folder(Some((root, entries))) => {
                self.children.clear();
                self.expanded.clear();
                self.expanded.insert(root.handle);
                self.children.insert(root.handle, entries);
                self.root = Some(root);
                self.selected_directory = self.root.clone();
                self.status.clear();
            }
            Response::Listed(dir, entries) => {
                // Refresh discards descendant caches so deleted/renamed entries cannot linger.
                if self
                    .root
                    .as_ref()
                    .is_some_and(|root| root.handle == dir.handle)
                {
                    self.children.clear();
                    self.expanded.clear();
                    self.expanded.insert(dir.handle);
                    self.selected_directory = self.root.clone();
                }
                self.children.insert(dir.handle, entries);
                self.status = "".into();
            }
            Response::Saved(Some((mut target, version, source))) => {
                target.expected = Some(version);
                self.file = Some(target);
                self.document.mark_saved(source);
                self.status = if self.document.dirty() {
                    "保存しました。保存後の編集が残っています。"
                } else {
                    "保存しました。"
                }
                .into();
                if !self.document.dirty()
                    && let Some(action) = self.pending.take()
                {
                    return self.navigate(action);
                }
            }
            _ => {
                self.pending = None;
                self.status = "キャンセルしました。".into();
            }
        }
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selecting_root_sets_creation_target_without_collapsing() {
        let root = DirectoryRef {
            handle: DirectoryHandle(2),
            name: "root".into(),
            location: String::new(),
        };
        let child = DirectoryRef {
            handle: DirectoryHandle(3),
            name: "資料".into(),
            location: String::new(),
        };
        let mut vm = EditorViewModel {
            root: Some(root.clone()),
            expanded: [root.handle].into(),
            ..Default::default()
        };
        vm.children.insert(child.handle, vec![]);
        vm.dispatch(Command::ToggleDirectory(child.clone()));
        assert_eq!(vm.creation_directory().unwrap().handle, child.handle);
        assert!(
            vm.dispatch(Command::ToggleDirectory(root.clone()))
                .is_empty()
        );
        assert_eq!(vm.creation_directory().unwrap().handle, root.handle);
        assert!(vm.expanded.contains(&root.handle));
    }
    #[test]
    fn directory_creation_preserves_dirty_document_and_selects_new_parent() {
        let root = DirectoryRef {
            handle: DirectoryHandle(2),
            name: "root".into(),
            location: String::new(),
        };
        let child = DirectoryRef {
            handle: DirectoryHandle(3),
            name: "資料".into(),
            location: String::new(),
        };
        let mut vm = EditorViewModel {
            root: Some(root.clone()),
            ..Default::default()
        };
        vm.dispatch(Command::Edit("未保存の本文".into()));
        vm.dispatch(Command::NewDirectory);
        assert!(vm.pending.is_none());
        vm.dispatch(Command::NewName("資料".into()));
        let effect = vm.dispatch(Command::CreateFile).remove(0);
        assert!(matches!(effect.request, Request::CreateDirectory(_, ref name) if name == "資料"));
        vm.dispatch(Command::Completed(
            effect.id,
            Ok(Response::DirectoryCreated(root.clone(), child.clone())),
        ));
        assert_eq!(vm.document.source, "未保存の本文");
        assert!(vm.document.dirty());
        assert_eq!(vm.selected_directory.as_ref().unwrap().handle, child.handle);
        assert!(
            matches!(&vm.children[&root.handle][0], Entry::Directory(dir) if dir.handle == child.handle)
        );
        vm.dispatch(Command::Navigate(Action::New));
        vm.dispatch(Command::ConfirmDiscard);
        assert_eq!(vm.creation.as_ref().unwrap().0.handle, child.handle);
        assert!(!vm.creating_directory);
    }
    #[test]
    fn create_failure_keeps_input_and_document_then_success_opens_file() {
        let mut vm = EditorViewModel {
            root: Some(DirectoryRef {
                handle: DirectoryHandle(2),
                name: "notes".into(),
                location: String::new(),
            }),
            ..Default::default()
        };
        vm.document = Document::new("existing".into());
        vm.dispatch(Command::Navigate(Action::New));
        assert!(!vm.can_edit());
        vm.dispatch(Command::NewName("new".into()));
        let effect = vm.dispatch(Command::CreateFile).remove(0);
        assert!(matches!(effect.request, Request::Create(_, ref name) if name == "new.md"));
        vm.dispatch(Command::Completed(
            effect.id,
            Err(PlatformError {
                kind: ErrorKind::Conflict,
                message: "exists".into(),
            }),
        ));
        assert_eq!(vm.document.source, "existing");
        assert_eq!(vm.creation.as_ref().unwrap().1, "new");
        vm.dispatch(Command::NewName("other".into()));
        let effect = vm.dispatch(Command::CreateFile).remove(0);
        vm.dispatch(Command::Completed(
            effect.id,
            Ok(Response::Created(
                vm.root.clone().unwrap(),
                file(),
                FileVersion("empty".into()),
            )),
        ));
        assert!(vm.creation.is_none());
        assert!(vm.document.source.is_empty());
        assert!(!vm.document.dirty());
        assert_eq!(vm.children[&DirectoryHandle(2)].len(), 1);
        assert_eq!(vm.file.as_ref().unwrap().file.handle, file().handle);
    }
    #[test]
    fn cancel_file_creation_preserves_unsaved_work() {
        let mut vm = EditorViewModel {
            root: Some(DirectoryRef {
                handle: DirectoryHandle(2),
                name: "notes".into(),
                location: String::new(),
            }),
            ..Default::default()
        };
        vm.dispatch(Command::Edit("unsaved".into()));
        vm.dispatch(Command::Navigate(Action::New));
        assert!(vm.creation.is_none());
        vm.dispatch(Command::ConfirmDiscard);
        assert!(vm.creation.is_some());
        vm.dispatch(Command::CancelCreate);
        assert_eq!(vm.document.source, "unsaved");
        assert!(vm.document.dirty());
    }
    fn file() -> FileRef {
        FileRef {
            handle: FileHandle(1),
            name: "a.md".into(),
        }
    }
    #[test]
    fn cancel_navigation_keeps_document() {
        let mut vm = EditorViewModel::default();
        vm.dispatch(Command::Edit("大切なノート".into()));
        assert!(vm.dispatch(Command::Navigate(Action::New)).is_empty());
        assert!(vm.pending.is_some());
        vm.dispatch(Command::Cancel);
        assert_eq!(vm.document.source, "大切なノート");
    }
    #[test]
    fn stale_completion_is_ignored() {
        let mut vm = EditorViewModel::default();
        vm.dispatch(Command::Navigate(Action::Open));
        vm.dispatch(Command::Completed(
            999,
            Ok(Response::Opened(Some((
                file(),
                ReadResult {
                    bytes: b"bad".to_vec(),
                    version: FileVersion("1".into()),
                },
            )))),
        ));
        assert_eq!(vm.document.source, DEFAULT_DOCUMENT);
        assert!(vm.busy());
    }
    #[test]
    fn save_completion_preserves_edits_made_during_save() {
        let mut vm = EditorViewModel::default();
        vm.dispatch(Command::Edit("first".into()));
        let effect = vm.dispatch(Command::Save(false)).remove(0);
        vm.dispatch(Command::Edit("later".into()));
        let target = SaveTarget {
            file: file(),
            expected: None,
        };
        vm.dispatch(Command::Completed(
            effect.id,
            Ok(Response::Saved(Some((
                target,
                FileVersion("v".into()),
                "first".into(),
            )))),
        ));
        assert_eq!(vm.document.source, "later");
        assert!(vm.document.dirty());
    }
    #[test]
    fn failed_open_preserves_current_document() {
        let mut vm = EditorViewModel::default();
        let effect = vm.dispatch(Command::Navigate(Action::Open)).remove(0);
        vm.dispatch(Command::Completed(
            effect.id,
            Err(PlatformError {
                kind: ErrorKind::NotFound,
                message: "missing".into(),
            }),
        ));
        assert_eq!(vm.document.source, DEFAULT_DOCUMENT);
        assert!(vm.error);
    }
    #[test]
    fn mock_adapter_supports_save_then_undo_to_clean() {
        let adapter = norito_platform_mock::MemoryFileStore::default();
        let mut vm = EditorViewModel::default();
        vm.dispatch(Command::Edit("saved".into()));
        let effect = vm.dispatch(Command::Save(false)).remove(0);
        let target = SaveTarget {
            file: file(),
            expected: None,
        };
        let version = adapter.write(&target, &vm.document.bytes()).unwrap();
        vm.dispatch(Command::Completed(
            effect.id,
            Ok(Response::Saved(Some((target, version, "saved".into())))),
        ));
        vm.dispatch(Command::Edit("changed".into()));
        vm.dispatch(Command::Undo);
        assert!(!vm.document.dirty());
    }
}
