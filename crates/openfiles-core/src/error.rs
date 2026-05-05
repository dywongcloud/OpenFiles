use thiserror::Error;

pub type Result<T> = std::result::Result<T, OpenFilesError>;

#[derive(Debug, Error)]
pub enum OpenFilesError {
    #[error("invalid path: {0}")]
    InvalidPath(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("internal error: {0}")]
    Internal(String),
}

#[cfg(feature = "opendal-backend")]
impl From<opendal::Error> for OpenFilesError {
    fn from(value: opendal::Error) -> Self {
        match value.kind() {
            opendal::ErrorKind::NotFound => OpenFilesError::NotFound(value.to_string()),
            opendal::ErrorKind::Unsupported => OpenFilesError::Unsupported(value.to_string()),
            _ => OpenFilesError::Storage(value.to_string()),
        }
    }
}
