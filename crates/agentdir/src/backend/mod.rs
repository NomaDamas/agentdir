pub mod local;

pub use local::LocalBackend;

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::error::Result;
use crate::types::{SourceMetadata, SourcePath};

/// Events emitted by a backend watcher.
#[derive(Debug, Clone)]
pub enum SourceEvent {
    /// A new file was created at this path.
    Created { path: SourcePath },
    /// An existing file was modified.
    Modified { path: SourcePath },
    /// A file was deleted.
    Deleted { path: SourcePath },
    /// A file was renamed or moved.
    Renamed { from: SourcePath, to: SourcePath },
    /// Events may have been missed; a full rescan is required.
    RescanNeeded,
}

/// Handle to an active watcher. Stops watching when dropped.
pub struct WatchHandle {
    cancel: tokio_util::sync::CancellationToken,
}

impl WatchHandle {
    pub fn new(cancel: tokio_util::sync::CancellationToken) -> Self {
        Self { cancel }
    }

    /// Signal the watcher to stop.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

/// Abstract backend trait for filesystem providers.
#[async_trait]
pub trait Backend: Send + Sync {
    /// List all files under a source root, returning `(path, metadata)` pairs.
    async fn scan(&self, root: &SourcePath) -> Result<Vec<(SourcePath, SourceMetadata)>>;

    /// Get metadata for a single file.
    async fn metadata(&self, path: &SourcePath) -> Result<SourceMetadata>;

    /// Read file content as bytes.
    async fn read_bytes(&self, path: &SourcePath) -> Result<Vec<u8>>;

    /// Start watching source roots for changes.
    async fn watch(
        &self,
        roots: &[SourcePath],
        tx: mpsc::Sender<SourceEvent>,
    ) -> Result<WatchHandle>;

    /// Human-readable backend name.
    fn name(&self) -> &str;

    /// Whether this backend supports CoW reflink cloning.
    fn supports_reflink(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_backend_implements_trait() {
        use std::sync::Arc;

        let backend: Arc<dyn Backend> = Arc::new(LocalBackend);
        assert_eq!(backend.name(), "local");
        assert!(backend.supports_reflink());
    }

    #[test]
    fn test_watch_handle_cancels_on_drop() {
        let token = tokio_util::sync::CancellationToken::new();
        let child = token.child_token();
        let probe = child.clone();
        {
            let handle = WatchHandle::new(child);
            assert!(!token.is_cancelled());
            drop(handle);
        }
        assert!(probe.is_cancelled());
    }
}
