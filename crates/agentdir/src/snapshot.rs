use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::backend::Backend;
use crate::catalog::Catalog;
use crate::error::{AgentdirError, Result};
use crate::manifest;
use crate::materializer::Materializer;
use crate::types::{
    CatalogEntry, EntryType, MaterializeStrategy, SourceMetadata, SourcePath, VirtualPath,
};
use crate::workspace::Workspace;

pub struct SnapshotWorkspace {
    pub name: String,
    pub catalog: Catalog,
    pub materializer: Materializer,
    pub backend: Arc<dyn Backend>,
    pub manifest_path: PathBuf,
    pub snapshot_root: PathBuf,
    #[allow(dead_code)]
    base_materialized_root: PathBuf,
}

impl Workspace {
    pub fn snapshot(&self, name: &str) -> Result<SnapshotWorkspace> {
        let snapshots_dir = self
            .manifest_path
            .parent()
            .unwrap_or(Path::new("."))
            .join("snapshots");
        let snapshot_root = snapshots_dir.join(name);

        if snapshot_root.exists() {
            return Err(AgentdirError::EntryExists(format!(
                "snapshot '{}' already exists",
                name
            )));
        }

        std::fs::create_dir_all(&snapshot_root)?;

        let mut snapshot_manifest = self.catalog.manifest.clone();
        snapshot_manifest.strategy = MaterializeStrategy::Symlink;
        snapshot_manifest.touch();

        let snapshot_manifest_path = snapshot_root.join("manifest.json");
        manifest::save(&snapshot_manifest, &snapshot_manifest_path)?;

        let snapshot_catalog = Catalog::from_manifest(snapshot_manifest, snapshot_root.clone());

        let mat = Materializer::with_strategy(snapshot_root.clone(), MaterializeStrategy::Symlink)?;

        let base_entries: Vec<_> = self.catalog.entries().to_vec();
        for entry in &base_entries {
            let base_file = self.materializer.materialized_path(&entry.virtual_path);
            if !base_file.exists() && base_file.symlink_metadata().is_err() {
                continue;
            }
            let dst = mat.materialized_path(&entry.virtual_path);
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)?;
            }
            match entry.metadata.entry_type {
                EntryType::Directory => {
                    std::fs::create_dir_all(&dst)?;
                }
                EntryType::File => {
                    #[cfg(unix)]
                    std::os::unix::fs::symlink(&base_file, &dst).map_err(|e| {
                        AgentdirError::ReflinkFailed(format!(
                            "snapshot symlink {:?} -> {:?}: {e}",
                            base_file, dst
                        ))
                    })?;
                    #[cfg(windows)]
                    std::os::windows::fs::symlink_file(&base_file, &dst).map_err(|e| {
                        AgentdirError::ReflinkFailed(format!(
                            "snapshot symlink {:?} -> {:?}: {e}",
                            base_file, dst
                        ))
                    })?;
                }
            }
        }

        Ok(SnapshotWorkspace {
            name: name.to_string(),
            catalog: snapshot_catalog,
            materializer: mat,
            backend: self.backend.clone(),
            manifest_path: snapshot_manifest_path,
            snapshot_root,
            base_materialized_root: self.materializer.materialized_root.clone(),
        })
    }

    pub fn list_snapshots(&self) -> Result<Vec<String>> {
        let snapshots_dir = self
            .manifest_path
            .parent()
            .unwrap_or(Path::new("."))
            .join("snapshots");

        if !snapshots_dir.exists() {
            return Ok(Vec::new());
        }

        let mut names = Vec::new();
        for entry in std::fs::read_dir(&snapshots_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    names.push(name.to_string());
                }
            }
        }
        names.sort();
        Ok(names)
    }

    pub fn destroy_snapshot(&self, name: &str) -> Result<()> {
        let snapshots_dir = self
            .manifest_path
            .parent()
            .unwrap_or(Path::new("."))
            .join("snapshots");
        let snapshot_root = snapshots_dir.join(name);

        if !snapshot_root.exists() {
            return Err(AgentdirError::EntryNotFound(format!(
                "snapshot '{}' does not exist",
                name
            )));
        }

        std::fs::remove_dir_all(&snapshot_root)?;
        Ok(())
    }

    pub fn open_snapshot(&self, name: &str) -> Result<SnapshotWorkspace> {
        let snapshots_dir = self
            .manifest_path
            .parent()
            .unwrap_or(Path::new("."))
            .join("snapshots");
        let snapshot_root = snapshots_dir.join(name);

        if !snapshot_root.exists() {
            return Err(AgentdirError::EntryNotFound(format!(
                "snapshot '{}' does not exist",
                name
            )));
        }

        let snapshot_manifest_path = snapshot_root.join("manifest.json");
        let loaded = manifest::load(&snapshot_manifest_path)?;

        Ok(SnapshotWorkspace {
            name: name.to_string(),
            catalog: Catalog::from_manifest(loaded, snapshot_root.clone()),
            materializer: Materializer::with_strategy(
                snapshot_root.clone(),
                MaterializeStrategy::Symlink,
            )?,
            backend: self.backend.clone(),
            manifest_path: snapshot_manifest_path,
            snapshot_root,
            base_materialized_root: self.materializer.materialized_root.clone(),
        })
    }
}

