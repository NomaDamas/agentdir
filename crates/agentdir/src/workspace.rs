//! Workspace — top-level API facade integrating Catalog, Materializer, Backend, and Manifest.
//!
//! This is the single entry point for all agentdir operations.
//! Every mutation saves the manifest atomically after success.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::backend::{Backend, LocalBackend};
use crate::catalog::Catalog;
use crate::error::{AgentdirError, Result};
use crate::manifest;
use crate::materializer::Materializer;
use crate::reconciler::{ReconcileSummary, Reconciler};
use crate::types::{CatalogEntry, SourcePath, SourceRoot, VirtualPath};

/// Summary of a map operation.
#[derive(Debug, Default)]
pub struct MapSummary {
    pub entries_added: usize,
    pub reflinked: usize,
    pub copied: usize,
    pub dirs_created: usize,
    pub errors: usize,
}

/// Summary of an unmap operation.
#[derive(Debug, Default)]
pub struct UnmapSummary {
    pub entries_removed: usize,
}

/// Status of the workspace.
#[derive(Debug)]
pub struct WorkspaceStatus {
    pub total_entries: usize,
    pub source_roots: usize,
    pub materialized_root: PathBuf,
    pub last_updated_epoch_secs: u64,
}

/// Top-level API for managing a virtual filesystem workspace.
pub struct Workspace {
    pub catalog: Catalog,
    pub materializer: Materializer,
    pub backend: Arc<dyn Backend>,
    pub manifest_path: PathBuf,
}

impl Workspace {
    /// Initialize a new workspace at the given root directory.
    ///
    /// Creates `.agentdir/manifest.json` and an empty catalog.
    pub fn init(workspace_root: PathBuf) -> Result<Self> {
        manifest::ensure_workspace_dir(&workspace_root)?;

        let catalog = Catalog::new(workspace_root.clone());
        let materializer = Materializer::new(workspace_root.clone())?;
        let backend: Arc<dyn Backend> = Arc::new(LocalBackend);
        let manifest_path = manifest::manifest_path(&workspace_root);

        manifest::save(&catalog.manifest, &manifest_path)?;

        Ok(Self {
            catalog,
            materializer,
            backend,
            manifest_path,
        })
    }

    /// Open an existing workspace from disk.
    pub fn open(workspace_root: PathBuf) -> Result<Self> {
        let manifest_path = manifest::manifest_path(&workspace_root);
        let loaded_manifest = manifest::load(&manifest_path)?;

        Ok(Self {
            catalog: Catalog::from_manifest(loaded_manifest, workspace_root.clone()),
            materializer: Materializer::new(workspace_root)?,
            backend: Arc::new(LocalBackend),
            manifest_path,
        })
    }

    /// Map a source directory into the virtual tree at the given mount point.
    ///
    /// Scans the source, adds entries to catalog, materializes all files.
    pub async fn map(&mut self, source: SourcePath, mount: VirtualPath) -> Result<MapSummary> {
        self.catalog.add_source_root(SourceRoot {
            source_path: source.clone(),
            virtual_mount: mount.clone(),
            recursive: true,
        })?;

        let scanned = self.backend.scan(&source).await?;
        let mut entries = Vec::with_capacity(scanned.len());

        for (path, metadata) in scanned {
            let virtual_path = virtual_path_for_source(&source, &mount, &path)?;
            entries.push(CatalogEntry {
                virtual_path,
                source_path: path,
                content_hash: None,
                metadata,
                materialized: false,
            });
        }

        let entries_added = entries.len();
        self.catalog.add_entries(entries.clone())?;

        let batch = self.materializer.materialize_batch(&entries, None, 50)?;

        for entry in &entries {
            if let Ok(catalog_entry) = self.catalog.get_mut(&entry.virtual_path) {
                catalog_entry.materialized = true;
            }
        }

        self.save()?;

        Ok(MapSummary {
            entries_added,
            reflinked: 0,
            copied: 0,
            dirs_created: 0,
            errors: batch.failed,
        })
    }

