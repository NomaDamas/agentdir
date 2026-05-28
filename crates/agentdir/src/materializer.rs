//! Materializer — creates real files on disk at virtual paths using CoW reflinks.
//!
//! The materializer is commanded by the Workspace. It does NOT interact with the Catalog.
//! It only knows about CatalogEntry structs passed to it.

use std::cmp::Ordering;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use tracing::info;

use crate::error::{AgentdirError, Result};
use crate::reflink::{self, CloneResult};
use crate::types::{CatalogEntry, EntryType, MaterializeStrategy, VirtualPath};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaterializeResult {
    Reflinked,
    Copied(u64),
    DirCreated,
    Symlinked,
    Hardlinked,
    Virtual,
}

/// Summary of materializing multiple entries.
#[derive(Debug, Default)]
pub struct MaterializeSummary {
    pub reflinked: usize,
    pub copied: usize,
    pub dirs_created: usize,
    pub errors: Vec<(VirtualPath, AgentdirError)>,
}

#[derive(Debug, Default)]
pub struct BatchResult {
    pub succeeded: usize,
    pub failed: usize,
    pub reflinked: usize,
    pub copied: usize,
    pub symlinked: usize,
    pub hardlinked: usize,
    pub dirs_created: usize,
    pub errors: Vec<(VirtualPath, AgentdirError)>,
}

/// Progress reporter for batch operations.
pub trait ProgressReporter: Send + Sync {
    fn report(&self, completed: usize, total: usize, current: &VirtualPath);
}

/// Default progress reporter that logs via tracing.
pub struct LogProgressReporter;

impl ProgressReporter for LogProgressReporter {
    fn report(&self, completed: usize, total: usize, current: &VirtualPath) {
        info!("materializing [{}/{}] {}", completed, total, current);
    }
}

pub struct Materializer {
    pub materialized_root: PathBuf,
    pub strategy: MaterializeStrategy,
}

impl Materializer {
    pub fn new(root: PathBuf) -> Result<Self> {
        Self::with_strategy(root, MaterializeStrategy::default())
    }

    pub fn with_strategy(root: PathBuf, strategy: MaterializeStrategy) -> Result<Self> {
        if !root.exists() {
            fs::create_dir_all(&root)?;
        }
        Ok(Self {
            materialized_root: root,
            strategy,
        })
    }

    /// Get the real on-disk path for a virtual path.
    pub fn materialized_path(&self, virtual_path: &VirtualPath) -> PathBuf {
        let rel = virtual_path.as_str().trim_start_matches('/');
        self.materialized_root.join(rel)
    }

    pub fn materialize_entry(&self, entry: &CatalogEntry) -> Result<MaterializeResult> {
        if matches!(self.strategy, MaterializeStrategy::Virtual) {
            return Ok(MaterializeResult::Virtual);
        }

        let dst = self.materialized_path(&entry.virtual_path);

        match &entry.metadata.entry_type {
            EntryType::File => {
                let src = entry.source_path.as_path();
                if let Some(parent) = dst.parent() {
                    fs::create_dir_all(parent)?;
                }
                let result = match self.strategy {
                    MaterializeStrategy::Symlink => {
                        #[cfg(unix)]
                        {
                            std::os::unix::fs::symlink(src, &dst).map_err(|e| {
                                AgentdirError::ReflinkFailed(format!(
                                    "symlink {} -> {:?}: {e}",
                                    src.display(),
                                    dst
                                ))
                            })?;
                        }
                        #[cfg(windows)]
                        {
                            std::os::windows::fs::symlink_file(src, &dst).map_err(|e| {
                                AgentdirError::ReflinkFailed(format!(
                                    "symlink {} -> {:?}: {e}",
                                    src.display(),
                                    dst
                                ))
                            })?;
                        }
                        MaterializeResult::Symlinked
                    }
                    MaterializeStrategy::Hardlink => {
                        fs::hard_link(src, &dst).map_err(|e| {
                            AgentdirError::ReflinkFailed(format!(
                                "hardlink {} -> {:?}: {e}",
                                src.display(),
                                dst
                            ))
                        })?;
                        MaterializeResult::Hardlinked
                    }
                    MaterializeStrategy::Virtual => unreachable!(),
                    MaterializeStrategy::Reflink => {
                        let clone = reflink::clone_file(src, &dst)?;
                        match clone {
                            CloneResult::Reflinked => MaterializeResult::Reflinked,
                            CloneResult::Copied(bytes) => MaterializeResult::Copied(bytes),
                        }
                    }
                };
                info!(?src, ?dst, strategy = ?self.strategy, "materialized file");
                Ok(result)
            }
            EntryType::Directory => {
                fs::create_dir_all(&dst)?;
                info!(?dst, "created materialized directory");
                Ok(MaterializeResult::DirCreated)
            }
        }
    }

