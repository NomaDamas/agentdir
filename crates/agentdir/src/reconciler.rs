//! Change reconciler — converts SourceEvents into ChangeActions and applies them.
//!
//! One-way sync: source → virtual tree. No write-back. No conflict resolution.
//! Uses mtime+size for change detection (NOT sha256 — lazy hashing).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::backend::{Backend, SourceEvent};
use crate::catalog::Catalog;
use crate::error::{AgentdirError, Result};
use crate::materializer::Materializer;
use crate::reflink;
use crate::types::{
    CatalogEntry, ContentHash, EntryType, SourceMetadata, SourcePath, SourceRoot, VirtualPath,
};

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
        content_hash: Option<ContentHash>,
    },
    /// Metadata is unchanged, but lazy hash verification learned/stabilized the hash.
    UpdateHash {
        virtual_path: VirtualPath,
        content_hash: ContentHash,
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

struct RollbackBackup {
    root: PathBuf,
    entries: Vec<(CatalogEntry, Option<PathBuf>)>,
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
                    let metadata = metadata_for_path(path)?;
                    Ok(vec![ChangeAction::Add {
                        source: path.clone(),
                        virtual_path,
                        metadata,
                    }])
                })
                .unwrap_or_else(|| Ok(Vec::new())),
            SourceEvent::Modified { path } => {
                let entries = catalog.find_all_by_source(path);
                Ok(entries
                    .into_iter()
                    .map(|entry| {
                        metadata_for_path(path).map(|new_metadata| ChangeAction::Refresh {
                            virtual_path: entry.virtual_path.clone(),
                            source: path.clone(),
                            new_metadata,
                            content_hash: None,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?
                    .into_iter()
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
                        metadata: metadata_for_path(to)?,
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
        Self::full_reconcile_with_options(catalog, backend, roots, false).await
    }

    /// Full reconciliation with opt-in SHA-256 verification for unchanged mtime/size.
    pub async fn full_reconcile_with_options(
        catalog: &Catalog,
        backend: &dyn Backend,
        roots: &[SourceRoot],
        verify_hashes: bool,
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
                        let content_hash = if verify_hashes
                            && matches!(scanned_meta.entry_type, EntryType::File)
                        {
                            Some(reflink::compute_hash(source_path.as_path())?)
                        } else {
                            None
                        };
                        for entry in &existing {
                            actions.push(ChangeAction::Refresh {
                                virtual_path: entry.virtual_path.clone(),
                                source: source_path.clone(),
                                new_metadata: scanned_meta.clone(),
                                content_hash: content_hash.clone(),
                            });
                        }
                    } else if verify_hashes && matches!(scanned_meta.entry_type, EntryType::File) {
                        let scanned_hash = reflink::compute_hash(source_path.as_path())?;
                        if existing[0].content_hash.as_ref() != Some(&scanned_hash) {
                            for entry in &existing {
                                actions.push(ChangeAction::Refresh {
                                    virtual_path: entry.virtual_path.clone(),
                                    source: source_path.clone(),
                                    new_metadata: scanned_meta.clone(),
                                    content_hash: Some(scanned_hash.clone()),
                                });
                            }
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
        if let Err(errors) = Self::preflight_actions(catalog, actions) {
            summary.errors = errors;
            return Ok(summary);
        }

        let snapshot = catalog.clone();
        let mut touched_added = Vec::new();
        let backup = match Self::backup_snapshot_materialized(&snapshot, materializer) {
            Ok(backup) => backup,
            Err(errors) => {
                summary.errors = errors;
                return Ok(summary);
            }
        };

        for action in actions
            .iter()
            .filter(|action| !matches!(action, ChangeAction::Remove { .. }))
            .chain(
                actions
                    .iter()
                    .filter(|action| matches!(action, ChangeAction::Remove { .. })),
            )
        {
            let errors_before = summary.errors.len();
            match action {
                ChangeAction::Add {
                    source,
                    virtual_path,
                    metadata,
                } => {
                    Self::apply_add(
                        catalog,
                        materializer,
                        &mut summary,
                        source,
                        virtual_path,
                        metadata,
                    );
                    if summary.errors.len() == errors_before {
                        touched_added.push(virtual_path.clone());
                    }
                }
                ChangeAction::Remove { virtual_path } => {
                    Self::apply_remove(catalog, materializer, &mut summary, virtual_path);
                }
                ChangeAction::Refresh {
                    virtual_path,
                    source,
                    new_metadata,
                    content_hash,
                } => Self::apply_refresh(
                    catalog,
                    materializer,
                    &mut summary,
                    virtual_path,
                    source,
                    new_metadata,
                    content_hash.clone(),
                ),
                ChangeAction::UpdateHash {
                    virtual_path,
                    content_hash,
                } => {
                    if let Ok(entry) = catalog.get_mut(virtual_path) {
                        entry.content_hash = Some(content_hash.clone());
                    }
                }
            }

            if summary.errors.len() != errors_before {
                Self::rollback_to_snapshot(
                    catalog,
                    materializer,
                    &snapshot,
                    &touched_added,
                    &backup,
                );
                break;
            }
        }

        Self::cleanup_backup(&backup);
        Ok(summary)
    }

    fn backup_snapshot_materialized(
        snapshot: &Catalog,
        materializer: &Materializer,
    ) -> std::result::Result<RollbackBackup, Vec<(VirtualPath, AgentdirError)>> {
        let root = std::env::temp_dir().join(format!(
            "agentdir-rollback-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let mut errors = Vec::new();
        let mut entries = Vec::new();

        for entry in snapshot.entries().iter().filter(|entry| entry.materialized) {
            match entry.metadata.entry_type {
                EntryType::Directory => entries.push((entry.clone(), None)),
                EntryType::File => {
                    let source = materializer.materialized_path(&entry.virtual_path);
                    if !source.exists() {
                        entries.push((entry.clone(), None));
                        continue;
                    }
                    let backup_path =
                        root.join(entry.virtual_path.as_str().trim_start_matches('/'));
                    if let Some(parent) = backup_path.parent() {
                        if let Err(error) = fs::create_dir_all(parent) {
                            errors.push((entry.virtual_path.clone(), AgentdirError::Io(error)));
                            continue;
                        }
                    }
                    if let Err(error) = fs::copy(&source, &backup_path) {
                        errors.push((entry.virtual_path.clone(), AgentdirError::Io(error)));
                        continue;
                    }
                    entries.push((entry.clone(), Some(backup_path)));
                }
            }
        }

        if errors.is_empty() {
            Ok(RollbackBackup { root, entries })
        } else {
            let _ = fs::remove_dir_all(&root);
            Err(errors)
        }
    }

    fn rollback_to_snapshot(
        catalog: &mut Catalog,
        materializer: &Materializer,
        snapshot: &Catalog,
        touched_added: &[VirtualPath],
        backup: &RollbackBackup,
    ) {
        for path in touched_added {
            let _ = materializer.dematerialize_entry(path);
        }

        for entry in snapshot.entries().iter().rev() {
            if entry.virtual_path.as_str() == "/" {
                continue;
            }
            let _ = materializer.dematerialize_entry(&entry.virtual_path);
        }

        for (entry, backup_path) in &backup.entries {
            let dst = materializer.materialized_path(&entry.virtual_path);
            match entry.metadata.entry_type {
                EntryType::Directory => {
                    let _ = fs::create_dir_all(&dst);
                }
                EntryType::File => {
                    if let Some(parent) = dst.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    if let Some(backup_path) = backup_path {
                        let _ = fs::copy(backup_path, &dst);
                    }
                }
            }
        }

        *catalog = snapshot.clone();
        Self::cleanup_backup(backup);
    }

    fn cleanup_backup(backup: &RollbackBackup) {
        let _ = fs::remove_dir_all(&backup.root);
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

    fn preflight_actions(
        catalog: &Catalog,
        actions: &[ChangeAction],
    ) -> std::result::Result<(), Vec<(VirtualPath, AgentdirError)>> {
        let mut errors = Vec::new();

        let mut add_targets: HashMap<String, VirtualPath> = HashMap::new();

        for action in actions {
            match action {
                ChangeAction::Add {
                    source,
                    virtual_path,
                    ..
                } => {
                    let key = virtual_path.as_str().to_string();
                    if catalog.get(virtual_path).is_ok()
                        || add_targets
                            .insert(key.clone(), virtual_path.clone())
                            .is_some()
                    {
                        errors.push((virtual_path.clone(), AgentdirError::EntryExists(key)));
                    }
                    if !source.as_path().exists() {
                        errors.push((
                            virtual_path.clone(),
                            AgentdirError::Io(std::io::Error::new(
                                std::io::ErrorKind::NotFound,
                                format!("source {} disappeared before apply", source),
                            )),
                        ));
                    }
                }
                ChangeAction::Refresh {
                    virtual_path,
                    source,
                    ..
                } => {
                    if catalog.get(virtual_path).is_err() {
                        errors.push((
                            virtual_path.clone(),
                            AgentdirError::EntryNotFound(virtual_path.as_str().to_string()),
                        ));
                    }
                    if !source.as_path().exists() {
                        errors.push((
                            virtual_path.clone(),
                            AgentdirError::Io(std::io::Error::new(
                                std::io::ErrorKind::NotFound,
                                format!("source {} disappeared before apply", source),
                            )),
                        ));
                    }
                }
                ChangeAction::Remove { virtual_path }
                | ChangeAction::UpdateHash { virtual_path, .. } => {
                    if catalog.get(virtual_path).is_err() {
                        errors.push((
                            virtual_path.clone(),
                            AgentdirError::EntryNotFound(virtual_path.as_str().to_string()),
                        ));
                    }
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn apply_add(
        catalog: &mut Catalog,
        materializer: &Materializer,
        summary: &mut ReconcileSummary,
        source: &SourcePath,
        virtual_path: &VirtualPath,
        metadata: &SourceMetadata,
    ) {
        let mut entry = CatalogEntry {
            virtual_path: virtual_path.clone(),
            source_path: source.clone(),
            content_hash: None,
            metadata: metadata.clone(),
            materialized: false,
        };

        match materializer.materialize_entry(&entry) {
            Ok(_) => entry.materialized = true,
            Err(error) => {
                summary.errors.push((virtual_path.clone(), error));
                return;
            }
        }

        if let Err(error) = catalog.add_entries(vec![entry]) {
            let _ = materializer.dematerialize_entry(virtual_path);
            summary.errors.push((virtual_path.clone(), error));
            return;
        }

        summary.added += 1;
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
        content_hash: Option<ContentHash>,
    ) {
        match catalog.get(virtual_path) {
            Ok(entry) => {
                let mut refreshed = entry.clone();
                refreshed.metadata = new_metadata.clone();
                refreshed.content_hash = content_hash;
                refreshed.materialized = true;

                match materializer.refresh_entry(&refreshed) {
                    Ok(_) => {
                        if let Ok(entry) = catalog.get_mut(virtual_path) {
                            entry.metadata = refreshed.metadata;
                            entry.content_hash = refreshed.content_hash;
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

fn metadata_for_path(path: &SourcePath) -> Result<SourceMetadata> {
    let metadata = match std::fs::symlink_metadata(path.as_path()) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SourceMetadata {
                mtime_ns: 0,
                size_bytes: 0,
                entry_type: EntryType::File,
            });
        }
        Err(error) => return Err(AgentdirError::Io(error)),
    };
    let mtime_ns = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let entry_type = if metadata.is_dir() {
        EntryType::Directory
    } else {
        EntryType::File
    };
    Ok(SourceMetadata {
        mtime_ns,
        size_bytes: metadata.len(),
        entry_type,
    })
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
