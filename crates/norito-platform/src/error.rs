//! The typed failure every port returns. Messages are for display.
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    Io,
    Conflict,
    NotFound,
    PermissionDenied,
    Unsupported,
}
#[derive(Debug, Clone)]
pub struct PlatformError {
    pub kind: ErrorKind,
    pub message: String,
}
impl PlatformError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}
impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(f)
    }
}
impl std::error::Error for PlatformError {}
pub type Result<T> = std::result::Result<T, PlatformError>;
