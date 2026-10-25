//! Composition root: concrete adapters behind their ports, and the one way to call them.
use iced::Task;
use norito_platform::*;
use norito_platform_native::NativePlatform;
use std::sync::Arc;

/// Every OS service the app uses. Only `native` names a concrete adapter.
#[derive(Clone)]
pub struct Services {
    pub files: Arc<dyn FileStore>,
    pub directories: Arc<dyn DirectoryStore>,
    pub dialogs: Arc<dyn FileDialog>,
    pub paths: Arc<dyn Paths>,
    pub resources: Arc<dyn DocumentResources>,
    pub picker: Arc<dyn ResourcePicker>,
    pub opener: Arc<dyn ExternalOpener>,
    #[cfg(target_os = "windows")]
    pub clipboard: Arc<dyn ClipboardStore>,
}

impl Services {
    pub fn native() -> Self {
        let native = Arc::new(NativePlatform::default());
        Self {
            files: native.clone(),
            directories: native.clone(),
            dialogs: native.clone(),
            paths: native.clone(),
            resources: native.clone(),
            picker: native.clone(),
            opener: native,
            #[cfg(target_os = "windows")]
            clipboard: Arc::new(norito_platform_native::clipboard::NativeClipboard),
        }
    }
}

/// Run `work` on a blocking worker, never the UI thread; a lost worker becomes `failed`.
pub async fn off_thread<R: Send + 'static>(
    work: impl FnOnce() -> R + Send + 'static,
    failed: impl FnOnce(String) -> R,
) -> R {
    tokio::task::spawn_blocking(work)
        .await
        .unwrap_or_else(|e| failed(e.to_string()))
}

/// `off_thread` for port calls, delivering the result as a message.
pub fn blocking<T: Send + 'static, M: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
    done: impl Fn(Result<T>) -> M + Send + 'static,
) -> Task<M> {
    Task::perform(
        off_thread(work, |e| Err(PlatformError::new(ErrorKind::Io, e))),
        done,
    )
}
