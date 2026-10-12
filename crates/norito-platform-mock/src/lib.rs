use norito_platform::*;
use std::{collections::HashMap, sync::Mutex};

#[derive(Default)]
pub struct MemoryFileStore {
    files: Mutex<HashMap<FileHandle, (Vec<u8>, u64)>>,
}
impl MemoryFileStore {
    pub fn insert(&self, handle: FileHandle, bytes: Vec<u8>) {
        let mut files = self.files.lock().unwrap();
        let revision = files.get(&handle).map_or(0, |(_, r)| r + 1);
        files.insert(handle, (bytes, revision));
    }
}
impl FileStore for MemoryFileStore {
    fn read(&self, handle: FileHandle) -> Result<ReadResult> {
        self.files
            .lock()
            .unwrap()
            .get(&handle)
            .map(|(bytes, r)| ReadResult {
                bytes: bytes.clone(),
                version: FileVersion(r.to_string()),
            })
            .ok_or(PlatformError {
                kind: ErrorKind::NotFound,
                message: "Not found".into(),
            })
    }
    fn write(&self, target: &SaveTarget, bytes: &[u8]) -> Result<FileVersion> {
        let mut files = self.files.lock().unwrap();
        let previous = files.get(&target.file.handle);
        if previous.map(|(_, r)| FileVersion(r.to_string())) != target.expected {
            return Err(PlatformError {
                kind: ErrorKind::Conflict,
                message: "Conflict".into(),
            });
        }
        let next = previous.map_or(0, |(_, r)| r + 1);
        files.insert(target.file.handle, (bytes.to_vec(), next));
        Ok(FileVersion(next.to_string()))
    }
}
