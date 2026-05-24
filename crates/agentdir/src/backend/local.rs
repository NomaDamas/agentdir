use std::fs;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use async_trait::async_trait;
use tokio::sync::mpsc;
use walkdir::WalkDir;

use crate::backend::{Backend, SourceEvent, WatchHandle};
use crate::error::{AgentdirError, Result};
use crate::types::{EntryType, SourceMetadata, SourcePath};

pub struct LocalBackend;

#[async_trait]
impl Backend for LocalBackend {
    async fn scan(&self, root: &SourcePath) -> Result<Vec<(SourcePath, SourceMetadata)>> {
        let root_path = root.as_path().to_path_buf();

        tokio::task::spawn_blocking(move || {
            let mut results = Vec::new();

            for entry in WalkDir::new(&root_path)
                .follow_links(false) // prevent infinite loops with circular symlinks
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path().to_path_buf();
                let metadata = fs::symlink_metadata(&path)?;

                let mtime_ns = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);

                let entry_type = if metadata.is_symlink() {
                    let target = fs::read_link(&path).unwrap_or_else(|_| PathBuf::new());
                    EntryType::Symlink { target }
                } else if metadata.is_dir() {
                    EntryType::Directory
                } else {
                    EntryType::File
                };

                let source_meta = SourceMetadata {
                    mtime_ns,
                    size_bytes: metadata.len(),
                    entry_type,
                };

                results.push((SourcePath::new(path), source_meta));
            }

            Ok(results)
        })
        .await
        .map_err(|e| AgentdirError::Io(std::io::Error::other(e)))?
    }

    async fn metadata(&self, path: &SourcePath) -> Result<SourceMetadata> {
        let path = path.as_path().to_path_buf();

        tokio::task::spawn_blocking(move || {
            let metadata = fs::symlink_metadata(&path)?;

            let mtime_ns = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_nanos())
                .unwrap_or(0);

            let entry_type = if metadata.is_symlink() {
                let target = fs::read_link(&path).unwrap_or_else(|_| PathBuf::new());
                EntryType::Symlink { target }
            } else if metadata.is_dir() {
                EntryType::Directory
            } else {
                EntryType::File
            };

            Ok(SourceMetadata {
                mtime_ns,
                size_bytes: metadata.len(),
                entry_type,
            })
        })
        .await
        .map_err(|e| AgentdirError::Io(std::io::Error::other(e)))?
    }

    async fn read_bytes(&self, path: &SourcePath) -> Result<Vec<u8>> {
        let path = path.as_path().to_path_buf();
        tokio::task::spawn_blocking(move || fs::read(&path).map_err(AgentdirError::Io))
            .await
            .map_err(|e| {
                AgentdirError::Io(std::io::Error::other(e))
            })?
    }

    async fn watch(
        &self,
        roots: &[SourcePath],
        tx: mpsc::Sender<SourceEvent>,
    ) -> Result<WatchHandle> {
        use notify::RecursiveMode;
        use notify_debouncer_full::new_debouncer;
        use std::time::Duration;

        let tx_clone = tx.clone();
        let roots_owned: Vec<std::path::PathBuf> =
            roots.iter().map(|r| r.as_path().to_path_buf()).collect();

        let cancel_token = tokio_util::sync::CancellationToken::new();
        let cancel_child = cancel_token.child_token();

        let rt = tokio::runtime::Handle::current();

        std::thread::spawn(move || {
            let (debounce_tx, debounce_rx) = std::sync::mpsc::channel();

            let mut debouncer = match new_debouncer(Duration::from_millis(500), None, debounce_tx) {
                Ok(d) => d,
                Err(e) => {
                    let _ = rt.block_on(tx_clone.send(SourceEvent::RescanNeeded));
                    tracing::error!("failed to create debouncer: {}", e);
                    return;
                }
            };

            for root in &roots_owned {
                if let Err(e) = debouncer.watch(root, RecursiveMode::Recursive) {
                    tracing::warn!("failed to watch {:?}: {}", root, e);
                }
            }

            loop {
                if cancel_child.is_cancelled() {
                    break;
                }

                match debounce_rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(Ok(events)) => {
                        for event in events {
                            if event.need_rescan() {
                                let _ = rt.block_on(tx_clone.send(SourceEvent::RescanNeeded));
                                continue;
                            }

                            use notify::EventKind;
                            let source_event = match event.kind {
                                EventKind::Create(_) => {
                                    event.paths.first().map(|p| SourceEvent::Created {
                                        path: SourcePath::new(p.clone()),
                                    })
                                }
                                EventKind::Modify(notify::event::ModifyKind::Data(_)) => {
                                    event.paths.first().map(|p| SourceEvent::Modified {
                                        path: SourcePath::new(p.clone()),
                                    })
                                }
                                EventKind::Remove(_) => {
                                    event.paths.first().map(|p| SourceEvent::Deleted {
                                        path: SourcePath::new(p.clone()),
                                    })
                                }
                                EventKind::Modify(notify::event::ModifyKind::Name(
                                    notify::event::RenameMode::Both,
                                )) => {
                                    if event.paths.len() >= 2 {
                                        Some(SourceEvent::Renamed {
                                            from: SourcePath::new(event.paths[0].clone()),
                                            to: SourcePath::new(event.paths[1].clone()),
                                        })
                                    } else {
                                        None
                                    }
                                }
                                _ => None,
                            };

                            if let Some(ev) = source_event {
                                let _ = rt.block_on(tx_clone.send(ev));
                            }
                        }
                    }
                    Ok(Err(errors)) => {
                        for e in errors {
                            tracing::warn!("watcher error: {}", e);
                        }
                        let _ = rt.block_on(tx_clone.send(SourceEvent::RescanNeeded));
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        break;
                    }
                }
            }
        });

        Ok(WatchHandle::new(cancel_token))
    }

    fn name(&self) -> &str {
        "local"
    }

    fn supports_reflink(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_scan_directory() {
        let dir = TempDir::new().unwrap();

        std::fs::write(dir.path().join("file1.txt"), b"hello").unwrap();
        std::fs::write(dir.path().join("file2.txt"), b"world").unwrap();
        std::fs::create_dir(dir.path().join("subdir")).unwrap();
        std::fs::write(dir.path().join("subdir/nested.txt"), b"nested").unwrap();

        let backend = LocalBackend;
        let root = SourcePath::new(dir.path().to_path_buf());
        let entries = backend.scan(&root).await.unwrap();

        assert!(entries.len() >= 4);

        let file1 = entries
            .iter()
            .find(|(p, _)| p.as_path().ends_with("file1.txt"));
        assert!(file1.is_some());
        assert_eq!(file1.unwrap().1.size_bytes, 5);
    }

    #[tokio::test]
    async fn test_symlinks_not_followed() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("target.txt");
        std::fs::write(&target, b"target content").unwrap();

        let link = dir.path().join("link.txt");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link).unwrap();

        #[cfg(unix)]
        {
            let backend = LocalBackend;
            let root = SourcePath::new(dir.path().to_path_buf());
            let entries = backend.scan(&root).await.unwrap();

            let link_entry = entries
                .iter()
                .find(|(p, _)| p.as_path().ends_with("link.txt"));
            assert!(link_entry.is_some());

            assert!(matches!(
                link_entry.unwrap().1.entry_type,
                EntryType::Symlink { .. }
            ));
        }
    }

    #[tokio::test]
    async fn test_metadata_returns_correct_size() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, b"12345").unwrap();

        let backend = LocalBackend;
        let source = SourcePath::new(path);
        let meta = backend.metadata(&source).await.unwrap();

        assert_eq!(meta.size_bytes, 5);
        assert!(matches!(meta.entry_type, EntryType::File));
    }

    #[tokio::test]
    async fn test_read_bytes_returns_content() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, b"hello agentdir").unwrap();

        let backend = LocalBackend;
        let source = SourcePath::new(path);
        let bytes = backend.read_bytes(&source).await.unwrap();

        assert_eq!(bytes, b"hello agentdir");
    }

    #[tokio::test]
    async fn test_scan_empty_directory() {
        let dir = TempDir::new().unwrap();
        let backend = LocalBackend;
        let root = SourcePath::new(dir.path().to_path_buf());
        let entries = backend.scan(&root).await.unwrap();

        assert!(!entries.is_empty());
    }
}
