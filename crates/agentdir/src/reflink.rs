//! Safe reflink (copy-on-write clone) wrapper.
//!
//! This is the ONLY module that calls `reflink_copy::reflink_or_copy`.

use std::fs;
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};
use tracing::debug;

use crate::error::{AgentdirError, Result};
use crate::types::ContentHash;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloneResult {
    Reflinked,
    Copied(u64),
}

pub fn clone_file(src: &Path, dst: &Path) -> Result<CloneResult> {
    if dst.exists() {
        fs::remove_file(dst).map_err(|e| {
            AgentdirError::ReflinkFailed(format!("failed to remove existing dst {:?}: {}", dst, e))
        })?;
    }

    if let Some(parent) = dst.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| {
                AgentdirError::ReflinkFailed(format!(
                    "failed to create parent dirs for {:?}: {}",
                    dst, e
                ))
            })?;
        }
    }

    match reflink_copy::reflink_or_copy(src, dst) {
        Ok(None) => {
            debug!(?src, ?dst, "reflinked file");
            Ok(CloneResult::Reflinked)
        }
        Ok(Some(bytes)) => {
            debug!(?src, ?dst, bytes, "copied file");
            Ok(CloneResult::Copied(bytes))
        }
        Err(e) => Err(AgentdirError::ReflinkFailed(format!(
            "reflink_or_copy {:?} -> {:?}: {}",
            src, dst, e
        ))),
    }
}

pub fn clone_file_verified(
    src: &Path,
    dst: &Path,
    expected_hash: Option<&ContentHash>,
) -> Result<CloneResult> {
    let result = clone_file(src, dst)?;

    if let Some(expected) = expected_hash {
        let actual = compute_hash(dst)?;
        if actual != *expected {
            return Err(AgentdirError::HashMismatch {
                expected: expected.to_string(),
                actual: actual.to_string(),
            });
        }
    }

    Ok(result)
}

pub fn compute_hash(path: &Path) -> Result<ContentHash> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];

    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    let result = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    Ok(ContentHash(hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    use tempfile::TempDir;

    fn write_file(dir: &Path, name: &str, content: &[u8]) -> std::path::PathBuf {
        let path = dir.join(name);
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(content).unwrap();
        path
    }

    #[test]
    fn test_clone_file_basic() {
        let dir = TempDir::new().unwrap();
        let src = write_file(dir.path(), "src.txt", b"hello agentdir");
        let dst = dir.path().join("dst.txt");

        let result = clone_file(&src, &dst).unwrap();
        assert!(matches!(
            result,
            CloneResult::Reflinked | CloneResult::Copied(_)
        ));
        assert_eq!(fs::read(&dst).unwrap(), b"hello agentdir");
    }

    #[test]
    fn test_clone_overwrites_existing() {
        let dir = TempDir::new().unwrap();
        let src = write_file(dir.path(), "src.txt", b"new content");
        let dst = write_file(dir.path(), "dst.txt", b"old content");

        clone_file(&src, &dst).unwrap();
        assert_eq!(fs::read(&dst).unwrap(), b"new content");
    }

    #[test]
    fn test_clone_creates_parents() {
        let dir = TempDir::new().unwrap();
        let src = write_file(dir.path(), "src.txt", b"data");
        let dst = dir.path().join("a").join("b").join("c").join("dst.txt");

        clone_file(&src, &dst).unwrap();
        assert_eq!(fs::read(&dst).unwrap(), b"data");
    }

    #[test]
    fn test_clone_nonexistent_source() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("nonexistent.txt");
        let dst = dir.path().join("dst.txt");

        let result = clone_file(&src, &dst);
        assert!(result.is_err());
    }

    #[test]
    fn test_compute_hash() {
        let dir = TempDir::new().unwrap();
        let path = write_file(dir.path(), "empty.txt", b"");
        let hash = compute_hash(&path).unwrap();
        let expected = [
            0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f,
            0xb9, 0x24, 0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c, 0xa4, 0x95, 0x99, 0x1b,
            0x78, 0x52, 0xb8, 0x55,
        ];
        assert_eq!(hash.0, expected);
    }

    #[test]
    fn test_clone_file_verified_correct_hash() {
        let dir = TempDir::new().unwrap();
        let src = write_file(dir.path(), "src.txt", b"verify me");
        let dst = dir.path().join("dst.txt");

        let expected_hash = compute_hash(&src).unwrap();
        let result = clone_file_verified(&src, &dst, Some(&expected_hash)).unwrap();
        assert!(matches!(
            result,
            CloneResult::Reflinked | CloneResult::Copied(_)
        ));
    }

    #[test]
    fn test_clone_file_verified_wrong_hash() {
        let dir = TempDir::new().unwrap();
        let src = write_file(dir.path(), "src.txt", b"real content");
        let dst = dir.path().join("dst.txt");

        let wrong_hash = ContentHash([0u8; 32]);
        let result = clone_file_verified(&src, &dst, Some(&wrong_hash));
        assert!(matches!(result, Err(AgentdirError::HashMismatch { .. })));
    }
}
