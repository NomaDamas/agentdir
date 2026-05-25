//! Virtual path edge cases: normalization, error handling, catalog operations with boundary paths.

use agentdir::error::AgentdirError;
use agentdir::types::{SourcePath, VirtualPath};
use agentdir::workspace::Workspace;
use tempfile::TempDir;

#[tokio::test]
async fn test_virtual_path_empty_rejected() {
    let result = VirtualPath::new("");

    assert!(matches!(result, Err(AgentdirError::InvalidPath(_))));
}

#[tokio::test]
async fn test_virtual_path_dot_only_relative() {
    let result = VirtualPath::new(".");

    assert!(matches!(result, Err(AgentdirError::InvalidPath(_))));
}

#[tokio::test]
async fn test_virtual_path_double_dot_normalization() {
    let path = VirtualPath::new("/a/b/../c").unwrap();

    assert_eq!(path.as_str(), "/a/c");
}

#[tokio::test]
async fn test_virtual_path_traversal_beyond_root() {
    let path = VirtualPath::new("/a/../../b").unwrap();

    assert_eq!(path.as_str(), "/b");
}

#[tokio::test]
async fn test_virtual_path_double_slashes_normalized() {
    let path = VirtualPath::new("/a//b///c").unwrap();

    assert_eq!(path.as_str(), "/a/b/c");
}

#[tokio::test]
async fn test_virtual_path_trailing_slash_stripped() {
    let path = VirtualPath::new("/foo/bar/").unwrap();

    assert_eq!(path.as_str(), "/foo/bar");
}

#[tokio::test]
async fn test_virtual_path_root_only() {
    let path = VirtualPath::new("/").unwrap();

    assert_eq!(path.as_str(), "/");
}

#[tokio::test]
async fn test_virtual_path_parent_of_root() {
    let path = VirtualPath::new("/").unwrap();

    assert!(path.parent().is_none());
}

#[tokio::test]
async fn test_virtual_path_file_name_of_root() {
    let path = VirtualPath::new("/").unwrap();

    assert_eq!(path.file_name(), None);
}

#[tokio::test]
async fn test_map_to_root_mount() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::write(src.path().join("filename.txt"), b"root mounted").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new("/").unwrap(),
    )
    .await
    .unwrap();

    let entry = ws.catalog.get(&VirtualPath::new("/filename.txt").unwrap());
    assert!(entry.is_ok());
    assert_eq!(
        std::fs::read(ws_dir.path().join("filename.txt")).unwrap(),
        b"root mounted"
    );
}

#[tokio::test]
async fn test_mv_to_same_path_is_error() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::write(src.path().join("file.txt"), b"content").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    let path = VirtualPath::new("/docs/file.txt").unwrap();
    let result = ws.mv(&path, &path);

    assert!(matches!(result, Err(AgentdirError::EntryExists(_))));
}

#[tokio::test]
async fn test_mv_nonexistent_source_is_error() {
    let ws_dir = TempDir::new().unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    let result = ws.mv(
        &VirtualPath::new("/missing.txt").unwrap(),
        &VirtualPath::new("/target.txt").unwrap(),
    );

    assert!(matches!(result, Err(AgentdirError::EntryNotFound(_))));
}

#[tokio::test]
async fn test_cp_to_existing_path_is_error() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::write(src.path().join("file1.txt"), b"one").unwrap();
    std::fs::write(src.path().join("file2.txt"), b"two").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    let result = ws.cp(
        &VirtualPath::new("/docs/file1.txt").unwrap(),
        &VirtualPath::new("/docs/file2.txt").unwrap(),
    );

    assert!(matches!(result, Err(AgentdirError::EntryExists(_))));
}

#[tokio::test]
async fn test_rename_root_entry_rejected() {
    let ws_dir = TempDir::new().unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    let result = ws.catalog.rename(&VirtualPath::new("/").unwrap(), "new");

    assert!(matches!(result, Err(AgentdirError::InvalidPath(_))));
}

#[tokio::test]
async fn test_mkdir_duplicate_rejected() {
    let ws_dir = TempDir::new().unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    let path = VirtualPath::new("/dir").unwrap();

    ws.mkdir(&path).unwrap();
    let result = ws.mkdir(&path);

    assert!(matches!(result, Err(AgentdirError::EntryExists(_))));
}
