//! Change reconciler — converts SourceEvents into ChangeActions and applies them.
//!
//! One-way sync: source → virtual tree. No write-back. No conflict resolution.
//! Uses mtime+size for change detection (NOT sha256 — lazy hashing).

use std::collections::HashSet;
use std::path::Path;

use crate::backend::{Backend, SourceEvent};
use crate::catalog::Catalog;
use crate::error::{AgentdirError, Result};
use crate::materializer::Materializer;
use crate::types::{CatalogEntry, EntryType, SourceMetadata, SourcePath, SourceRoot, VirtualPath};

/// An action to apply to the catalog and materialized tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeAction {
    /// A new file appeared in a source root — add to catalog and materialize.
    Add {
        source: SourcePath,
        virtual_path: VirtualPath,
        metadata: SourceMetadata,
    },
    /// A file was deleted from a source root — remove from catalog and dematerialize.
    Remove { virtual_path: VirtualPath },
    /// A file was modified — update catalog metadata and refresh materialization.
    Refresh {
        virtual_path: VirtualPath,
        source: SourcePath,
        new_metadata: SourceMetadata,
    },
}

/// Summary of applying a set of change actions.
#[derive(Debug, Default)]
pub struct ReconcileSummary {
    pub added: usize,
    pub removed: usize,
    pub refreshed: usize,
    pub errors: Vec<(VirtualPath, AgentdirError)>,
}

/// Converts SourceEvents to ChangeActions and applies them.
pub struct Reconciler;

impl Reconciler {
    /// Convert a single SourceEvent into ChangeActions.
    ///
    /// For RescanNeeded, returns empty vec — caller should call full_reconcile instead.
    pub fn from_event(catalog: &Catalog, event: &SourceEvent) -> Result<Vec<ChangeAction>> {
        match event {
            SourceEvent::Created { path } => Self::source_to_virtual(catalog, path)
                .map(|virtual_path| {
                    vec![ChangeAction::Add {
                        source: path.clone(),
                        virtual_path,
                        metadata: placeholder_file_metadata(),
                    }]
                })
                .map_or_else(|| Ok(Vec::new()), Ok),
            SourceEvent::Modified { path } => {
                let entries = catalog.find_all_by_source(path);
                Ok(entries
                    .into_iter()
                    .map(|entry| ChangeAction::Refresh {
                        virtual_path: entry.virtual_path.clone(),
                        source: path.clone(),
                        new_metadata: SourceMetadata {
                            mtime_ns: 0,
                            size_bytes: 0,
                            entry_type: entry.metadata.entry_type.clone(),
                        },
                    })
                    .collect())
            }
            SourceEvent::Deleted { path } => {
                let entries = catalog.find_all_by_source(path);
                Ok(entries
                    .into_iter()
                    .map(|entry| ChangeAction::Remove {
                        virtual_path: entry.virtual_path.clone(),
                    })
                    .collect())
            }
            SourceEvent::Renamed { from, to } => {
                let mut actions = Vec::new();

                for entry in catalog.find_all_by_source(from) {
                    actions.push(ChangeAction::Remove {
                        virtual_path: entry.virtual_path.clone(),
                    });
                }

                if let Some(virtual_path) = Self::source_to_virtual(catalog, to) {
                    actions.push(ChangeAction::Add {
                        source: to.clone(),
                        virtual_path,
                        metadata: placeholder_file_metadata(),
                    });
                }

                Ok(actions)
            }
            SourceEvent::RescanNeeded => Ok(Vec::new()),
        }
    }

    /// Full reconciliation: scan all source roots and diff against catalog.
    ///
    /// Uses mtime+size for change detection (NOT sha256).
    pub async fn full_reconcile(
        catalog: &Catalog,
        backend: &dyn Backend,
        roots: &[SourceRoot],
    ) -> Result<Vec<ChangeAction>> {
        let mut actions = Vec::new();

        for root in roots {
            let scanned = backend.scan(&root.source_path).await?;
            let scanned_paths = scanned
                .iter()
                .map(|(path, _)| path.as_path().to_path_buf())
                .collect::<HashSet<_>>();

            for (source_path, scanned_meta) in &scanned {
                if matches!(scanned_meta.entry_type, EntryType::Directory) {
                    continue;
                }

                let existing = catalog.find_all_by_source(source_path);
                if !existing.is_empty() {
                    if metadata_changed(&existing[0].metadata, scanned_meta) {
                        for entry in &existing {
                            actions.push(ChangeAction::Refresh {
                                virtual_path: entry.virtual_path.clone(),
                                source: source_path.clone(),
                                new_metadata: scanned_meta.clone(),
                            });
                        }
                    }
                } else if let Some(virtual_path) = Self::source_to_virtual(catalog, source_path) {
                    actions.push(ChangeAction::Add {
                        source: source_path.clone(),
                        virtual_path,
                        metadata: scanned_meta.clone(),
                    });
                }
            }

            for entry in catalog.entries() {
                if entry
                    .source_path
                    .as_path()
                    .starts_with(root.source_path.as_path())
                    && !scanned_paths.contains(entry.source_path.as_path())
                {
                    actions.push(ChangeAction::Remove {
                        virtual_path: entry.virtual_path.clone(),
                    });
                }
            }
        }

        Ok(actions)
    }

