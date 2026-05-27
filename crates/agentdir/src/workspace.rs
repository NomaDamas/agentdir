//! Workspace — top-level API facade integrating Catalog, Materializer, Backend, and Manifest.
//!
//! This is the single entry point for all agentdir operations.
//! Every mutation saves the manifest atomically after success.
//!
//! Consistency model: reconciliation preflights every action before mutating the
//! catalog, then applies filesystem work before catalog mutations where possible.
//! If a source file disappears or a destination is invalid between scan and apply,
//! the refresh reports errors and leaves the previously persisted catalog intact.
//!
//! Thread-safety model: `Workspace` methods require `&mut self` for mutation.
//! Multi-task consumers should share a workspace as [`SharedWorkspace`], a
//! `tokio::sync::RwLock` wrapper that serializes mutations while allowing
//! read-side access through explicit read guards.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use globset::GlobBuilder;
use tokio::sync::RwLock;

use crate::backend::{Backend, LocalBackend};
use crate::catalog::Catalog;
use crate::error::{AgentdirError, Result};
use crate::manifest;
use crate::materializer::Materializer;
use crate::reconciler::{ReconcileSummary, Reconciler};
use crate::types::{
    CatalogEntry, EntryType, MappingDirection, SourcePath, SourceRoot, VirtualPath, VirtualStat,
};

