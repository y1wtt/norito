//! ExternalOpener: allow-listed links and attachments, passed to the OS without a shell.
use super::{NativePlatform, error, extension};
use norito_platform::*;
use std::fs;

const SCHEMES: &[&str] = &["https", "http", "mailto"];
const ATTACHMENTS: &[&str] = &[
    "pdf", "txt", "csv", "zip", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "png",
    "jpg", "jpeg", "gif", "webp", "bmp", "svg", "mp3", "mp4", "wav", "ogg", "mov", "json", "html",
];

impl ExternalOpener for NativePlatform {
    fn open_external(&self, destination: &str) -> Result<()> {
        let scheme = destination
            .split_once(':')
            .map(|(scheme, _)| scheme.to_ascii_lowercase());
        if !scheme.is_some_and(|s| SCHEMES.contains(&s.as_str()))
            || destination.chars().any(char::is_control)
        {
            return Err(PlatformError::new(
                ErrorKind::Io,
                "このリンクは開けません。",
            ));
        }
        open(destination).map_err(error)
    }
    fn open_attachment(&self, document: FileHandle, destination: &str) -> Result<()> {
        let path = fs::canonicalize(self.resource_path(document, destination)?).map_err(error)?;
        if !path.is_file() || !ATTACHMENTS.contains(&extension(&path).as_str()) {
            return Err(PlatformError::new(
                ErrorKind::Unsupported,
                "この形式の添付ファイルは自動で開けません。",
            ));
        }
        open(&path.to_string_lossy()).map_err(error)
    }
}

#[cfg(target_os = "windows")]
fn open(destination: &str) -> std::io::Result<()> {
    use std::ptr::null;
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn ShellExecuteW(
            window: *const std::ffi::c_void,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show: i32,
        ) -> isize;
    }
    let operation: Vec<u16> = "open\0".encode_utf16().collect();
    let file: Vec<u16> = destination.encode_utf16().chain(Some(0)).collect();
    // Pass the validated URL directly to the OS, without a command shell.
    let result =
        unsafe { ShellExecuteW(null(), operation.as_ptr(), file.as_ptr(), null(), null(), 1) };
    if result > 32 {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "リンクを開けませんでした ({result})"
        )))
    }
}

#[cfg(not(target_os = "windows"))]
fn open(destination: &str) -> std::io::Result<()> {
    let status = std::process::Command::new(if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    })
    .arg(destination)
    .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("リンクを開けませんでした。"))
    }
}