    /// Apply a list of ChangeActions to the catalog and materialized tree.
    pub fn apply_actions(
        catalog: &mut Catalog,
        materializer: &Materializer,
        actions: &[ChangeAction],
    ) -> Result<ReconcileSummary> {
        let mut summary = ReconcileSummary::default();

        for action in actions {
            match action {
                ChangeAction::Add {
                    source,
                    virtual_path,
                    metadata,
                } => Self::apply_add(
                    catalog,
                    materializer,
                    &mut summary,
                    source,
                    virtual_path,
                    metadata,
                ),
                ChangeAction::Remove { virtual_path } => {
                    Self::apply_remove(catalog, materializer, &mut summary, virtual_path);
                }
                ChangeAction::Refresh {
                    virtual_path,
                    source,
                    new_metadata,
                } => Self::apply_refresh(
                    catalog,
                    materializer,
                    &mut summary,
                    virtual_path,
                    source,
                    new_metadata,
                ),
            }
        }

        Ok(summary)
    }

    /// Compute the virtual path for a source path given the catalog's source roots.
    fn source_to_virtual(catalog: &Catalog, source: &SourcePath) -> Option<VirtualPath> {
        catalog.source_roots().iter().find_map(|root| {
            let rel = source
                .as_path()
                .strip_prefix(root.source_path.as_path())
                .ok()?;
            virtual_path_for_relative(&root.virtual_mount, rel).ok()
        })
    }

    fn apply_add(
        catalog: &mut Catalog,
        materializer: &Materializer,
        summary: &mut ReconcileSummary,
        source: &SourcePath,
        virtual_path: &VirtualPath,
        metadata: &SourceMetadata,
    ) {
        let entry = CatalogEntry {
            virtual_path: virtual_path.clone(),
            source_path: source.clone(),
            content_hash: None,
            metadata: metadata.clone(),
            materialized: false,
        };

        if let Err(error) = catalog.add_entries(vec![entry.clone()]) {
            summary.errors.push((virtual_path.clone(), error));
            return;
        }

        match materializer.materialize_entry(&entry) {
            Ok(_) => {
                if let Ok(entry) = catalog.get_mut(virtual_path) {
                    entry.materialized = true;
                }
                summary.added += 1;
            }
            Err(error) => summary.errors.push((virtual_path.clone(), error)),
        }
    }

    fn apply_remove(
        catalog: &mut Catalog,
        materializer: &Materializer,
        summary: &mut ReconcileSummary,
        virtual_path: &VirtualPath,
    ) {
        if let Err(error) = materializer.dematerialize_entry(virtual_path) {
            summary.errors.push((virtual_path.clone(), error));
            return;
        }

        if let Err(error) = catalog.unmap(virtual_path) {
            summary.errors.push((virtual_path.clone(), error));
            return;
        }

        summary.removed += 1;
    }

    fn apply_refresh(
        catalog: &mut Catalog,
        materializer: &Materializer,
        summary: &mut ReconcileSummary,
        virtual_path: &VirtualPath,
        source: &SourcePath,
        new_metadata: &SourceMetadata,
    ) {
        match catalog.get_mut(virtual_path) {
            Ok(entry) => {
                entry.metadata = new_metadata.clone();
                entry.content_hash = None;
                let entry = entry.clone();

                match materializer.refresh_entry(&entry) {
                    Ok(_) => {
                        if let Ok(entry) = catalog.get_mut(virtual_path) {
                            entry.materialized = true;
                        }
                        summary.refreshed += 1;
                    }
                    Err(error) => summary.errors.push((virtual_path.clone(), error)),
                }
            }
            Err(_) => {
                Self::apply_add(
                    catalog,
                    materializer,
                    summary,
                    source,
                    virtual_path,
                    new_metadata,
                );
            }
        }
    }
}

fn placeholder_file_metadata() -> SourceMetadata {
    SourceMetadata {
        mtime_ns: 0,
        size_bytes: 0,
        entry_type: EntryType::File,
    }
}

fn metadata_changed(current: &SourceMetadata, scanned: &SourceMetadata) -> bool {
    current.mtime_ns != scanned.mtime_ns || current.size_bytes != scanned.size_bytes
}