    /// Remove a materialized entry from disk.
    pub fn dematerialize_entry(&self, virtual_path: &VirtualPath) -> Result<()> {
        let path = self.materialized_path(virtual_path);

        if path.is_dir() && !path.is_symlink() {
            fs::remove_dir_all(&path)?;
        } else if path.exists() || path.symlink_metadata().is_ok() {
            fs::remove_file(&path)?;
        }

        Ok(())
    }

    pub fn refresh_entry(&self, entry: &CatalogEntry) -> Result<MaterializeResult> {
        if matches!(self.strategy, MaterializeStrategy::Virtual) {
            return Ok(MaterializeResult::Virtual);
        }

        match self.strategy {
            MaterializeStrategy::Symlink | MaterializeStrategy::Hardlink => {
                self.dematerialize_entry(&entry.virtual_path)?;
                self.materialize_entry(entry)
            }
            _ => self.refresh_entry_reflink(entry),
        }
    }

    fn refresh_entry_reflink(&self, entry: &CatalogEntry) -> Result<MaterializeResult> {
        let dst = self.materialized_path(&entry.virtual_path);

        match &entry.metadata.entry_type {
            EntryType::File => {
                let parent = dst.parent().ok_or_else(|| {
                    AgentdirError::InvalidPath(format!("materialized path {:?} has no parent", dst))
                })?;
                fs::create_dir_all(parent)?;
                let tmp = parent.join(format!(
                    ".agentdir-refresh-{}-{}",
                    std::process::id(),
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos()
                ));

                let result = match reflink::clone_file(entry.source_path.as_path(), &tmp) {
                    Ok(CloneResult::Reflinked) => MaterializeResult::Reflinked,
                    Ok(CloneResult::Copied(bytes)) => MaterializeResult::Copied(bytes),
                    Err(error) => {
                        let _ = fs::remove_file(&tmp);
                        return Err(error);
                    }
                };

                if let Err(error) = fs::rename(&tmp, &dst) {
                    let _ = fs::remove_file(&tmp);
                    return Err(AgentdirError::Io(error));
                }

                Ok(result)
            }
            EntryType::Directory => self.materialize_entry(entry),
        }
    }

