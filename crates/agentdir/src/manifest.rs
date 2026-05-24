//! Atomic JSON manifest persistence.
//!
//! Uses write-tmp → fsync → rename pattern to prevent corruption on crash.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use tracing::info;

use crate::error::{AgentdirError, Result};
use crate::types::Manifest;

/// Save a manifest to disk atomically.
///
/// Pattern: write to `.json.tmp` → fsync → rename to final path.
/// This is atomic on POSIX filesystems (rename is atomic).
pub fn save(manifest: &Manifest, path: &Path) -> Result<()> {
    let tmp_path = path.with_extension("json.tmp");

    let json = serde_json::to_string_pretty(manifest)
        .map_err(|e| AgentdirError::ManifestWrite(e.to_string()))?;

    let mut file = File::create(&tmp_path)?;
    file.write_all(json.as_bytes())?;
    file.sync_all()?;
    drop(file);

    fs::rename(&tmp_path, path)?;

    info!("saved manifest to {:?} ({} bytes)", path, json.len());
    Ok(())
}

/// Load a manifest from disk.
///
/// Validates that `version == 1`. Rejects unknown versions.
pub fn load(path: &Path) -> Result<Manifest> {
    let content = fs::read_to_string(path)?;

    let manifest: Manifest = serde_json::from_str(&content)
        .map_err(|e| AgentdirError::ManifestParse(e.to_string()))?;

    if manifest.version != 1 {
        return Err(AgentdirError::ManifestParse(format!(
            "unsupported manifest version: {} (expected 1)",
            manifest.version
        )));
    }

    info!("loaded manifest from {:?} ({} entries)", path, manifest.entries.len());
    Ok(manifest)
}

/// Returns the path to the manifest file within a workspace root.
pub fn manifest_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join(".agentdir").join("manifest.json")
}

/// Ensures the `.agentdir/` directory exists within the workspace root.
/// Returns the path to the `.agentdir/` directory.
pub fn ensure_workspace_dir(workspace_root: &Path) -> Result<PathBuf> {
    let dir = workspace_root.join(".agentdir");
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_save_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("manifest.json");

        let manifest = Manifest::new();
        save(&manifest, &path).unwrap();

        let loaded = load(&path).unwrap();
        assert_eq!(loaded.version, manifest.version);
        assert_eq!(loaded.entries.len(), manifest.entries.len());
        assert_eq!(loaded.source_roots.len(), manifest.source_roots.len());
    }

    #[test]
    fn test_no_tmp_file_after_save() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("manifest.json");

        let manifest = Manifest::new();
        save(&manifest, &path).unwrap();

        let tmp = path.with_extension("json.tmp");
        assert!(!tmp.exists());
        assert!(path.exists());
    }

    #[test]
    fn test_reject_unknown_version() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("manifest.json");

        let bad_json = r#"{"version":2,"created_at_epoch_secs":0,"updated_at_epoch_secs":0,"source_roots":[],"entries":[]}"#;
        std::fs::write(&path, bad_json).unwrap();

        let result = load(&path);
        assert!(matches!(result, Err(AgentdirError::ManifestParse(_))));
    }

    #[test]
    fn test_manifest_path() {
        let root = Path::new("/tmp/workspace");
        let path = manifest_path(root);
        assert_eq!(path, Path::new("/tmp/workspace/.agentdir/manifest.json"));
    }

    #[test]
    fn test_ensure_workspace_dir_creates_dir() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().join("my_workspace");
        std::fs::create_dir(&workspace).unwrap();

        let agentdir = ensure_workspace_dir(&workspace).unwrap();
        assert!(agentdir.exists());
        assert_eq!(agentdir, workspace.join(".agentdir"));
    }

    #[test]
    fn test_load_nonexistent_file() {
        let result = load(Path::new("/nonexistent/manifest.json"));
        assert!(result.is_err());
    }
}
