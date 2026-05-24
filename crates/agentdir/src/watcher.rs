use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time;

use crate::backend::{Backend, SourceEvent, WatchHandle};
use crate::error::Result;
use crate::types::SourcePath;

pub struct FileWatcher {
    backend: Arc<dyn Backend>,
    roots: Vec<SourcePath>,
    poll_interval: Duration,
}

impl FileWatcher {
    pub fn new(backend: Arc<dyn Backend>, roots: Vec<SourcePath>) -> Self {
        Self {
            backend,
            roots,
            poll_interval: Duration::from_secs(60),
        }
    }

    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    pub async fn start(&self) -> Result<(mpsc::Receiver<SourceEvent>, WatchHandle)> {
        let (tx, rx) = mpsc::channel(256);

        let handle = self.backend.watch(&self.roots, tx.clone()).await?;

        let poll_tx = tx.clone();
        let poll_interval = self.poll_interval;
        let cancel = handle.cancel_token();

        tokio::spawn(async move {
            let mut interval = time::interval(poll_interval);
            interval.tick().await;

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let _ = poll_tx.send(SourceEvent::RescanNeeded).await;
                    }
                    _ = cancel.cancelled() => {
                        break;
                    }
                }
            }
        });

        Ok((rx, handle))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::local::LocalBackend;
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_detect_file_creation() {
        let dir = TempDir::new().unwrap();
        let backend: Arc<dyn Backend> = Arc::new(LocalBackend);
        let roots = vec![SourcePath::new(dir.path().to_path_buf())];

        let watcher = FileWatcher::new(backend, roots);
        let (mut rx, _handle) = watcher.start().await.unwrap();

        tokio::time::sleep(Duration::from_millis(200)).await;

        std::fs::write(dir.path().join("newfile.txt"), b"hello").unwrap();

        let event = timeout(Duration::from_secs(5), rx.recv()).await;
        assert!(event.is_ok(), "Timed out waiting for file creation event");
        assert!(event.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_watcher_cleanup() {
        let dir = TempDir::new().unwrap();
        let backend: Arc<dyn Backend> = Arc::new(LocalBackend);
        let roots = vec![SourcePath::new(dir.path().to_path_buf())];

        let watcher = FileWatcher::new(backend, roots);
        let (_rx, handle) = watcher.start().await.unwrap();

        drop(handle);
    }

    #[tokio::test]
    async fn test_periodic_polling_emits_rescan() {
        let dir = TempDir::new().unwrap();
        let backend: Arc<dyn Backend> = Arc::new(LocalBackend);
        let roots = vec![SourcePath::new(dir.path().to_path_buf())];

        let watcher =
            FileWatcher::new(backend, roots).with_poll_interval(Duration::from_millis(200));
        let (mut rx, _handle) = watcher.start().await.unwrap();

        let found_rescan = timeout(Duration::from_secs(3), async {
            loop {
                if let Some(event) = rx.recv().await {
                    if matches!(event, SourceEvent::RescanNeeded) {
                        return true;
                    }
                } else {
                    return false;
                }
            }
        })
        .await;

        assert!(
            found_rescan.is_ok() && found_rescan.unwrap(),
            "Expected RescanNeeded event from periodic polling"
        );
    }
}
