use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::{AgentdirError, Result};
use crate::types::{
    CatalogEntry, EntryType, Manifest, SourceMetadata, SourcePath, SourceRoot, VirtualPath,
};

/// In-memory virtual filesystem catalog.
///
/// Maps virtual paths to source file locations. Pure data — no filesystem operations.
#[derive(Clone)]
pub struct Catalog {
    /// The persisted manifest (source of truth for entries and source roots).
    pub manifest: Manifest,
    /// The root directory where files are materialized.
    pub materialized_root: PathBuf,
    /// O(1) lookup index: virtual path string → index into manifest.entries.
    entry_index: HashMap<String, usize>,
}

impl Catalog {
    /// Create a new empty catalog.
    pub fn new(materialized_root: PathBuf) -> Self {
        Self {
            manifest: Manifest::new(),
            materialized_root,
            entry_index: HashMap::new(),
        }
    }

    /// Create a catalog from an existing manifest.
    pub fn from_manifest(manifest: Manifest, materialized_root: PathBuf) -> Self {
        let mut catalog = Self {
            manifest,
            materialized_root,
            entry_index: HashMap::new(),
        };
        catalog.rebuild_index();
        catalog
    }

    /// Rebuild the entry index from manifest entries.
    fn rebuild_index(&mut self) {
        self.entry_index.clear();
        for (index, entry) in self.manifest.entries.iter().enumerate() {
            self.entry_index
                .insert(entry.virtual_path.as_str().to_string(), index);
        }
    }

    /// Register a source root mapping.
    ///
    /// Rejects overlaps with the materialized root and with already-registered
    /// source roots so the same real files cannot be cataloged twice.
    pub fn add_source_root(&mut self, source_root: SourceRoot) -> Result<()> {
        Self::validate_no_overlap(source_root.source_path.as_path(), &self.materialized_root)?;

        for existing in &self.manifest.source_roots {
            let new_path = comparable_path(source_root.source_path.as_path());
            let existing_path = comparable_path(existing.source_path.as_path());
            if new_path.starts_with(&existing_path) || existing_path.starts_with(&new_path) {
                return Err(AgentdirError::PathOverlap(format!(
                    "source root {:?} overlaps existing source root {:?}",
                    source_root.source_path.as_path(),
                    existing.source_path.as_path()
                )));
            }
        }

        self.manifest.source_roots.push(source_root);
        self.manifest.touch();
        Ok(())
    }

    /// Add pre-scanned entries to the catalog. Validates no duplicate virtual paths.
    ///
    /// The scanning itself is done by a backend; Catalog stays pure.
    pub fn add_entries(&mut self, entries: Vec<CatalogEntry>) -> Result<()> {
        for entry in entries {
            let key = entry.virtual_path.as_str().to_string();
            if self.entry_index.contains_key(&key) {
                return Err(AgentdirError::EntryExists(key));
            }

            let index = self.manifest.entries.len();
            self.entry_index.insert(key, index);
            self.manifest.entries.push(entry);
        }
        self.manifest.touch();
        Ok(())
    }

    /// Remove all entries under a virtual mount point. Returns the removed entries.
    pub fn unmap(&mut self, virtual_mount: &VirtualPath) -> Result<Vec<CatalogEntry>> {
        let prefix = virtual_mount.as_str();
        let child_prefix = child_prefix(prefix);
        let mut removed = Vec::new();
        let mut remaining = Vec::new();

        for entry in self.manifest.entries.drain(..) {
            let entry_path = entry.virtual_path.as_str();
            if entry_path == prefix || entry_path.starts_with(&child_prefix) {
                removed.push(entry);
            } else {
                remaining.push(entry);
            }
        }

        self.manifest.entries = remaining;
        self.manifest
            .source_roots
            .retain(|root| root.virtual_mount.as_str() != prefix);
        self.rebuild_index();
        self.manifest.touch();
        Ok(removed)
    }

    /// Create a virtual directory with no source backing.
    pub fn mkdir(&mut self, path: &VirtualPath) -> Result<()> {
        let key = path.as_str().to_string();
        if self.entry_index.contains_key(&key) {
            return Err(AgentdirError::EntryExists(key));
        }

        let entry = CatalogEntry {
            virtual_path: path.clone(),
            source_path: SourcePath::new(PathBuf::new()),
            content_hash: None,
            metadata: SourceMetadata {
                mtime_ns: 0,
                size_bytes: 0,
                entry_type: EntryType::Directory,
            },
            materialized: false,
        };

        let index = self.manifest.entries.len();
        self.entry_index.insert(key, index);
        self.manifest.entries.push(entry);
        self.manifest.touch();
        Ok(())
    }

