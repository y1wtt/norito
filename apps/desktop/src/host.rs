use norito_platform::*;
use norito_viewmodel::{Request, Response};

/// The host injects concrete adapters through these independent ports.
pub fn execute(
    files: &dyn FileStore,
    directories: &dyn DirectoryStore,
    dialogs: &dyn FileDialog,
    request: Request,
) -> Result<Response> {
    match request {
        Request::CreateDirectory(parent, name) => {
            let dir = directories.create_directory(parent.handle, &name)?;
            Ok(Response::DirectoryCreated(parent, dir))
        }
        Request::Create(dir, name) => {
            let (file, version) = directories.create_file(dir.handle, &name)?;
            Ok(Response::Created(dir, file, version))
        }
        Request::Open(file) => {
            let file = match file {
                Some(file) => Some(file),
                None => dialogs.choose_open()?,
            };
            Ok(Response::Opened(
                file.map(|f| files.read(f.handle).map(|read| (f, read)))
                    .transpose()?,
            ))
        }
        Request::OpenFolder => {
            let root = dialogs.choose_directory()?;
            Ok(Response::Folder(
                root.map(|d| {
                    directories
                        .list_children(d.handle)
                        .map(|entries| (d, entries))
                })
                .transpose()?,
            ))
        }
        Request::List(dir) => {
            let entries = directories.list_children(dir.handle)?;
            Ok(Response::Listed(dir, entries))
        }
        Request::Save {
            target,
            initial,
            name,
            source,
            bytes,
        } => {
            let target = match target {
                Some(target) => Some(target),
                None => dialogs.choose_save(initial, &name)?,
            };
            Ok(Response::Saved(
                target
                    .map(|target| {
                        files
                            .write(&target, &bytes)
                            .map(|version| (target, version, source))
                    })
                    .transpose()?,
            ))
        }
        Request::Exit => unreachable!("Exit is handled by the GUI host"),
    }
}
