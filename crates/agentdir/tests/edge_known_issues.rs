//! Known issue regression tests. All tests are #[ignore] — they document bugs or underspecified behaviors that should be fixed. Run with: cargo test -- --ignored

use agentdir::error::AgentdirError;
use agentdir::types::{SourcePath, VirtualPath};
use agentdir::workspace::Workspace;
use std::path::PathBuf;
use tempfile::TempDir;

#[tokio::test]
#[ignore = "known issue: From<PathBuf> bypasses VirtualPath validation"]
async fn test_virtual_path_from_pathbuf_bypasses_validation() {
    // This documents that the From<PathBuf> impl does not reuse VirtualPath::new validation.
    // SHOULD: converting an empty PathBuf should reject the path just like VirtualPath::new("").
    // ACTUALLY: From<PathBuf> constructs VirtualPath("") directly, leaving an invalid empty path.
    let path = VirtualPath::from(PathBuf::from(""));

    assert!(
        !path.as_str().is_empty(),
        "VirtualPath::from(PathBuf::from(\"\")) should not produce an empty virtual path"
    );
}

#[tokio::test]
#[ignore = "known issue: mv only moves single entry, not subtree"]
async fn test_mv_directory_does_not_move_children() {
    // This documents directory mv semantics for mapped subtrees.
    // SHOULD: moving /mount/parent to /target/parent should move every child catalog entry too.
    // ACTUALLY: only the directory entry is moved; child entries remain under the old prefix.
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
#[ignore = "known issue: cp only copies single entry, not subtree"]
async fn test_cp_directory_does_not_copy_children() {
    // This documents directory cp semantics for mapped subtrees.
    // SHOULD: copying /mount/dir to /copy/dir should copy every child catalog entry too.
    // ACTUALLY: only the directory entry is copied; child entries are not added under /copy/dir.
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
#[ignore = "known issue: apply_remove uses unmap which may remove sibling entries"]
async fn test_apply_remove_uses_unmap_removes_siblings() {
    // This documents refresh removal boundary behavior for paths with shared string prefixes.
    // SHOULD: deleting source readme should remove only /mount/readme and keep /mount/readme.backup.
    // ACTUALLY: an overly broad prefix-based unmap would remove siblings; current child_prefix logic may pass.
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
#[ignore = "known issue: recursive rmdir does not clean up materialized files"]
async fn test_rmdir_without_dematerialization() {
    // This documents recursive rmdir materialization cleanup behavior.
    // SHOULD: rmdir recursive=true should remove catalog entries and all materialized child files from disk.
    // ACTUALLY: catalog.rmdir removes child entries without individually dematerializing them; directory cleanup may mask this.
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
#[ignore = "known issue: duplicate source root registration allowed"]
async fn test_map_same_source_twice_different_mounts() {
    // This documents duplicate source-root registration for one real source directory.
    // SHOULD: mapping the same source twice should be rejected or represented in a deduplicated, explicit way.
    // ACTUALLY: source_roots can contain the same source path twice, so refresh scans the same tree repeatedly.
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
#[ignore = "known issue: relative virtual paths not explicitly rejected by map"]
async fn test_relative_virtual_path_in_map() {
    // This documents mount-point handling for relative virtual paths.
    // SHOULD: map should reject a relative mount path, or the API should clearly normalize it consistently.
    // ACTUALLY: relative mounts create catalog and materialized paths like "relative/file.txt" without a leading slash.
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
#[ignore = "known issue: rename does not reject names containing path separators"]
async fn test_rename_with_slash_in_new_name() {
    // This documents that rename accepts a path-like new_name.
    // SHOULD: rename should reject new names containing '/' because rename changes only the final component.
    // ACTUALLY: rename builds /mount/sub/file.txt, effectively moving into a subpath that may not exist in the catalog.
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
