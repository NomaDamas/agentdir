//! Regression tests for previously known issues (GitHub issues #5–#10 and related edge cases).
//! These tests were originally #[ignore] and documented bugs. Now that the bugs are fixed,
//! they run as normal regression tests to prevent regressions.

use agentdir::error::AgentdirError;
use agentdir::types::{SourcePath, VirtualPath};
use agentdir::workspace::Workspace;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
#[should_panic(expected = "PathBuf must convert to a valid VirtualPath")]
fn test_virtual_path_from_pathbuf_rejects_empty() {
    // Fixed: From<PathBuf> now delegates to VirtualPath::new(), which rejects empty paths.
    // An empty PathBuf triggers a panic via .expect() — this is the correct behavior
    // since From<T> cannot return Result.
    let _path = VirtualPath::from(PathBuf::from(""));
}

#[tokio::test]
async fn test_mv_directory_moves_children() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::create_dir(src.path().join("parent")).unwrap();
    std::fs::write(src.path().join("parent/child.txt"), b"child").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new("/mount").unwrap(),
    )
    .await
    .unwrap();
    ws.mkdir(&VirtualPath::new("/target").unwrap()).unwrap();

    ws.mv(
        &VirtualPath::new("/mount/parent").unwrap(),
        &VirtualPath::new("/target/parent").unwrap(),
    )
    .unwrap();

    assert!(
        ws.catalog
            .get(&VirtualPath::new("/target/parent/child.txt").unwrap())
            .is_ok(),
        "moving a directory should move child catalog entries into the new subtree"
    );
}

#[tokio::test]
async fn test_cp_directory_copies_children() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::create_dir(src.path().join("dir")).unwrap();
    std::fs::write(src.path().join("dir/file.txt"), b"file").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new("/mount").unwrap(),
    )
    .await
    .unwrap();
    ws.mkdir(&VirtualPath::new("/copy").unwrap()).unwrap();

    ws.cp(
        &VirtualPath::new("/mount/dir").unwrap(),
        &VirtualPath::new("/copy/dir").unwrap(),
    )
    .unwrap();

    assert!(
        ws.catalog
            .get(&VirtualPath::new("/copy/dir/file.txt").unwrap())
            .is_ok(),
        "copying a directory should copy child catalog entries into the new subtree"
    );
}

#[tokio::test]
async fn test_remove_does_not_affect_siblings() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::write(src.path().join("readme"), b"readme").unwrap();
    std::fs::write(src.path().join("readme.backup"), b"backup").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new("/mount").unwrap(),
    )
    .await
    .unwrap();

    std::fs::remove_file(src.path().join("readme")).unwrap();
    ws.refresh().await.unwrap();

    assert!(
        ws.catalog
            .get(&VirtualPath::new("/mount/readme.backup").unwrap())
            .is_ok(),
        "removing /mount/readme should not remove sibling /mount/readme.backup"
    );
}

#[tokio::test]
async fn test_rmdir_recursive_dematerializes_children() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::write(src.path().join("a.txt"), b"a").unwrap();
    std::fs::write(src.path().join("b.txt"), b"b").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new("/mount").unwrap(),
    )
    .await
    .unwrap();
    ws.mkdir(&VirtualPath::new("/temp").unwrap()).unwrap();
    ws.mv(
        &VirtualPath::new("/mount/a.txt").unwrap(),
        &VirtualPath::new("/temp/a.txt").unwrap(),
    )
    .unwrap();
    ws.mv(
        &VirtualPath::new("/mount/b.txt").unwrap(),
        &VirtualPath::new("/temp/b.txt").unwrap(),
    )
    .unwrap();

    ws.rmdir(&VirtualPath::new("/temp").unwrap(), true).unwrap();

    assert!(
        !ws_dir.path().join("temp/a.txt").exists() && !ws_dir.path().join("temp/b.txt").exists(),
        "recursive rmdir should remove all materialized child files from disk"
    );
}

#[tokio::test]
async fn test_map_same_source_twice_rejected() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::write(src.path().join("file.txt"), b"file").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new("/mount_a").unwrap(),
    )
    .await
    .unwrap();
    let second_map = ws
        .map(
            SourcePath::new(src.path().to_path_buf()),
            VirtualPath::new("/mount_b").unwrap(),
        )
        .await;

    assert!(
        second_map.is_err() || ws.catalog.source_roots().len() == 1,
        "mapping the same source root twice should be rejected or deduplicated"
    );
}

#[tokio::test]
async fn test_relative_virtual_path_in_map_rejected() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::write(src.path().join("file.txt"), b"file").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    let result = ws
        .map(
            SourcePath::new(src.path().to_path_buf()),
            VirtualPath::new("relative").unwrap(),
        )
        .await;

    assert!(
        matches!(result, Err(AgentdirError::InvalidPath(_))),
        "mapping to a relative virtual mount should be rejected with InvalidPath"
    );
}

#[tokio::test]
async fn test_rename_with_slash_in_new_name_rejected() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::write(src.path().join("file.txt"), b"file").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new("/mount").unwrap(),
    )
    .await
    .unwrap();

    let result = ws.rename(
        &VirtualPath::new("/mount/file.txt").unwrap(),
        "sub/file.txt",
    );

    assert!(
        result.is_err(),
        "rename should reject new names containing path separators"
    );
}