    /// Remove a source mapping from the virtual tree.
    pub fn unmap(&mut self, mount: &VirtualPath) -> Result<UnmapSummary> {
        let removed = self.catalog.unmap(mount)?;
        let entries_removed = removed.len();

        for entry in &removed {
            let _ = self.materializer.dematerialize_entry(&entry.virtual_path);
        }

        self.save()?;
        Ok(UnmapSummary { entries_removed })
    }

    /// Create a virtual directory.
    pub fn mkdir(&mut self, path: &VirtualPath) -> Result<()> {
        self.catalog.mkdir(path)?;

        let entry = self.catalog.get(path)?.clone();
        self.materializer.materialize_entry(&entry)?;

        self.save()
    }

    /// Remove a virtual directory.
    pub fn rmdir(&mut self, path: &VirtualPath, recursive: bool) -> Result<()> {
        self.materializer.dematerialize_entry(path)?;
        self.catalog.rmdir(path, recursive)?;
        self.save()
    }

    /// Move an entry in the virtual namespace.
    pub fn mv(&mut self, from: &VirtualPath, to: &VirtualPath) -> Result<()> {
        let from_path = self.materializer.materialized_path(from);
        let to_path = self.materializer.materialized_path(to);

        if let Some(parent) = to_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        if from_path.exists() || from_path.symlink_metadata().is_ok() {
            std::fs::rename(&from_path, &to_path)?;
        }

        self.catalog.mv(from, to)?;
        self.save()
    }

    /// Copy an entry in the virtual namespace.
    pub fn cp(&mut self, from: &VirtualPath, to: &VirtualPath) -> Result<()> {
        let mut new_entry = self.catalog.get(from)?.clone();
        new_entry.virtual_path = to.clone();
        new_entry.materialized = false;

        self.materializer.materialize_entry(&new_entry)?;
        self.catalog.cp(from, to)?;

        if let Ok(entry) = self.catalog.get_mut(to) {
            entry.materialized = true;
        }

        self.save()
    }

    /// Create a virtual symlink.
    pub fn ln(&mut self, target: &VirtualPath, link: &VirtualPath) -> Result<()> {
        self.catalog.ln(target, link)?;

        let link_entry = self.catalog.get(link)?.clone();
        self.materializer.materialize_entry(&link_entry)?;

        if let Ok(entry) = self.catalog.get_mut(link) {
            entry.materialized = true;
        }

        self.save()
    }

    /// Rename an entry in the virtual namespace.
    pub fn rename(&mut self, path: &VirtualPath, new_name: &str) -> Result<()> {
        let parent = path
            .parent()
            .ok_or_else(|| AgentdirError::InvalidPath("cannot rename root".into()))?;
        let separator = if parent.as_str() == "/" { "" } else { "/" };
        let new_path = VirtualPath::new(format!("{}{separator}{new_name}", parent.as_str()))?;
        self.mv(path, &new_path)
    }

    /// Run a full reconciliation — detect source changes and update the virtual tree.
    pub async fn refresh(&mut self) -> Result<ReconcileSummary> {
        let roots = self.catalog.source_roots().to_vec();
        let actions =
            Reconciler::full_reconcile(&self.catalog, self.backend.as_ref(), &roots).await?;
        let summary = Reconciler::apply_actions(&mut self.catalog, &self.materializer, &actions)?;

        self.save()?;
        Ok(summary)
    }

    /// Get workspace status.
    pub fn status(&self) -> WorkspaceStatus {
        WorkspaceStatus {
            total_entries: self.catalog.len(),
            source_roots: self.catalog.source_roots().len(),
            materialized_root: self.materializer.materialized_root.clone(),
            last_updated_epoch_secs: self.catalog.manifest.updated_at_epoch_secs,
        }
    }

    /// Save the manifest atomically.
    pub fn save(&self) -> Result<()> {
        manifest::save(&self.catalog.manifest, &self.manifest_path)
    }
}

fn virtual_path_for_source(
    source_root: &SourcePath,
    mount: &VirtualPath,
    source_path: &SourcePath,
) -> Result<VirtualPath> {
    let rel = source_path
        .as_path()
        .strip_prefix(source_root.as_path())
        .map_err(|_| AgentdirError::InvalidPath(format!("source path {source_path} outside root")))?;

    virtual_path_for_relative(mount, rel)
}

