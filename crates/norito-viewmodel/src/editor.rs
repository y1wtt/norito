//! Sequencing: which command is allowed now, which request it issues, and
//! how a completed request changes the session, the workspace and the status.
use crate::*;
use norito_model::Document;
use norito_platform::*;

#[derive(Default)]
pub struct EditorViewModel {
    pub session: Session,
    pub workspace: Workspace,
    pub creation: Option<Creation>,
    /// Navigation waiting for the save / discard / cancel answer.
    pub pending: Option<Action>,
    pub status: Status,
    active: Option<u64>,
    saving: bool,
    next_id: u64,
}

impl EditorViewModel {
    pub fn busy(&self) -> bool {
        self.active.is_some()
    }
    /// Typing continues while saving; everything else that changes the document waits.
    pub fn can_edit(&self) -> bool {
        self.creation.is_none() && self.pending.is_none() && (!self.busy() || self.saving)
    }
    fn issue(&mut self, request: Request) -> Vec<Effect> {
        self.next_id += 1;
        self.active = Some(self.next_id);
        self.saving = matches!(request, Request::Save { .. });
        self.status.note(if self.saving {
            "保存中…"
        } else {
            "読み込み中…"
        });
        vec![Effect {
            id: self.next_id,
            request,
        }]
    }
    /// Open the name prompt in the creation directory. False without a folder.
    fn begin_creation(&mut self, kind: EntryKind) -> bool {
        let Some(directory) = self.workspace.creation_directory().cloned() else {
            return false;
        };
        self.workspace.expanded.insert(directory.handle);
        self.creation = Some(Creation::new(directory, kind));
        self.status.clear();
        true
    }
    fn navigate(&mut self, action: Action) -> Vec<Effect> {
        match action {
            Action::New => {
                if !self.begin_creation(EntryKind::File) {
                    self.session.replace(Document::new(String::new()), None);
                    self.status.clear();
                }
                vec![]
            }
            Action::Open => self.issue(Request::Open(None)),
            Action::OpenFile(file) => self.issue(Request::Open(Some(file))),
            Action::Exit => self.issue(Request::Exit),
        }
    }
    fn save(&mut self, save_as: bool) -> Vec<Effect> {
        let initial = self.workspace.root.as_ref().map(|d| d.handle);
        self.issue(self.session.save_request(save_as, initial))
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
            Command::Edit(source) if self.can_edit() => self.session.document.edit(source),
            Command::Undo if self.can_edit() => self.session.document.undo(),
            Command::Redo if self.can_edit() => self.session.document.redo(),
            _ if self.busy() => return vec![],
            Command::NewName(name) => {
                if let Some(creation) = &mut self.creation {
                    creation.name = name;
                }
                self.status.clear();
            }
            Command::CancelCreate => {
                self.creation = None;
                self.status.clear();
            }
            Command::CreateFile => {
                if let Some(creation) = &self.creation {
                    match creation.request() {
                        Ok(request) => return self.issue(request),
                        Err(e) => self.status.fail(e.message),
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
                self.status.note("キャンセルしました。");
            }
            Command::ConfirmDiscard => {
                if let Some(action) = self.pending.take() {
                    return self.navigate(action);
                }
            }
            Command::ConfirmSave if self.pending.is_some() => return self.save(false),
            _ if self.pending.is_some() => return vec![],
            Command::NewDirectory => {
                if !self.begin_creation(EntryKind::Directory) {
                    self.status.fail("フォルダを開いてから作成してください。");
                }
            }
            Command::Navigate(action) => {
                if self.session.document.dirty() {
                    self.pending = Some(action);
                } else {
                    return self.navigate(action);
                }
            }
            Command::Save(save_as) => return self.save(save_as),
            Command::OpenFolder => return self.issue(Request::OpenFolder),
            Command::Refresh => {
                if let Some(root) = self.workspace.root.clone() {
                    return self.issue(Request::List(root));
                }
            }
            Command::ToggleDirectory(directory) => {
                let unlisted = self.workspace.toggle(&directory);
                return if unlisted {
                    self.issue(Request::List(directory))
                } else {
                    vec![]
                };
            }
            _ => {}
        }
        vec![]
    }
    fn complete(&mut self, result: Result<Response>) -> Vec<Effect> {
        let response = match result {
            Ok(response) => response,
            Err(e) => {
                self.status.fail(e.message);
                return vec![];
            }
        };
        match response {
            Response::DirectoryCreated(parent, directory) => {
                self.workspace.add_directory(parent.handle, directory);
                self.creation = None;
                self.status.clear();
            }
            Response::Created(directory, file, version) => {
                self.workspace
                    .add(directory.handle, Entry::File(file.clone()));
                let target = SaveTarget {
                    file,
                    expected: Some(version),
                };
                self.session
                    .replace(Document::new(String::new()), Some(target));
                self.creation = None;
                self.status.clear();
            }
            Response::Opened(Some((file, read))) => match Document::from_bytes(&read.bytes) {
                Ok(document) => {
                    self.workspace.selected = self.workspace.parent_of(file.handle);
                    let target = SaveTarget {
                        file,
                        expected: Some(read.version),
                    };
                    self.session.replace(document, Some(target));
                    self.status.clear();
                }
                Err(e) => self.status.fail(e),
            },
            Response::Folder(Some((root, entries))) => {
                self.workspace.open(root, entries);
                self.status.clear();
            }
            Response::Listed(directory, entries) => {
                self.workspace.listed(directory, entries);
                self.status.clear();
            }
            Response::Saved(Some((target, version, source))) => {
                self.session.saved(target, version, source);
                let dirty = self.session.document.dirty();
                self.status.note(if dirty {
                    "保存しました。保存後の編集が残っています。"
                } else {
                    "保存しました。"
                });
                if !dirty && let Some(action) = self.pending.take() {
                    return self.navigate(action);
                }
            }
            _ => {
                self.pending = None;
                self.status.note("キャンセルしました。");
            }
        }
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn directory(handle: u64, name: &str) -> DirectoryRef {
        DirectoryRef {
            handle: DirectoryHandle(handle),
            name: name.into(),
            location: String::new(),
        }
    }
    fn with_root(root: &DirectoryRef) -> EditorViewModel {
        let mut vm = EditorViewModel::default();
        vm.workspace.root = Some(root.clone());
        vm
    }
    fn file() -> FileRef {
        FileRef {
            handle: FileHandle(1),
            name: "a.md".into(),
        }
    }
    #[test]
    fn selecting_root_sets_creation_target_without_collapsing() {
        let root = directory(2, "root");
        let child = directory(3, "資料");
        let mut vm = with_root(&root);
        vm.workspace.expanded.insert(root.handle);
        vm.workspace.children.insert(child.handle, vec![]);
        vm.dispatch(Command::ToggleDirectory(child.clone()));
        let target = |vm: &EditorViewModel| vm.workspace.creation_directory().unwrap().handle;
        assert_eq!(target(&vm), child.handle);
        assert!(
            vm.dispatch(Command::ToggleDirectory(root.clone()))
                .is_empty()
        );
        assert_eq!(target(&vm), root.handle);
        assert!(vm.workspace.expanded.contains(&root.handle));
    }
    #[test]
    fn directory_creation_preserves_dirty_document_and_selects_new_parent() {
        let root = directory(2, "root");
        let child = directory(3, "資料");
        let mut vm = with_root(&root);
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
        assert_eq!(vm.session.document.source, "未保存の本文");
        assert!(vm.session.document.dirty());
        assert_eq!(vm.workspace.selected.as_ref().unwrap().handle, child.handle);
        assert!(
            matches!(&vm.workspace.children[&root.handle][0], Entry::Directory(dir) if dir.handle == child.handle)
        );
        vm.dispatch(Command::Navigate(Action::New));
        vm.dispatch(Command::ConfirmDiscard);
        let creation = vm.creation.as_ref().unwrap();
        assert_eq!(creation.directory.handle, child.handle);
        assert_eq!(creation.kind, EntryKind::File);
    }
    #[test]
    fn create_failure_keeps_input_and_document_then_success_opens_file() {
        let root = directory(2, "notes");
        let mut vm = with_root(&root);
        vm.session.document = Document::new("existing".into());
        vm.dispatch(Command::Navigate(Action::New));
        assert!(!vm.can_edit());
        vm.dispatch(Command::NewName("new".into()));
        let effect = vm.dispatch(Command::CreateFile).remove(0);
        assert!(matches!(effect.request, Request::Create(_, ref name) if name == "new.md"));
        vm.dispatch(Command::Completed(
            effect.id,
            Err(PlatformError::new(ErrorKind::Conflict, "exists")),
        ));
        assert_eq!(vm.session.document.source, "existing");
        assert_eq!(vm.creation.as_ref().unwrap().name, "new");
        vm.dispatch(Command::NewName("other".into()));
        let effect = vm.dispatch(Command::CreateFile).remove(0);
        vm.dispatch(Command::Completed(
            effect.id,
            Ok(Response::Created(root, file(), FileVersion("empty".into()))),
        ));
        assert!(vm.creation.is_none());
        assert!(vm.session.document.source.is_empty());
        assert!(!vm.session.document.dirty());
        assert_eq!(vm.workspace.children[&DirectoryHandle(2)].len(), 1);
        assert!(vm.session.is(file().handle));
    }
    #[test]
    fn cancel_file_creation_preserves_unsaved_work() {
        let mut vm = with_root(&directory(2, "notes"));
        vm.dispatch(Command::Edit("unsaved".into()));
        vm.dispatch(Command::Navigate(Action::New));
        assert!(vm.creation.is_none());
        vm.dispatch(Command::ConfirmDiscard);
        assert!(vm.creation.is_some());
        vm.dispatch(Command::CancelCreate);
        assert_eq!(vm.session.document.source, "unsaved");
        assert!(vm.session.document.dirty());
    }
    #[test]
    fn opened_file_selects_its_listed_parent() {
        let root = directory(2, "notes");
        let child = directory(3, "資料");
        let mut vm = with_root(&root);
        vm.workspace
            .children
            .insert(root.handle, vec![Entry::Directory(child.clone())]);
        vm.workspace
            .children
            .insert(child.handle, vec![Entry::File(file())]);
        let effect = vm
            .dispatch(Command::Navigate(Action::OpenFile(file())))
            .remove(0);
        vm.dispatch(Command::Completed(
            effect.id,
            Ok(Response::Opened(Some((
                file(),
                ReadResult {
                    bytes: b"# a".to_vec(),
                    version: FileVersion("1".into()),
                },
            )))),
        ));
        assert_eq!(vm.workspace.selected.as_ref().unwrap().name, "資料");
        assert_eq!(vm.session.epoch, 1);
    }
    #[test]
    fn cancel_navigation_keeps_document() {
        let mut vm = EditorViewModel::default();
        vm.dispatch(Command::Edit("大切なノート".into()));
        assert!(vm.dispatch(Command::Navigate(Action::New)).is_empty());
        assert!(vm.pending.is_some());
        vm.dispatch(Command::Cancel);
        assert_eq!(vm.session.document.source, "大切なノート");
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
        assert_eq!(vm.session.document.source, "");
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
        assert_eq!(vm.session.document.source, "later");
        assert!(vm.session.document.dirty());
    }
    #[test]
    fn failed_open_preserves_current_document() {
        let mut vm = EditorViewModel::default();
        let effect = vm.dispatch(Command::Navigate(Action::Open)).remove(0);
        vm.dispatch(Command::Completed(
            effect.id,
            Err(PlatformError::new(ErrorKind::NotFound, "missing")),
        ));
        assert_eq!(vm.session.document.source, "");
        assert!(vm.status.error);
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
        let version = adapter
            .write(&target, &vm.session.document.bytes())
            .unwrap();
        vm.dispatch(Command::Completed(
            effect.id,
            Ok(Response::Saved(Some((target, version, "saved".into())))),
        ));
        vm.dispatch(Command::Edit("changed".into()));
        vm.dispatch(Command::Undo);
        assert!(!vm.session.document.dirty());
    }
}