    /// Materialize entries in batches with optional progress reporting.
    ///
    /// Sorts: directories first (by depth ascending), then files.
    /// Continues on individual errors (collects into `BatchResult`).
    pub fn materialize_batch(
        &self,
        entries: &[CatalogEntry],
        progress: Option<&dyn ProgressReporter>,
        chunk_size: usize,
    ) -> Result<BatchResult> {
        let mut result = BatchResult::default();

        let mut sorted: Vec<&CatalogEntry> = entries.iter().collect();
        sorted.sort_by(|a, b| {
            let a_is_dir = matches!(a.metadata.entry_type, EntryType::Directory);
            let b_is_dir = matches!(b.metadata.entry_type, EntryType::Directory);
            match (a_is_dir, b_is_dir) {
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                _ => virtual_depth(&a.virtual_path).cmp(&virtual_depth(&b.virtual_path)),
            }
        });

        let total = sorted.len();
        let effective_chunk = if chunk_size == 0 { 50 } else { chunk_size };

        for (i, entry) in sorted.iter().enumerate() {
            match self.materialize_entry(entry) {
                Ok(MaterializeResult::Reflinked) => {
                    result.succeeded += 1;
                    result.reflinked += 1;
                }
                Ok(MaterializeResult::Copied(_)) => {
                    result.succeeded += 1;
                    result.copied += 1;
                }
                Ok(MaterializeResult::DirCreated) => {
                    result.succeeded += 1;
                    result.dirs_created += 1;
                }
                Ok(MaterializeResult::Symlinked) => {
                    result.succeeded += 1;
                    result.symlinked += 1;
                }
                Ok(MaterializeResult::Hardlinked) => {
                    result.succeeded += 1;
                    result.hardlinked += 1;
                }
                Ok(MaterializeResult::Virtual) => {
                    result.succeeded += 1;
                }
                Err(e) => {
                    result.failed += 1;
                    result.errors.push((entry.virtual_path.clone(), e));
                }
            }

            if (i + 1) % effective_chunk == 0 || i + 1 == total {
                if let Some(reporter) = progress {
                    reporter.report(i + 1, total, &entry.virtual_path);
                }
            }
        }

        Ok(result)
    }

    /// Dematerialize entries in batches.
    ///
    /// Removes deeper paths first, then shallower ones.
    pub fn dematerialize_batch(&self, paths: &[VirtualPath]) -> Result<BatchResult> {
        let mut result = BatchResult::default();

        let mut sorted: Vec<&VirtualPath> = paths.iter().collect();
        sorted.sort_by(|a, b| {
            let a_depth = a.as_str().matches('/').count();
            let b_depth = b.as_str().matches('/').count();
            b_depth.cmp(&a_depth)
        });

        for path in sorted {
            match self.dematerialize_entry(path) {
                Ok(()) => result.succeeded += 1,
                Err(e) => {
                    result.failed += 1;
                    result.errors.push((path.clone(), e));
                }
            }
        }

        Ok(result)
    }

    /// Materialize all entries. Creates directories first (sorted by depth), then files.
    pub fn materialize_all(&self, entries: &[CatalogEntry]) -> Result<MaterializeSummary> {
        let mut summary = MaterializeSummary::default();
        let mut sorted: Vec<&CatalogEntry> = entries.iter().collect();

        sorted.sort_by(|a, b| {
            let a_is_dir = matches!(a.metadata.entry_type, EntryType::Directory);
            let b_is_dir = matches!(b.metadata.entry_type, EntryType::Directory);

            match (a_is_dir, b_is_dir) {
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                _ => virtual_depth(&a.virtual_path).cmp(&virtual_depth(&b.virtual_path)),
            }
        });

        for entry in sorted {
            match self.materialize_entry(entry) {
                Ok(MaterializeResult::Reflinked) => summary.reflinked += 1,
                Ok(MaterializeResult::Copied(_)) => summary.copied += 1,
                Ok(MaterializeResult::DirCreated) => summary.dirs_created += 1,
                Ok(
                    MaterializeResult::Symlinked
                    | MaterializeResult::Hardlinked
                    | MaterializeResult::Virtual,
                ) => {}
                Err(error) => summary.errors.push((entry.virtual_path.clone(), error)),
            }
        }

        Ok(summary)
    }
}

fn virtual_depth(path: &VirtualPath) -> usize {
    path.as_str().matches('/').count()
}

#[cfg(test)]
mod tests {
    use crate::types::{CatalogEntry, EntryType, SourceMetadata, SourcePath, VirtualPath};
    use std::path::PathBuf;
    use tempfile::TempDir;

    use super::*;

    fn make_file_entry(virtual_path: &str, content: &[u8]) -> (CatalogEntry, TempDir) {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("source_file");
        std::fs::write(&src, content).unwrap();

        let entry = CatalogEntry {
            virtual_path: VirtualPath::new(virtual_path).unwrap(),
            source_path: SourcePath::new(src),
            content_hash: None,
            metadata: SourceMetadata {
                mtime_ns: 1000,
                size_bytes: content.len() as u64,
                entry_type: EntryType::File,
            },
            materialized: false,
        };
        (entry, dir)
    }

