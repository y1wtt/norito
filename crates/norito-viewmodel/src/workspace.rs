//! The folder tree: lazily listed children, expansion, and the creation target.
use norito_platform::{DirectoryHandle, DirectoryRef, Entry, FileHandle};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub struct Workspace {
    pub root: Option<DirectoryRef>,
    pub children: HashMap<DirectoryHandle, Vec<Entry>>,
    pub expanded: HashSet<DirectoryHandle>,
    /// Receives new files and folders; the root when `None`.
    pub selected: Option<DirectoryRef>,
}

impl Workspace {
    /// Directory that receives new files and folders.
    pub fn creation_directory(&self) -> Option<&DirectoryRef> {
        self.selected.as_ref().or(self.root.as_ref())
    }
    pub fn is_root(&self, directory: DirectoryHandle) -> bool {
        self.root.as_ref().is_some_and(|r| r.handle == directory)
    }
    /// Replace the tree with a newly listed root.
    pub fn open(&mut self, root: DirectoryRef, entries: Vec<Entry>) {
        self.reset(&root);
        self.children.insert(root.handle, entries);
        self.root = Some(root);
        self.selected = self.root.clone();
    }
    /// Refresh discards descendant caches so deleted/renamed entries cannot linger.
    pub fn listed(&mut self, directory: DirectoryRef, entries: Vec<Entry>) {
        if self.is_root(directory.handle) {
            self.reset(&directory);
            self.selected = self.root.clone();
        }
        self.children.insert(directory.handle, entries);
    }
    /// Select `directory` and flip its expansion. True when it must be listed first.
    pub fn toggle(&mut self, directory: &DirectoryRef) -> bool {
        self.selected = Some(directory.clone());
        // The root is always listed; selecting it only sets the creation target.
        if self.is_root(directory.handle) {
            return false;
        }
        if self.expanded.remove(&directory.handle) {
            return false;
        }
        self.expanded.insert(directory.handle);
        !self.children.contains_key(&directory.handle)
    }
    pub fn add(&mut self, parent: DirectoryHandle, entry: Entry) {
        let entries = self.children.entry(parent).or_default();
        entries.push(entry);
        Entry::sort(entries);
    }
    /// A new, empty directory becomes the next creation target.
    pub fn add_directory(&mut self, parent: DirectoryHandle, directory: DirectoryRef) {
        self.add(parent, Entry::Directory(directory.clone()));
        self.children.insert(directory.handle, vec![]);
        self.expanded.insert(directory.handle);
        self.selected = Some(directory);
    }
    /// The listed directory containing `file`, if any.
    pub fn parent_of(&self, file: FileHandle) -> Option<DirectoryRef> {
        let parent = self.children.iter().find_map(|(handle, entries)| {
            entries
                .iter()
                .any(|e| matches!(e, Entry::File(f) if f.handle == file))
                .then_some(*handle)
        })?;
        Some(self.directory(parent).unwrap_or(DirectoryRef {
            handle: parent,
            name: String::new(),
            location: String::new(),
        }))
    }
    fn directory(&self, handle: DirectoryHandle) -> Option<DirectoryRef> {
        self.root
            .iter()
            .chain(self.children.values().flatten().filter_map(|e| match e {
                Entry::Directory(d) => Some(d),
                Entry::File(_) => None,
            }))
            .find(|d| d.handle == handle)
            .cloned()
    }
    fn reset(&mut self, root: &DirectoryRef) {
        self.children.clear();
        self.expanded.clear();
        self.expanded.insert(root.handle);
    }
}