fn virtual_path_for_relative(mount: &VirtualPath, rel: &Path) -> Result<VirtualPath> {
    if rel.as_os_str().is_empty() {
        return Ok(mount.clone());
    }

    let separator = if mount.as_str() == "/" { "" } else { "/" };
    VirtualPath::new(format!("{}{separator}{}", mount.as_str(), rel.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_source_files(dir: &std::path::Path) {
        std::fs::write(dir.join("file1.txt"), b"hello").unwrap();
        std::fs::write(dir.join("file2.txt"), b"world").unwrap();
        std::fs::create_dir(dir.join("subdir")).unwrap();
        std::fs::write(dir.join("subdir/nested.txt"), b"nested").unwrap();
    }

    #[tokio::test]
    async fn test_init_creates_workspace() {
        let ws_dir = TempDir::new().unwrap();
        let ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();

        assert!(ws_dir.path().join(".agentdir/manifest.json").exists());
        assert_eq!(ws.catalog.len(), 0);
    }

    #[tokio::test]
    async fn test_init_map_verify() {
        let src_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();

        create_source_files(src_dir.path());

        let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
        let summary = ws
            .map(
                SourcePath::new(src_dir.path().to_path_buf()),
                VirtualPath::new("/docs").unwrap(),
            )
            .await
            .unwrap();

        assert!(summary.entries_added > 0);

        let mat_file = ws_dir.path().join("docs/file1.txt");
        assert!(mat_file.exists());
        assert_eq!(std::fs::read(&mat_file).unwrap(), b"hello");
    }

    #[tokio::test]
    async fn test_persist_reload() {
        let src_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();

        create_source_files(src_dir.path());

        {
            let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
            ws.map(
                SourcePath::new(src_dir.path().to_path_buf()),
                VirtualPath::new("/docs").unwrap(),
            )
            .await
            .unwrap();
        }

        let ws = Workspace::open(ws_dir.path().to_path_buf()).unwrap();
        assert!(ws.catalog.len() > 0);
        assert_eq!(ws.catalog.source_roots().len(), 1);
    }

    #[tokio::test]
    async fn test_refresh_source_modification() {
        let src_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();

        std::fs::write(src_dir.path().join("file.txt"), b"original").unwrap();

        let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
        ws.map(
            SourcePath::new(src_dir.path().to_path_buf()),
            VirtualPath::new("/docs").unwrap(),
        )
        .await
        .unwrap();

        let mat_file = ws_dir.path().join("docs/file.txt");
        assert_eq!(std::fs::read(&mat_file).unwrap(), b"original");

        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(src_dir.path().join("file.txt"), b"modified content").unwrap();

        ws.refresh().await.unwrap();

        assert_eq!(std::fs::read(&mat_file).unwrap(), b"modified content");
    }

    #[tokio::test]
    async fn test_mv_updates_virtual_path() {
        let src_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();

        std::fs::write(src_dir.path().join("file.txt"), b"content").unwrap();

        let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
        ws.map(
            SourcePath::new(src_dir.path().to_path_buf()),
            VirtualPath::new("/docs").unwrap(),
        )
        .await
        .unwrap();

        ws.mv(
            &VirtualPath::new("/docs/file.txt").unwrap(),
            &VirtualPath::new("/docs/renamed.txt").unwrap(),
        )
        .unwrap();

        assert!(!ws_dir.path().join("docs/file.txt").exists());
        assert!(ws_dir.path().join("docs/renamed.txt").exists());
        assert_eq!(
            std::fs::read(ws_dir.path().join("docs/renamed.txt")).unwrap(),
            b"content"
        );
    }

    #[tokio::test]
    async fn test_unmap_removes_entries() {
        let src_dir = TempDir::new().unwrap();
        let ws_dir = TempDir::new().unwrap();

        create_source_files(src_dir.path());

        let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
        ws.map(
            SourcePath::new(src_dir.path().to_path_buf()),
            VirtualPath::new("/docs").unwrap(),
        )
        .await
        .unwrap();

        let initial_count = ws.catalog.len();
        assert!(initial_count > 0);

        ws.unmap(&VirtualPath::new("/docs").unwrap()).unwrap();
        assert_eq!(ws.catalog.len(), 0);
    }
}