/// Shared workspace handle for concurrent async consumers.
pub type SharedWorkspace = Arc<RwLock<Workspace>>;

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

    /// Wrap this workspace in an async RwLock for concurrent consumers.
    pub fn into_shared(self) -> SharedWorkspace {
        Arc::new(RwLock::new(self))
    }

    /// Map a source directory into the virtual tree at the given mount point.
    ///
    /// Scans the source, adds entries to catalog, materializes all files.
    pub async fn map(&mut self, source: SourcePath, mount: VirtualPath) -> Result<MapSummary> {
        validate_absolute_virtual_path(&mount, "mount point")?;
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

        let batch_result = self.materializer.materialize_batch(&entries, None, 50)?;

        let failed_paths: std::collections::HashSet<_> = batch_result
            .errors
            .iter()
            .map(|(vp, _)| vp.as_str().to_string())
            .collect();

        for entry in &entries {
            if !failed_paths.contains(entry.virtual_path.as_str()) {
                if let Ok(catalog_entry) = self.catalog.get_mut(&entry.virtual_path) {
                    catalog_entry.materialized = true;
                }
            }
        }

        self.save()?;

        Ok(MapSummary {
            entries_added,
            reflinked: batch_result.reflinked,
            copied: batch_result.copied,
            dirs_created: batch_result.dirs_created,
            errors: batch_result.failed,
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
        let subtree = self.catalog.entries_under(path)?;
        if !recursive && subtree.len() > 1 {
            return Err(AgentdirError::EntryExists(format!(
                "directory {path} is not empty"
            )));
        }

        if recursive {
            let paths: Vec<_> = subtree
                .into_iter()
                .map(|entry| entry.virtual_path)
                .collect();
            let result = self.materializer.dematerialize_batch(&paths)?;
            if let Some((failed_path, error)) = result.errors.into_iter().next() {
                return Err(AgentdirError::Io(std::io::Error::other(format!(
                    "failed to dematerialize {failed_path}: {error}"
                ))));
            }
        } else {
            self.materializer.dematerialize_entry(path)?;
        }
        self.catalog.rmdir(path, recursive)?;
        self.save()
    }

    /// Move an entry in the virtual namespace.
    pub fn mv(&mut self, from: &VirtualPath, to: &VirtualPath) -> Result<()> {
        self.preflight_rebase(from, to, true)?;

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
        let new_entries = self.preflight_rebase(from, to, false)?;

        let mut materialized = Vec::new();
        for entry in &new_entries {
            match self.materializer.materialize_entry(entry) {
                Ok(_) => materialized.push(entry.virtual_path.clone()),
                Err(error) => {
                    let _ = self.materializer.dematerialize_batch(&materialized);
                    return Err(error);
                }
            }
        }

        if let Err(error) = self.catalog.cp(from, to) {
            let _ = self.materializer.dematerialize_batch(&materialized);
            return Err(error);
        }

        for entry in &new_entries {
            if let Ok(catalog_entry) = self.catalog.get_mut(&entry.virtual_path) {
                catalog_entry.materialized = true;
            }
        }

        self.save()
    }

    fn preflight_rebase(
        &self,
        from: &VirtualPath,
        to: &VirtualPath,
        is_move: bool,
    ) -> Result<Vec<CatalogEntry>> {
        let source_entries = self.catalog.entries_under(from)?;
        let source_paths: std::collections::HashSet<_> = source_entries
            .iter()
            .map(|entry| entry.virtual_path.as_str().to_string())
            .collect();
        let mut seen = std::collections::HashSet::new();
        let mut new_entries = Vec::with_capacity(source_entries.len());

        for entry in &source_entries {
            let mut new_entry = entry.clone();
            new_entry.virtual_path = rebase_virtual_path(&entry.virtual_path, from, to)?;
            new_entry.materialized = false;
            let key = new_entry.virtual_path.as_str().to_string();

            if !seen.insert(key.clone()) {
                return Err(AgentdirError::EntryExists(key));
            }

            let destination_owned_by_move_source = is_move && source_paths.contains(&key);
            if !destination_owned_by_move_source
                && self.catalog.get(&new_entry.virtual_path).is_ok()
            {
                return Err(AgentdirError::EntryExists(key));
            }

            let materialized = self.materializer.materialized_path(&new_entry.virtual_path);
            if !destination_owned_by_move_source
                && (materialized.exists() || materialized.symlink_metadata().is_ok())
            {
                return Err(AgentdirError::EntryExists(
                    new_entry.virtual_path.as_str().to_string(),
                ));
            }

            new_entries.push(new_entry);
        }

        Ok(new_entries)
    }

    /// Rename an entry in the virtual namespace.
    pub fn rename(&mut self, path: &VirtualPath, new_name: &str) -> Result<()> {
        validate_new_name(new_name)?;
        let parent = path
            .parent()
            .ok_or_else(|| AgentdirError::InvalidPath("cannot rename root".into()))?;
        let separator = if parent.as_str() == "/" { "" } else { "/" };
        let new_path = VirtualPath::new(format!("{}{separator}{new_name}", parent.as_str()))?;
        self.mv(path, &new_path)
    }

    /// Run a full reconciliation — detect source changes and update the virtual tree.
    pub async fn refresh(&mut self) -> Result<ReconcileSummary> {
        self.refresh_with_hash_verification(false).await
    }

    /// Run reconciliation with optional SHA-256 verification for unchanged mtime/size.
    pub async fn refresh_with_hash_verification(
        &mut self,
        verify_hashes: bool,
    ) -> Result<ReconcileSummary> {
        let roots = self.catalog.source_roots().to_vec();
        let actions = Reconciler::full_reconcile_with_options(
            &self.catalog,
            self.backend.as_ref(),
            &roots,
            verify_hashes,
        )
        .await?;
        let summary = Reconciler::apply_actions(&mut self.catalog, &self.materializer, &actions)?;

        if summary.errors.is_empty() {
            self.save()?;
        }
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

    /// Return true if a virtual path exists in the catalog.
    pub fn exists(&self, path: &VirtualPath) -> bool {
        self.catalog.get(path).is_ok()
    }

    /// Return metadata for a virtual catalog entry.
    pub fn stat(&self, path: &VirtualPath) -> Result<VirtualStat> {
        let entry = self.catalog.get(path)?;
        Ok(VirtualStat {
            virtual_path: entry.virtual_path.clone(),
            source_path: entry.source_path.clone(),
            size_bytes: entry.metadata.size_bytes,
            mtime_ns: entry.metadata.mtime_ns,
            entry_type: entry.metadata.entry_type.clone(),
            materialized: entry.materialized,
        })
    }

    /// Read bytes from the source file behind a virtual path.
    pub async fn read_bytes(&self, path: &VirtualPath) -> Result<Vec<u8>> {
        let entry = self.catalog.get(path)?;
        if !matches!(entry.metadata.entry_type, EntryType::File) {
            return Err(AgentdirError::InvalidPath(format!(
                "cannot read directory {path}"
            )));
        }

        let source_path = self.catalog.resolve(path)?;
        self.backend.read_bytes(source_path).await
    }

    /// Match catalog entries by virtual path using a glob pattern.
    pub fn rglob(&self, pattern: &str) -> Result<Vec<&CatalogEntry>> {
        let glob = GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()
            .map_err(|e| AgentdirError::InvalidPath(format!("invalid glob pattern: {e}")))?
            .compile_matcher();

        Ok(self
            .catalog
            .entries()
            .iter()
            .filter(|entry| glob.is_match(entry.virtual_path.as_str()))
            .collect())
    }

    pub fn export_mapping(
        &self,
        direction: MappingDirection,
        relative_to: Option<&Path>,
    ) -> Result<BTreeMap<String, String>> {
        let canonical_base = match relative_to {
            Some(base) => Some(base.canonicalize().map_err(|e| {
                AgentdirError::InvalidPath(format!(
                    "cannot canonicalize relative_to base {:?}: {e}",
                    base
                ))
            })?),
            None => None,
        };

        let mut map = BTreeMap::new();
        for e in self.catalog.entries() {
            if !matches!(e.metadata.entry_type, EntryType::File) {
                continue;
            }

            let source_str = match &canonical_base {
                Some(base) => e
                    .source_path
                    .as_path()
                    .strip_prefix(base)
                    .map_err(|_| {
                        AgentdirError::InvalidPath(format!(
                            "source path {} is not under base {:?}",
                            e.source_path, base
                        ))
                    })?
                    .to_string_lossy()
                    .into_owned(),
                None => e.source_path.as_path().to_string_lossy().into_owned(),
            };
            let virtual_str = e.virtual_path.as_str().to_string();

            let (key, value) = match direction {
                MappingDirection::SourceToVirtual => (source_str, virtual_str),
                MappingDirection::VirtualToSource => (virtual_str, source_str),
            };

            if let Some(existing) = map.insert(key.clone(), value) {
                if direction == MappingDirection::SourceToVirtual {
                    return Err(AgentdirError::EntryExists(format!(
                        "duplicate source path {key}: maps to both {existing} and the current entry"
                    )));
                }
            }
        }

        Ok(map)
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
        .map_err(|_| {
            AgentdirError::InvalidPath(format!("source path {source_path} outside root"))
        })?;

    virtual_path_for_relative(mount, rel)
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

    #[test]
    fn test_virtual_path_for_relative_normalizes_separators() {
        let mount = VirtualPath::new("/docs").unwrap();
        let rel = Path::new("sub").join("file.txt");
        let result = virtual_path_for_relative(&mount, &rel).unwrap();
        assert_eq!(result.as_str(), "/docs/sub/file.txt");
    }

    #[test]
    fn test_virtual_path_for_relative_root_mount() {
        let mount = VirtualPath::new("/").unwrap();
        let rel = Path::new("dir").join("file.txt");
        let result = virtual_path_for_relative(&mount, &rel).unwrap();
        assert_eq!(result.as_str(), "/dir/file.txt");
    }

    #[test]
    fn test_virtual_path_for_relative_empty_rel() {
        let mount = VirtualPath::new("/docs").unwrap();
        let rel = Path::new("");
        let result = virtual_path_for_relative(&mount, rel).unwrap();
        assert_eq!(result.as_str(), "/docs");
    }
}

fn validate_absolute_virtual_path(path: &VirtualPath, label: &str) -> Result<()> {
    if !path.is_absolute() {
        return Err(AgentdirError::InvalidPath(format!(
            "{label} must be an absolute virtual path"
        )));
    }
    Ok(())
}

fn validate_new_name(new_name: &str) -> Result<()> {
    if new_name.contains('/') || new_name.contains('\\') {
        return Err(AgentdirError::InvalidPath(
            "new name must not contain path separators".into(),
        ));
    }
    if new_name.is_empty() {
        return Err(AgentdirError::InvalidPath(
            "new name must not be empty".into(),
        ));
    }
    Ok(())
}

fn rebase_virtual_path(
    path: &VirtualPath,
    from: &VirtualPath,
    to: &VirtualPath,
) -> Result<VirtualPath> {
    if path.as_str() == from.as_str() {
        return Ok(to.clone());
    }
    let from_prefix = if from.as_str() == "/" {
        "/".to_string()
    } else {
        format!("{}/", from.as_str())
    };
    let rest = path
        .as_str()
        .strip_prefix(&from_prefix)
        .ok_or_else(|| AgentdirError::InvalidPath(format!("{} is not under {}", path, from)))?;
    let separator = if to.as_str() == "/" { "" } else { "/" };
    VirtualPath::new(format!("{}{separator}{rest}", to.as_str()))
}