    /// Remove a virtual directory. Fails if not empty unless recursive is true.
    pub fn rmdir(&mut self, path: &VirtualPath, recursive: bool) -> Result<()> {
        let prefix = path.as_str();
        let child_prefix = child_prefix(prefix);
        let has_children = self.manifest.entries.iter().any(|entry| {
            let entry_path = entry.virtual_path.as_str();
            entry_path != prefix && entry_path.starts_with(&child_prefix)
        });

        if has_children && !recursive {
            return Err(AgentdirError::EntryExists(format!(
                "directory {prefix} is not empty"
            )));
        }

        self.manifest.entries.retain(|entry| {
            let entry_path = entry.virtual_path.as_str();
            if recursive {
                entry_path != prefix && !entry_path.starts_with(&child_prefix)
            } else {
                entry_path != prefix
            }
        });

        self.rebuild_index();
        self.manifest.touch();
        Ok(())
    }

    /// Move/rename an entry or directory subtree in the virtual namespace.
    pub fn mv(&mut self, from: &VirtualPath, to: &VirtualPath) -> Result<()> {
        let from_key = from.as_str().to_string();
        let to_key = to.as_str().to_string();

        if !self.entry_index.contains_key(&from_key) {
            return Err(AgentdirError::EntryNotFound(from_key));
        }
        if self.entry_index.contains_key(&to_key) {
            return Err(AgentdirError::EntryExists(to_key));
        }

        let affected = self.affected_indices(from);
        let rebased: Vec<VirtualPath> = affected
            .iter()
            .map(|&index| rebase_virtual_path(&self.manifest.entries[index].virtual_path, from, to))
            .collect::<Result<_>>()?;

        for new_path in &rebased {
            if self.entry_index.contains_key(new_path.as_str()) {
                return Err(AgentdirError::EntryExists(new_path.as_str().to_string()));
            }
        }

        for (index, new_path) in affected.into_iter().zip(rebased) {
            self.manifest.entries[index].virtual_path = new_path;
        }
        self.rebuild_index();
        self.manifest.touch();
        Ok(())
    }

    /// Copy an entry or directory subtree to a new virtual path, preserving source references.
    pub fn cp(&mut self, from: &VirtualPath, to: &VirtualPath) -> Result<()> {
        let from_key = from.as_str().to_string();
        let to_key = to.as_str().to_string();

        if !self.entry_index.contains_key(&from_key) {
            return Err(AgentdirError::EntryNotFound(from_key));
        }
        if self.entry_index.contains_key(&to_key) {
            return Err(AgentdirError::EntryExists(to_key));
        }

        let affected = self.affected_indices(from);
        let mut new_entries = Vec::with_capacity(affected.len());
        for index in affected {
            let mut new_entry = self.manifest.entries[index].clone();
            new_entry.virtual_path = rebase_virtual_path(&new_entry.virtual_path, from, to)?;
            new_entry.materialized = false;
            if self
                .entry_index
                .contains_key(new_entry.virtual_path.as_str())
            {
                return Err(AgentdirError::EntryExists(
                    new_entry.virtual_path.as_str().to_string(),
                ));
            }
            new_entries.push(new_entry);
        }

        for entry in new_entries {
            let key = entry.virtual_path.as_str().to_string();
            let new_index = self.manifest.entries.len();
            self.entry_index.insert(key, new_index);
            self.manifest.entries.push(entry);
        }
        self.manifest.touch();
        Ok(())
    }

    /// Return cloned entries affected by an exact path or subtree operation.
    pub fn entries_under(&self, path: &VirtualPath) -> Result<Vec<CatalogEntry>> {
        if !self.entry_index.contains_key(path.as_str()) {
            return Err(AgentdirError::EntryNotFound(path.as_str().to_string()));
        }
        Ok(self
            .affected_indices(path)
            .into_iter()
            .map(|index| self.manifest.entries[index].clone())
            .collect())
    }