fn virtual_path_for_relative(mount: &VirtualPath, rel: &Path) -> Result<VirtualPath> {
    if rel.as_os_str().is_empty() {
        return Ok(mount.clone());
    }

    // Normalize path separators: use component iteration instead of display()
    // to ensure forward slashes on all platforms (Windows display() emits backslashes).
    let rel_str: String = rel
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => Some(s.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/");

    let separator = if mount.as_str() == "/" { "" } else { "/" };
    VirtualPath::new(format!("{}{separator}{rel_str}", mount.as_str()))
}

#[cfg(test)]
mod tests {
    use crate::backend::{local::LocalBackend, SourceEvent};
    use crate::catalog::Catalog;
    use crate::materializer::Materializer;
    use crate::types::{
        CatalogEntry, EntryType, SourceMetadata, SourcePath, SourceRoot, VirtualPath,
    };
    use tempfile::TempDir;

    use super::*;

    fn setup_catalog_with_root(source_dir: &std::path::Path, mat_dir: &std::path::Path) -> Catalog {
        let mut catalog = Catalog::new(mat_dir.to_path_buf());
        let root = SourceRoot {
            source_path: SourcePath::new(source_dir.to_path_buf()),
            virtual_mount: VirtualPath::new("/docs").unwrap(),
            recursive: true,
        };
        catalog.add_source_root(root).unwrap();
        catalog
    }

    fn make_entry(source_path: std::path::PathBuf, virtual_path: &str) -> CatalogEntry {
        CatalogEntry {
            virtual_path: VirtualPath::new(virtual_path).unwrap(),
            source_path: SourcePath::new(source_path),
            content_hash: None,
            metadata: SourceMetadata {
                mtime_ns: 1000,
                size_bytes: 7,
                entry_type: EntryType::File,
            },
            materialized: false,
        }
    }

    #[test]
    fn test_from_event_created_produces_add() {
        let src_dir = TempDir::new().unwrap();
        let mat_dir = TempDir::new().unwrap();
        let catalog = setup_catalog_with_root(src_dir.path(), mat_dir.path());

        let new_file = src_dir.path().join("newfile.txt");
        let event = SourceEvent::Created {
            path: SourcePath::new(new_file),
        };

        let actions = Reconciler::from_event(&catalog, &event).unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], ChangeAction::Add { .. }));
    }

    #[test]
    fn test_from_event_deleted_produces_remove() {
        let src_dir = TempDir::new().unwrap();
        let mat_dir = TempDir::new().unwrap();
        let mut catalog = setup_catalog_with_root(src_dir.path(), mat_dir.path());

        let src_file = src_dir.path().join("file.txt");
        std::fs::write(&src_file, b"content").unwrap();
        catalog
            .add_entries(vec![make_entry(src_file.clone(), "/docs/file.txt")])
            .unwrap();

        let event = SourceEvent::Deleted {
            path: SourcePath::new(src_file),
        };

        let actions = Reconciler::from_event(&catalog, &event).unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], ChangeAction::Remove { .. }));
    }

    #[test]
    fn test_from_event_modified_produces_refresh() {
        let src_dir = TempDir::new().unwrap();
        let mat_dir = TempDir::new().unwrap();
        let mut catalog = setup_catalog_with_root(src_dir.path(), mat_dir.path());

        let src_file = src_dir.path().join("file.txt");
        std::fs::write(&src_file, b"content").unwrap();
        catalog
            .add_entries(vec![make_entry(src_file.clone(), "/docs/file.txt")])
            .unwrap();

        let event = SourceEvent::Modified {
            path: SourcePath::new(src_file),
        };

        let actions = Reconciler::from_event(&catalog, &event).unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], ChangeAction::Refresh { .. }));
    }

    #[test]
    fn test_from_event_modified_refreshes_all_copies() {
        let src_dir = TempDir::new().unwrap();
        let mat_dir = TempDir::new().unwrap();
        let mut catalog = setup_catalog_with_root(src_dir.path(), mat_dir.path());

        let src_file = src_dir.path().join("file.txt");
        std::fs::write(&src_file, b"content").unwrap();
        catalog
            .add_entries(vec![
                make_entry(src_file.clone(), "/docs/file.txt"),
                make_entry(src_file.clone(), "/backup/file.txt"),
            ])
            .unwrap();

        let event = SourceEvent::Modified {
            path: SourcePath::new(src_file),
        };

        let actions = Reconciler::from_event(&catalog, &event).unwrap();
        assert_eq!(actions.len(), 2);
        assert_eq!(
            actions
                .iter()
                .filter(|action| matches!(action, ChangeAction::Refresh { .. }))
                .count(),
            2
        );
    }

    #[tokio::test]
    async fn test_full_reconcile_detects_new_file() {
        let src_dir = TempDir::new().unwrap();
        let mat_dir = TempDir::new().unwrap();
        let catalog = setup_catalog_with_root(src_dir.path(), mat_dir.path());

        std::fs::write(src_dir.path().join("new.txt"), b"new content").unwrap();

        let backend = LocalBackend;
        let roots = catalog.source_roots().to_vec();
        let actions = Reconciler::full_reconcile(&catalog, &backend, &roots)
            .await
            .unwrap();

        assert!(!actions.is_empty());
        assert!(actions
            .iter()
            .any(|a| matches!(a, ChangeAction::Add { .. })));
    }

    #[tokio::test]
    async fn test_full_reconcile_detects_mtime_size_change() {
        let src_dir = TempDir::new().unwrap();
        let mat_dir = TempDir::new().unwrap();
        let mut catalog = setup_catalog_with_root(src_dir.path(), mat_dir.path());

        let src_file = src_dir.path().join("file.txt");
        std::fs::write(&src_file, b"changed content").unwrap();
        catalog
            .add_entries(vec![make_entry(src_file, "/docs/file.txt")])
            .unwrap();

        let backend = LocalBackend;
        let roots = catalog.source_roots().to_vec();
        let actions = Reconciler::full_reconcile(&catalog, &backend, &roots)
            .await
            .unwrap();

        assert!(actions
            .iter()
            .any(|a| matches!(a, ChangeAction::Refresh { .. })));
    }

    #[tokio::test]
    async fn test_full_reconcile_refreshes_all_copies() {
        let src_dir = TempDir::new().unwrap();
        let mat_dir = TempDir::new().unwrap();
        let mut catalog = setup_catalog_with_root(src_dir.path(), mat_dir.path());

        let src_file = src_dir.path().join("file.txt");
        std::fs::write(&src_file, b"changed content").unwrap();
        catalog
            .add_entries(vec![
                make_entry(src_file.clone(), "/docs/file.txt"),
                make_entry(src_file, "/backup/file.txt"),
            ])
            .unwrap();

        let backend = LocalBackend;
        let roots = catalog.source_roots().to_vec();
        let actions = Reconciler::full_reconcile(&catalog, &backend, &roots)
            .await
            .unwrap();

        assert_eq!(
            actions
                .iter()
                .filter(|action| matches!(action, ChangeAction::Refresh { .. }))
                .count(),
            2
        );
    }

    #[tokio::test]
    async fn test_full_reconcile_detects_removed_file() {
        let src_dir = TempDir::new().unwrap();
        let mat_dir = TempDir::new().unwrap();
        let mut catalog = setup_catalog_with_root(src_dir.path(), mat_dir.path());

        let missing_file = src_dir.path().join("missing.txt");
        catalog
            .add_entries(vec![make_entry(missing_file, "/docs/missing.txt")])
            .unwrap();

        let backend = LocalBackend;
        let roots = catalog.source_roots().to_vec();
        let actions = Reconciler::full_reconcile(&catalog, &backend, &roots)
            .await
            .unwrap();

        assert!(actions
            .iter()
            .any(|a| matches!(a, ChangeAction::Remove { .. })));
    }

    #[test]
    fn test_apply_actions_add_creates_file() {
        let src_dir = TempDir::new().unwrap();
        let mat_dir = TempDir::new().unwrap();
        let mut catalog = setup_catalog_with_root(src_dir.path(), mat_dir.path());
        let materializer = Materializer::new(mat_dir.path().to_path_buf()).unwrap();

        let src_file = src_dir.path().join("file.txt");
        std::fs::write(&src_file, b"hello").unwrap();

        let actions = vec![ChangeAction::Add {
            source: SourcePath::new(src_file),
            virtual_path: VirtualPath::new("/docs/file.txt").unwrap(),
            metadata: SourceMetadata {
                mtime_ns: 1000,
                size_bytes: 5,
                entry_type: EntryType::File,
            },
        }];

        let summary = Reconciler::apply_actions(&mut catalog, &materializer, &actions).unwrap();
        assert_eq!(summary.added, 1);
        assert_eq!(summary.errors.len(), 0);

        let virtual_path = VirtualPath::new("/docs/file.txt").unwrap();
        let mat_path = materializer.materialized_path(&virtual_path);
        assert!(mat_path.exists());
        assert_eq!(std::fs::read(&mat_path).unwrap(), b"hello");
        assert!(catalog.get(&virtual_path).is_ok());
        assert!(catalog.get(&virtual_path).unwrap().materialized);
    }
}
