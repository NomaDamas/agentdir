use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Virtual namespace path (what the agent sees)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VirtualPath(String);

impl VirtualPath {
    pub fn new(s: impl AsRef<str>) -> Result<Self, crate::error::AgentdirError> {
        let s = s.as_ref();
        if s.is_empty() {
            return Err(crate::error::AgentdirError::InvalidPath(
                "empty path".into(),
            ));
        }

        let is_absolute = s.starts_with('/');
        let mut components: Vec<String> = Vec::new();

        for component in Path::new(s).components() {
            use std::path::Component;
            match component {
                Component::Normal(part) => components.push(part.to_string_lossy().into_owned()),
                Component::CurDir => {}
                Component::ParentDir => {
                    if !components.is_empty() {
                        components.pop();
                    }
                }
                Component::RootDir => {}
                Component::Prefix(_) => {}
            }
        }

        let normalized = if is_absolute {
            match components.is_empty() {
                true => "/".to_string(),
                false => format!("/{}", components.join("/")),
            }
        } else {
            components.join("/")
        };

        if normalized.is_empty() {
            return Err(crate::error::AgentdirError::InvalidPath(
                "path normalizes to empty".into(),
            ));
        }

        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }

    /// Returns the parent virtual path.
    pub fn parent(&self) -> Option<VirtualPath> {
        self.as_path()
            .parent()
            .map(|p| VirtualPath(p.to_string_lossy().into_owned()))
    }

    /// Returns the file name component.
    pub fn file_name(&self) -> Option<&str> {
        self.as_path().file_name().and_then(|n| n.to_str())
    }

    /// Check if this path starts with another path (is a child of).
    pub fn starts_with_path(&self, other: &VirtualPath) -> bool {
        self.as_path().starts_with(other.as_path())
    }
}

impl fmt::Display for VirtualPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<Path> for VirtualPath {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

impl From<PathBuf> for VirtualPath {
    fn from(p: PathBuf) -> Self {
        let normalized = p.to_string_lossy().into_owned();
        Self(normalized)
    }
}

/// Real filesystem path (where the source file actually lives)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourcePath(PathBuf);

impl SourcePath {
    pub fn new(p: PathBuf) -> Self {
        Self(p)
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn to_path_buf(&self) -> PathBuf {
        self.0.clone()
    }

    /// Check if this source path is a child of another.
    pub fn starts_with(&self, other: &SourcePath) -> bool {
        self.0.starts_with(&other.0)
    }
}

impl fmt::Display for SourcePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

impl AsRef<Path> for SourcePath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl From<PathBuf> for SourcePath {
    fn from(p: PathBuf) -> Self {
        Self(p)
    }
}

/// SHA-256 content hash (lazily computed)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentHash(pub [u8; 32]);

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}

/// Type of a filesystem entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryType {
    File,
    Directory,
    Symlink { target: PathBuf },
}

/// Metadata from the source filesystem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceMetadata {
    /// Modification time in nanoseconds since Unix epoch.
    pub mtime_ns: u128,
    /// File size in bytes.
    pub size_bytes: u64,
    /// Entry type.
    pub entry_type: EntryType,
}

/// A single entry in the virtual catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogEntry {
    /// The virtual path the agent sees.
    pub virtual_path: VirtualPath,
    /// The real source path on disk.
    pub source_path: SourcePath,
    /// SHA-256 hash (None = not yet computed).
    pub content_hash: Option<ContentHash>,
    /// Source file metadata.
    pub metadata: SourceMetadata,
    /// Whether this entry has been materialized on disk.
    pub materialized: bool,
}

/// A source root mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRoot {
    /// Real path to the source directory.
    pub source_path: SourcePath,
    /// Virtual mount point.
    pub virtual_mount: VirtualPath,
    /// Whether to scan recursively.
    pub recursive: bool,
}

/// The virtual catalog manifest (persisted as JSON).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Schema version (always 1 for v1).
    pub version: u32,
    /// Creation timestamp (seconds since Unix epoch).
    pub created_at_epoch_secs: u64,
    /// Last update timestamp (seconds since Unix epoch).
    pub updated_at_epoch_secs: u64,
    /// Registered source roots.
    pub source_roots: Vec<SourceRoot>,
    /// All catalog entries.
    pub entries: Vec<CatalogEntry>,
}

impl Manifest {
    pub fn new() -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            version: 1,
            created_at_epoch_secs: now,
            updated_at_epoch_secs: now,
            source_roots: Vec::new(),
            entries: Vec::new(),
        }
    }

    pub fn touch(&mut self) {
        self.updated_at_epoch_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }
}

impl Default for Manifest {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_virtual_path_normalization() {
        let p = VirtualPath::new("/foo/bar/").unwrap();
        assert_eq!(p.as_str(), "/foo/bar");

        let p = VirtualPath::new("/foo/./bar").unwrap();
        assert_eq!(p.as_str(), "/foo/bar");

        assert!(VirtualPath::new("").is_err());
    }

    #[test]
    fn test_catalog_entry_roundtrip() {
        use std::path::PathBuf;

        let entry = CatalogEntry {
            virtual_path: VirtualPath::new("/docs/readme.md").unwrap(),
            source_path: SourcePath::new(PathBuf::from("/home/user/readme.md")),
            content_hash: None,
            metadata: SourceMetadata {
                mtime_ns: 1_000_000_000,
                size_bytes: 42,
                entry_type: EntryType::File,
            },
            materialized: false,
        };

        let json = serde_json::to_string(&entry).unwrap();
        let decoded: CatalogEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(entry.virtual_path.as_str(), decoded.virtual_path.as_str());
        assert_eq!(entry.metadata.size_bytes, decoded.metadata.size_bytes);
    }

    #[test]
    fn test_manifest_version_field() {
        let manifest = Manifest::new();
        assert_eq!(manifest.version, 1);

        let json = serde_json::to_string(&manifest).unwrap();
        assert!(json.contains("\"version\":1"));
    }

    #[test]
    fn test_content_hash_display() {
        let hash = ContentHash([0u8; 32]);
        let s = format!("{}", hash);

        assert_eq!(s.len(), 64);
        assert!(s.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