    fn affected_indices(&self, path: &VirtualPath) -> Vec<usize> {
        let prefix = path.as_str();
        let child_prefix = child_prefix(prefix);
        self.manifest
            .entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                let entry_path = entry.virtual_path.as_str();
                (entry_path == prefix || entry_path.starts_with(&child_prefix)).then_some(index)
            })
            .collect()
    }

    /// Rename an entry without changing its parent directory.
    pub fn rename(&mut self, path: &VirtualPath, new_name: &str) -> Result<()> {
        validate_new_name(new_name)?;
        let parent = path
            .parent()
            .ok_or_else(|| AgentdirError::InvalidPath("cannot rename root".into()))?;
        let separator = if parent.as_str() == "/" { "" } else { "/" };
        let new_path = VirtualPath::new(format!("{}{separator}{new_name}", parent.as_str()))?;
        self.mv(path, &new_path)
    }

    /// List all direct children of a virtual directory.
    pub fn list(&self, path: &VirtualPath) -> Result<Vec<&CatalogEntry>> {
        let prefix = path.as_str();
        let child_prefix = child_prefix(prefix);
        let mut children = Vec::new();

        for entry in &self.manifest.entries {
            let entry_path = entry.virtual_path.as_str();
            if entry_path == prefix {
                continue;
            }

            if let Some(rest) = entry_path.strip_prefix(&child_prefix) {
                if !rest.contains('/') {
                    children.push(entry);
                }
            }
        }

        Ok(children)
    }

    /// Get a single entry by virtual path.
    pub fn get(&self, path: &VirtualPath) -> Result<&CatalogEntry> {
        let key = path.as_str();
        let index = self
            .entry_index
            .get(key)
            .ok_or_else(|| AgentdirError::EntryNotFound(key.to_string()))?;
        Ok(&self.manifest.entries[*index])
    }

    /// Get a mutable reference to a single entry by virtual path.
    pub fn get_mut(&mut self, path: &VirtualPath) -> Result<&mut CatalogEntry> {
        let key = path.as_str().to_string();
        let index = *self
            .entry_index
            .get(&key)
            .ok_or(AgentdirError::EntryNotFound(key))?;
        Ok(&mut self.manifest.entries[index])
    }

    /// Resolve a virtual path to its source path.
    pub fn resolve(&self, virtual_path: &VirtualPath) -> Result<&SourcePath> {
        Ok(&self.get(virtual_path)?.source_path)
    }

    /// Find an entry by its source path.
    pub fn find_by_source(&self, source: &SourcePath) -> Option<&CatalogEntry> {
        self.manifest
            .entries
            .iter()
            .find(|entry| entry.source_path.as_path() == source.as_path())
    }

    /// Find ALL entries that reference a given source path.
    /// Returns entries for 1:N mappings (e.g., files duplicated via `cp`).
    pub fn find_all_by_source(&self, source: &SourcePath) -> Vec<&CatalogEntry> {
        self.manifest
            .entries
            .iter()
            .filter(|entry| entry.source_path.as_path() == source.as_path())
            .collect()
    }

    /// All entries in the catalog.
    pub fn entries(&self) -> &[CatalogEntry] {
        &self.manifest.entries
    }

    /// All source roots.
    pub fn source_roots(&self) -> &[SourceRoot] {
        &self.manifest.source_roots
    }

    /// Total number of entries.
    pub fn len(&self) -> usize {
        self.manifest.entries.len()
    }

    /// Whether the catalog is empty.
    pub fn is_empty(&self) -> bool {
        self.manifest.entries.is_empty()
    }

    /// Validate that source and materialized paths don't overlap.
    pub fn validate_no_overlap(source: &Path, materialized: &Path) -> Result<()> {
        let comparable_source = comparable_path(source);
        let comparable_materialized = comparable_path(materialized);
        if comparable_source.starts_with(&comparable_materialized)
            || comparable_materialized.starts_with(&comparable_source)
        {
            return Err(AgentdirError::PathOverlap(format!(
                "source {source:?} and materialized {materialized:?} overlap"
            )));
        }
        Ok(())
    }
}

fn comparable_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(path)
        }
    })
}

fn child_prefix(path: &str) -> String {
    if path == "/" {
        "/".to_string()
    } else {
        format!("{path}/")
    }
}