    #[test]
    fn test_materialize_file_content() {
        let mat_dir = TempDir::new().unwrap();
        let mat = Materializer::new(mat_dir.path().to_path_buf()).unwrap();

        let (entry, _src_dir) = make_file_entry("/docs/readme.md", b"hello agentdir");

        let result = mat.materialize_entry(&entry).unwrap();
        assert!(matches!(
            result,
            MaterializeResult::Reflinked | MaterializeResult::Copied(_)
        ));

        let mat_path = mat.materialized_path(&entry.virtual_path);
        assert!(mat_path.exists());
        assert_eq!(std::fs::read(&mat_path).unwrap(), b"hello agentdir");
    }

    #[test]
    fn test_materialize_creates_parent_dirs() {
        let mat_dir = TempDir::new().unwrap();
        let mat = Materializer::new(mat_dir.path().to_path_buf()).unwrap();

        let (entry, _src_dir) = make_file_entry("/deep/nested/path/file.txt", b"data");
        mat.materialize_entry(&entry).unwrap();

        let mat_path = mat.materialized_path(&entry.virtual_path);
        assert!(mat_path.exists());
    }

    #[test]
    fn test_dematerialize_removes_file() {
        let mat_dir = TempDir::new().unwrap();
        let mat = Materializer::new(mat_dir.path().to_path_buf()).unwrap();

        let (entry, _src_dir) = make_file_entry("/docs/file.txt", b"content");
        mat.materialize_entry(&entry).unwrap();

        let mat_path = mat.materialized_path(&entry.virtual_path);
        assert!(mat_path.exists());

        mat.dematerialize_entry(&entry.virtual_path).unwrap();
        assert!(!mat_path.exists());
    }

    #[test]
    fn test_refresh_after_modification() {
        let mat_dir = TempDir::new().unwrap();
        let mat = Materializer::new(mat_dir.path().to_path_buf()).unwrap();

        let src_dir = TempDir::new().unwrap();
        let src_path = src_dir.path().join("file.txt");
        std::fs::write(&src_path, b"original").unwrap();

        let entry = CatalogEntry {
            virtual_path: VirtualPath::new("/docs/file.txt").unwrap(),
            source_path: SourcePath::new(src_path.clone()),
            content_hash: None,
            metadata: SourceMetadata {
                mtime_ns: 1000,
                size_bytes: 8,
                entry_type: EntryType::File,
            },
            materialized: false,
        };

        mat.materialize_entry(&entry).unwrap();

        std::fs::write(&src_path, b"modified content").unwrap();

        mat.refresh_entry(&entry).unwrap();

        let mat_path = mat.materialized_path(&entry.virtual_path);
        assert_eq!(std::fs::read(&mat_path).unwrap(), b"modified content");
    }

    #[test]
    fn test_materialize_all_dirs_before_files() {
        let mat_dir = TempDir::new().unwrap();
        let mat = Materializer::new(mat_dir.path().to_path_buf()).unwrap();

        let src_dir = TempDir::new().unwrap();
        let src_file = src_dir.path().join("file.txt");
        std::fs::write(&src_file, b"data").unwrap();

        let entries = vec![
            CatalogEntry {
                virtual_path: VirtualPath::new("/docs/file.txt").unwrap(),
                source_path: SourcePath::new(src_file),
                content_hash: None,
                metadata: SourceMetadata {
                    mtime_ns: 1000,
                    size_bytes: 4,
                    entry_type: EntryType::File,
                },
                materialized: false,
            },
            CatalogEntry {
                virtual_path: VirtualPath::new("/docs").unwrap(),
                source_path: SourcePath::new(PathBuf::new()),
                content_hash: None,
                metadata: SourceMetadata {
                    mtime_ns: 0,
                    size_bytes: 0,
                    entry_type: EntryType::Directory,
                },
                materialized: false,
            },
        ];

        let summary = mat.materialize_all(&entries).unwrap();
        assert_eq!(summary.errors.len(), 0);
        assert_eq!(summary.dirs_created, 1);

        let file_path = mat.materialized_path(&VirtualPath::new("/docs/file.txt").unwrap());
        assert!(file_path.exists());
    }

