//! Materializer — creates real files on disk at virtual paths using CoW reflinks.
//!
//! The materializer is commanded by the Workspace. It does NOT interact with the Catalog.
//! It only knows about CatalogEntry structs passed to it.

use std::cmp::Ordering;
use std::fs;
use std::path::PathBuf;

use tracing::info;

use crate::error::{AgentdirError, Result};
use crate::reflink::{self, CloneResult};
use crate::types::{CatalogEntry, EntryType, VirtualPath};

/// Result of materializing a single entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaterializeResult {
    /// File was cloned using CoW reflink.
    Reflinked,
    /// File was copied byte-by-byte.
    Copied(u64),
    /// Directory was created.
    DirCreated,
    /// Symlink was created.
    SymlinkCreated,
}

/// Summary of materializing multiple entries.
#[derive(Debug, Default)]
pub struct MaterializeSummary {
    pub reflinked: usize,
    pub copied: usize,
    pub dirs_created: usize,
    pub symlinks_created: usize,
    pub errors: Vec<(VirtualPath, AgentdirError)>,
}

/// Manages the on-disk materialized tree.
pub struct Materializer {
    /// Root directory where virtual files are materialized.
    pub materialized_root: PathBuf,
}

impl Materializer {
    /// Create a new materializer. Creates the root directory if it doesn't exist.
    pub fn new(root: PathBuf) -> Result<Self> {
        if !root.exists() {
            fs::create_dir_all(&root)?;
        }

        Ok(Self {
            materialized_root: root,
        })
    }

    /// Get the real on-disk path for a virtual path.
    pub fn materialized_path(&self, virtual_path: &VirtualPath) -> PathBuf {
        let rel = virtual_path.as_str().trim_start_matches('/');
        self.materialized_root.join(rel)
    }

    /// Materialize a single catalog entry.
    pub fn materialize_entry(&self, entry: &CatalogEntry) -> Result<MaterializeResult> {
        let dst = self.materialized_path(&entry.virtual_path);

        match &entry.metadata.entry_type {
            EntryType::File => {
                let src = entry.source_path.as_path();
                let result = reflink::clone_file(src, &dst)?;
                let materialize_result = match result {
                    CloneResult::Reflinked => MaterializeResult::Reflinked,
                    CloneResult::Copied(bytes) => MaterializeResult::Copied(bytes),
                };
                info!(?src, ?dst, "materialized file");
                Ok(materialize_result)
            }
            EntryType::Directory => {
                fs::create_dir_all(&dst)?;
                info!(?dst, "created materialized directory");
                Ok(MaterializeResult::DirCreated)
            }
            EntryType::Symlink { target } => {
                if dst.exists() || dst.symlink_metadata().is_ok() {
                    fs::remove_file(&dst)?;
                }

                if let Some(parent) = dst.parent() {
                    fs::create_dir_all(parent)?;
                }

                #[cfg(unix)]
                std::os::unix::fs::symlink(target, &dst)?;

                #[cfg(not(unix))]
                return Err(AgentdirError::ReflinkFailed(
                    "symlinks not supported on this platform".into(),
                ));

                info!(?dst, ?target, "created materialized symlink");
                Ok(MaterializeResult::SymlinkCreated)
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

    /// Refresh a materialized entry (dematerialize + re-materialize).
    pub fn refresh_entry(&self, entry: &CatalogEntry) -> Result<MaterializeResult> {
        self.dematerialize_entry(&entry.virtual_path)?;
        self.materialize_entry(entry)
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
                Ok(MaterializeResult::SymlinkCreated) => summary.symlinks_created += 1,
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
}