impl SnapshotWorkspace {
    pub fn exists(&self, path: &VirtualPath) -> bool {
        self.catalog.get(path).is_ok()
    }

    pub fn stat(&self, path: &VirtualPath) -> Result<&CatalogEntry> {
        self.catalog.get(path)
    }

    pub async fn read_bytes(&self, path: &VirtualPath) -> Result<Vec<u8>> {
        let entry = self.catalog.get(path)?;
        if matches!(entry.metadata.entry_type, EntryType::Directory) {
            return Err(AgentdirError::InvalidPath(format!(
                "cannot read directory: {}",
                path
            )));
        }
        self.backend.read_bytes(&entry.source_path).await
    }

    pub fn write(&mut self, virtual_path: &VirtualPath, content: &[u8]) -> Result<()> {
        let dst = self.materializer.materialized_path(virtual_path);

        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }

        if dst.symlink_metadata().is_ok() {
            std::fs::remove_file(&dst)?;
        }

        std::fs::write(&dst, content)?;

        let size = content.len() as u64;
        let mtime_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();

        if self.catalog.get(virtual_path).is_ok() {
            let entry = self.catalog.get_mut(virtual_path)?;
            entry.source_path = SourcePath::new(dst);
            entry.metadata.size_bytes = size;
            entry.metadata.mtime_ns = mtime_ns;
            entry.materialized = true;
        } else {
            let entry = CatalogEntry {
                virtual_path: virtual_path.clone(),
                source_path: SourcePath::new(dst.clone()),
                content_hash: None,
                metadata: SourceMetadata {
                    mtime_ns,
                    size_bytes: size,
                    entry_type: EntryType::File,
                },
                materialized: true,
            };
            self.catalog.add_entries(vec![entry])?;
        }

        self.save()
    }

    pub fn export_mapping(
        &self,
        direction: crate::types::MappingDirection,
        relative_to: Option<&Path>,
    ) -> Result<BTreeMap<String, String>> {
        let canonical_base = match relative_to {
            Some(base) => Some(base.canonicalize().map_err(|e| {
                AgentdirError::InvalidPath(format!("cannot canonicalize base {:?}: {e}", base))
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
                            "source {} is not under {:?}",
                            e.source_path, base
                        ))
                    })?
                    .to_string_lossy()
                    .into_owned(),
                None => e.source_path.as_path().to_string_lossy().into_owned(),
            };
            let virtual_str = e.virtual_path.as_str().to_string();
            let (key, value) = match direction {
                crate::types::MappingDirection::SourceToVirtual => (source_str, virtual_str),
                crate::types::MappingDirection::VirtualToSource => (virtual_str, source_str),
            };
            if let Some(existing) = map.insert(key.clone(), value) {
                if direction == crate::types::MappingDirection::SourceToVirtual {
                    return Err(AgentdirError::EntryExists(format!(
                        "duplicate source {key}: maps to both {existing} and current"
                    )));
                }
            }
        }
        Ok(map)
    }

    pub fn destroy(self) -> Result<()> {
        std::fs::remove_dir_all(&self.snapshot_root)?;
        Ok(())
    }

    pub fn save(&self) -> Result<()> {
        manifest::save(&self.catalog.manifest, &self.manifest_path)
    }
}