    #[test]
    fn test_batch_materialize_100() {
        let mat_dir = TempDir::new().unwrap();
        let src_dir = TempDir::new().unwrap();
        let mat = Materializer::new(mat_dir.path().to_path_buf()).unwrap();

        let mut entries = Vec::new();
        for i in 0..100 {
            let src = src_dir.path().join(format!("file{}.txt", i));
            std::fs::write(&src, format!("content {}", i).as_bytes()).unwrap();
            entries.push(CatalogEntry {
                virtual_path: VirtualPath::new(format!("/files/file{}.txt", i)).unwrap(),
                source_path: SourcePath::new(src),
                content_hash: None,
                metadata: SourceMetadata {
                    mtime_ns: 1000,
                    size_bytes: 10,
                    entry_type: EntryType::File,
                },
                materialized: false,
            });
        }

        let result = mat.materialize_batch(&entries, None, 50).unwrap();
        assert_eq!(result.succeeded, 100);
        assert_eq!(result.failed, 0);

        for i in 0..100 {
            let path =
                mat.materialized_path(&VirtualPath::new(format!("/files/file{}.txt", i)).unwrap());
            assert!(path.exists());
        }
    }

    #[test]
    fn test_batch_partial_failure() {
        let mat_dir = TempDir::new().unwrap();
        let src_dir = TempDir::new().unwrap();
        let mat = Materializer::new(mat_dir.path().to_path_buf()).unwrap();

        let good_src = src_dir.path().join("good.txt");
        std::fs::write(&good_src, b"good").unwrap();

        let entries = vec![
            CatalogEntry {
                virtual_path: VirtualPath::new("/good.txt").unwrap(),
                source_path: SourcePath::new(good_src),
                content_hash: None,
                metadata: SourceMetadata {
                    mtime_ns: 1000,
                    size_bytes: 4,
                    entry_type: EntryType::File,
                },
                materialized: false,
            },
            CatalogEntry {
                virtual_path: VirtualPath::new("/bad.txt").unwrap(),
                source_path: SourcePath::new(PathBuf::from("/nonexistent/bad.txt")),
                content_hash: None,
                metadata: SourceMetadata {
                    mtime_ns: 1000,
                    size_bytes: 0,
                    entry_type: EntryType::File,
                },
                materialized: false,
            },
        ];

        let result = mat.materialize_batch(&entries, None, 50).unwrap();
        assert_eq!(result.succeeded, 1);
        assert_eq!(result.failed, 1);
        assert_eq!(result.errors.len(), 1);
    }

    #[test]
    fn test_progress_reporter_called() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        struct CountingReporter(Arc<AtomicUsize>);
        impl ProgressReporter for CountingReporter {
            fn report(&self, _completed: usize, _total: usize, _current: &VirtualPath) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let mat_dir = TempDir::new().unwrap();
        let src_dir = TempDir::new().unwrap();
        let mat = Materializer::new(mat_dir.path().to_path_buf()).unwrap();

        let count = Arc::new(AtomicUsize::new(0));
        let reporter = CountingReporter(count.clone());

        let mut entries = Vec::new();
        for i in 0..5 {
            let src = src_dir.path().join(format!("f{}.txt", i));
            std::fs::write(&src, b"x").unwrap();
            entries.push(CatalogEntry {
                virtual_path: VirtualPath::new(format!("/f{}.txt", i)).unwrap(),
                source_path: SourcePath::new(src),
                content_hash: None,
                metadata: SourceMetadata {
                    mtime_ns: 1000,
                    size_bytes: 1,
                    entry_type: EntryType::File,
                },
                materialized: false,
            });
        }

        mat.materialize_batch(&entries, Some(&reporter), 2).unwrap();
        assert!(count.load(Ordering::SeqCst) >= 1);
    }
}