fn rebase_virtual_path(
    path: &VirtualPath,
    from: &VirtualPath,
    to: &VirtualPath,
) -> Result<VirtualPath> {
    if path.as_str() == from.as_str() {
        return Ok(to.clone());
    }

    let from_child_prefix = child_prefix(from.as_str());
    let rest = path
        .as_str()
        .strip_prefix(&from_child_prefix)
        .ok_or_else(|| AgentdirError::InvalidPath(format!("{} is not under {}", path, from)))?;
    let separator = if to.as_str() == "/" { "" } else { "/" };
    VirtualPath::new(format!("{}{separator}{rest}", to.as_str()))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(virtual_path: &str, source_path: &str) -> CatalogEntry {
        CatalogEntry {
            virtual_path: VirtualPath::new(virtual_path).unwrap(),
            source_path: SourcePath::new(PathBuf::from(source_path)),
            content_hash: None,
            metadata: SourceMetadata {
                mtime_ns: 1000,
                size_bytes: 100,
                entry_type: EntryType::File,
            },
            materialized: false,
        }
    }

    fn make_catalog() -> Catalog {
        Catalog::new(std::env::temp_dir().join("agentdir_test_materialized"))
    }

    #[test]
    fn test_add_entries_and_list() {
        let mut catalog = make_catalog();
        let entries = vec![
            make_entry("/docs/readme.md", "/src/readme.md"),
            make_entry("/docs/guide.md", "/src/guide.md"),
        ];
        catalog.add_entries(entries).unwrap();

        let children = catalog.list(&VirtualPath::new("/docs").unwrap()).unwrap();
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn test_mv_preserves_source() {
        let mut catalog = make_catalog();
        catalog
            .add_entries(vec![make_entry("/docs/old.md", "/src/old.md")])
            .unwrap();

        catalog
            .mv(
                &VirtualPath::new("/docs/old.md").unwrap(),
                &VirtualPath::new("/docs/new.md").unwrap(),
            )
            .unwrap();

        assert!(catalog
            .get(&VirtualPath::new("/docs/old.md").unwrap())
            .is_err());
        let entry = catalog
            .get(&VirtualPath::new("/docs/new.md").unwrap())
            .unwrap();
        assert_eq!(entry.source_path.as_path(), Path::new("/src/old.md"));
    }

    #[test]
    fn test_cp_same_source() {
        let mut catalog = make_catalog();
        catalog
            .add_entries(vec![make_entry("/docs/file.md", "/src/file.md")])
            .unwrap();

        catalog
            .cp(
                &VirtualPath::new("/docs/file.md").unwrap(),
                &VirtualPath::new("/backup/file.md").unwrap(),
            )
            .unwrap();

        let orig = catalog
            .get(&VirtualPath::new("/docs/file.md").unwrap())
            .unwrap();
        let copy = catalog
            .get(&VirtualPath::new("/backup/file.md").unwrap())
            .unwrap();
        assert_eq!(orig.source_path.as_path(), copy.source_path.as_path());
    }

    #[test]
    fn test_overlap_rejection() {
        let tmp = std::env::temp_dir();
        let result = Catalog::validate_no_overlap(
            &tmp.join("materialized/subdir"),
            &tmp.join("materialized"),
        );
        assert!(matches!(result, Err(AgentdirError::PathOverlap(_))));

        let result = Catalog::validate_no_overlap(&tmp.join("source"), &tmp.join("source/mat"));
        assert!(matches!(result, Err(AgentdirError::PathOverlap(_))));

        let result = Catalog::validate_no_overlap(&tmp.join("source"), &tmp.join("materialized"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_unmap_removes_entries() {
        let mut catalog = make_catalog();
        catalog
            .add_entries(vec![
                make_entry("/docs/a.md", "/src/a.md"),
                make_entry("/docs/b.md", "/src/b.md"),
                make_entry("/other/c.md", "/src/c.md"),
            ])
            .unwrap();

        let removed = catalog.unmap(&VirtualPath::new("/docs").unwrap()).unwrap();
        assert_eq!(removed.len(), 2);
        assert_eq!(catalog.len(), 1);
        assert!(catalog
            .get(&VirtualPath::new("/other/c.md").unwrap())
            .is_ok());
    }

    #[test]
    fn test_mkdir_and_rmdir() {
        let mut catalog = make_catalog();
        catalog.mkdir(&VirtualPath::new("/mydir").unwrap()).unwrap();
        assert!(catalog.get(&VirtualPath::new("/mydir").unwrap()).is_ok());

        catalog
            .rmdir(&VirtualPath::new("/mydir").unwrap(), false)
            .unwrap();
        assert!(catalog.get(&VirtualPath::new("/mydir").unwrap()).is_err());
    }

    #[test]
    fn test_rmdir_fails_if_not_empty() {
        let mut catalog = make_catalog();
        catalog
            .add_entries(vec![make_entry("/docs/file.md", "/src/file.md")])
            .unwrap();

        let result = catalog.rmdir(&VirtualPath::new("/docs").unwrap(), false);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_returns_source() {
        let mut catalog = make_catalog();
        catalog
            .add_entries(vec![make_entry("/docs/readme.md", "/src/readme.md")])
            .unwrap();

        let source = catalog
            .resolve(&VirtualPath::new("/docs/readme.md").unwrap())
            .unwrap();
        assert_eq!(source.as_path(), Path::new("/src/readme.md"));
    }

    #[test]
    fn test_entry_index_consistency_after_mv() {
        let mut catalog = make_catalog();
        catalog
            .add_entries(vec![make_entry("/a/b.md", "/src/b.md")])
            .unwrap();
        catalog
            .mv(
                &VirtualPath::new("/a/b.md").unwrap(),
                &VirtualPath::new("/c/d.md").unwrap(),
            )
            .unwrap();

        assert!(catalog.get(&VirtualPath::new("/a/b.md").unwrap()).is_err());
        assert!(catalog.get(&VirtualPath::new("/c/d.md").unwrap()).is_ok());
        assert_eq!(catalog.len(), 1);
    }
}
