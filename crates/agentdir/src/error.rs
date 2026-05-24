use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentdirError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Manifest parse error: {0}")]
    ManifestParse(String),

    #[error("Manifest write error: {0}")]
    ManifestWrite(String),

    #[error("Path overlap: source and materialized roots must not overlap: {0}")]
    PathOverlap(String),

    #[error("Entry not found: {0}")]
    EntryNotFound(String),

    #[error("Entry already exists: {0}")]
    EntryExists(String),

    #[error("Reflink failed: {0}")]
    ReflinkFailed(String),

    #[error("Watcher error: {0}")]
    WatcherError(String),

    #[error("Hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },

    #[error("Invalid path: {0}")]
    InvalidPath(String),
}

pub type Result<T> = std::result::Result<T, AgentdirError>;
