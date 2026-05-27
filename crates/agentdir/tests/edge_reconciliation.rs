//! Reconciliation edge cases: change detection, idempotency, bulk operations, 1:N mapping refresh.

use agentdir::types::{SourcePath, VirtualPath};
use agentdir::workspace::Workspace;
use tempfile::TempDir;

fn setup_workspace_with_files(file_specs: &[(&str, &[u8])]) -> (TempDir, TempDir, Workspace) {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    for (name, content) in file_specs {
        if let Some(parent) = std::path::Path::new(name).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(src.path().join(parent)).unwrap();
            }
        }
        std::fs::write(src.path().join(name), content).unwrap();
    }
    let ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    (src, ws_dir, ws)
}

fn sleep_after_source_modification() {
    std::thread::sleep(std::time::Duration::from_millis(10));
}

#[tokio::test]
async fn test_refresh_no_changes_returns_zero_counts() {
    let (src, _ws_dir, mut ws) = setup_workspace_with_files(&[("file.txt", b"content")]);

    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new("/mount").unwrap(),
    )
    .await
    .unwrap();

    let summary = ws.refresh().await.unwrap();

    assert_eq!(summary.added, 0);
    assert_eq!(summary.removed, 0);
    assert_eq!(summary.refreshed, 0);
    assert!(summary.errors.is_empty());
    assert!(ws
        .catalog
        .get(&VirtualPath::new("/mount/file.txt").unwrap())
        .is_ok());
}

#[tokio::test]
async fn test_refresh_detects_mtime_change() {
    let (src, ws_dir, mut ws) = setup_workspace_with_files(&[("file.txt", b"same")]);

    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new("/mount").unwrap(),
    )
    .await
    .unwrap();

    std::fs::write(src.path().join("file.txt"), b"same").unwrap();
    sleep_after_source_modification();

    let summary = ws.refresh().await.unwrap();

    assert!(summary.refreshed >= 1);
    assert!(ws
        .catalog
        .get(&VirtualPath::new("/mount/file.txt").unwrap())
        .is_ok());
    assert_eq!(
        std::fs::read(ws_dir.path().join("mount/file.txt")).unwrap(),
        b"same"
    );
}

#[tokio::test]
async fn test_refresh_detects_content_change() {
    let (src, ws_dir, mut ws) = setup_workspace_with_files(&[("file.txt", b"old")]);

    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new("/mount").unwrap(),
    )
    .await
    .unwrap();

    std::fs::write(src.path().join("file.txt"), b"new content").unwrap();
    sleep_after_source_modification();

    let summary = ws.refresh().await.unwrap();

    assert!(summary.refreshed >= 1);
    assert!(ws
        .catalog
        .get(&VirtualPath::new("/mount/file.txt").unwrap())
        .is_ok());
    assert_eq!(
        std::fs::read(ws_dir.path().join("mount/file.txt")).unwrap(),
        b"new content"
    );
}

#[tokio::test]
async fn test_refresh_detects_new_file() {
    let (src, ws_dir, mut ws) = setup_workspace_with_files(&[("existing.txt", b"existing")]);

    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new("/mount").unwrap(),
    )
    .await
    .unwrap();

    std::fs::write(src.path().join("new.txt"), b"new").unwrap();
    sleep_after_source_modification();

    let summary = ws.refresh().await.unwrap();

    assert!(summary.added >= 1);
    assert!(ws
        .catalog
        .get(&VirtualPath::new("/mount/new.txt").unwrap())
        .is_ok());
    assert_eq!(
        std::fs::read(ws_dir.path().join("mount/new.txt")).unwrap(),
        b"new"
    );
}

#[tokio::test]
async fn test_refresh_detects_deleted_file() {
    let (src, ws_dir, mut ws) =
        setup_workspace_with_files(&[("keep.txt", b"keep"), ("delete.txt", b"delete")]);

    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new("/mount").unwrap(),
    )
    .await
    .unwrap();

    std::fs::remove_file(src.path().join("delete.txt")).unwrap();
    sleep_after_source_modification();

    let summary = ws.refresh().await.unwrap();

    assert!(summary.removed >= 1);
    assert!(ws
        .catalog
        .get(&VirtualPath::new("/mount/delete.txt").unwrap())
        .is_err());
    assert!(!ws_dir.path().join("mount/delete.txt").exists());
    assert!(ws
        .catalog
        .get(&VirtualPath::new("/mount/keep.txt").unwrap())
        .is_ok());
}

#[tokio::test]
async fn test_refresh_multiple_changes_simultaneously() {
    let (src, ws_dir, mut ws) = setup_workspace_with_files(&[
        ("one.txt", b"one"),
        ("two.txt", b"two"),
        ("three.txt", b"three"),
    ]);

    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new("/mount").unwrap(),
    )
    .await
    .unwrap();

    std::fs::write(src.path().join("added-a.txt"), b"added a").unwrap();
    std::fs::write(src.path().join("added-b.txt"), b"added b").unwrap();
    std::fs::write(src.path().join("one.txt"), b"one modified").unwrap();
    std::fs::remove_file(src.path().join("two.txt")).unwrap();
    sleep_after_source_modification();

    let summary = ws.refresh().await.unwrap();

    assert!(summary.added >= 2);
    assert!(summary.refreshed >= 1);
    assert!(summary.removed >= 1);
    assert!(ws
        .catalog
        .get(&VirtualPath::new("/mount/added-a.txt").unwrap())
        .is_ok());
    assert!(ws
        .catalog
        .get(&VirtualPath::new("/mount/added-b.txt").unwrap())
        .is_ok());
    assert!(ws
        .catalog
        .get(&VirtualPath::new("/mount/two.txt").unwrap())
        .is_err());
    assert_eq!(
        std::fs::read(ws_dir.path().join("mount/one.txt")).unwrap(),
        b"one modified"
    );
}

#[tokio::test]
async fn test_refresh_cp_updates_all_copies() {
    let (src, ws_dir, mut ws) = setup_workspace_with_files(&[("file.txt", b"original")]);

    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new("/mount").unwrap(),
    )
    .await
    .unwrap();
    ws.cp(
        &VirtualPath::new("/mount/file.txt").unwrap(),
        &VirtualPath::new("/copies/file-a.txt").unwrap(),
    )
    .unwrap();
    ws.cp(
        &VirtualPath::new("/mount/file.txt").unwrap(),
        &VirtualPath::new("/copies/file-b.txt").unwrap(),
    )
    .unwrap();

    std::fs::write(src.path().join("file.txt"), b"updated").unwrap();
    sleep_after_source_modification();

    let summary = ws.refresh().await.unwrap();

    assert!(summary.refreshed >= 3);
    for path in ["mount/file.txt", "copies/file-a.txt", "copies/file-b.txt"] {
        assert_eq!(std::fs::read(ws_dir.path().join(path)).unwrap(), b"updated");
    }
    assert!(ws
        .catalog
        .get(&VirtualPath::new("/copies/file-a.txt").unwrap())
        .is_ok());
    assert!(ws
        .catalog
        .get(&VirtualPath::new("/copies/file-b.txt").unwrap())
        .is_ok());
}

#[tokio::test]
async fn test_refresh_after_unmap_ignores_old_root() {
    let src_a = TempDir::new().unwrap();
    let src_b = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    std::fs::write(src_a.path().join("a.txt"), b"a").unwrap();
    std::fs::write(src_b.path().join("b.txt"), b"b").unwrap();
    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();

    ws.map(
        SourcePath::new(src_a.path().to_path_buf()),
        VirtualPath::new("/a").unwrap(),
    )
    .await
    .unwrap();
    ws.map(
        SourcePath::new(src_b.path().to_path_buf()),
        VirtualPath::new("/b").unwrap(),
    )
    .await
    .unwrap();
    ws.unmap(&VirtualPath::new("/a").unwrap()).unwrap();

    std::fs::write(src_a.path().join("new-from-a.txt"), b"ignored").unwrap();
    sleep_after_source_modification();

    let summary = ws.refresh().await.unwrap();

    assert_eq!(summary.added, 0);
    assert!(ws
        .catalog
        .get(&VirtualPath::new("/a/new-from-a.txt").unwrap())
        .is_err());
    assert!(!ws_dir.path().join("a/new-from-a.txt").exists());
    assert!(ws
        .catalog
        .get(&VirtualPath::new("/b/b.txt").unwrap())
        .is_ok());
    assert_eq!(std::fs::read(ws_dir.path().join("b/b.txt")).unwrap(), b"b");
}

#[tokio::test]
async fn test_double_refresh_is_idempotent() {
    let (src, _ws_dir, mut ws) = setup_workspace_with_files(&[("file.txt", b"old")]);

    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new("/mount").unwrap(),
    )
    .await
    .unwrap();

    std::fs::write(src.path().join("file.txt"), b"new").unwrap();
    sleep_after_source_modification();

    let first = ws.refresh().await.unwrap();
    assert!(first.added + first.removed + first.refreshed > 0);

    let second = ws.refresh().await.unwrap();
    assert_eq!(second.added, 0);
    assert_eq!(second.removed, 0);
    assert_eq!(second.refreshed, 0);
    assert!(ws
        .catalog
        .get(&VirtualPath::new("/mount/file.txt").unwrap())
        .is_ok());
}

#[tokio::test]
async fn test_source_replaced_with_different_content() {
    let (src, ws_dir, mut ws) = setup_workspace_with_files(&[("file.txt", b"hello")]);

    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new("/mount").unwrap(),
    )
    .await
    .unwrap();

    let replacement = b"completely different and much longer content";
    std::fs::write(src.path().join("file.txt"), replacement).unwrap();
    sleep_after_source_modification();

    let summary = ws.refresh().await.unwrap();

    assert!(summary.refreshed >= 1);
    assert!(ws
        .catalog
        .get(&VirtualPath::new("/mount/file.txt").unwrap())
        .is_ok());
    assert_eq!(
        std::fs::read(ws_dir.path().join("mount/file.txt")).unwrap(),
        replacement
    );
}

#[tokio::test]
async fn test_rapid_source_modifications() {
    let (src, ws_dir, mut ws) = setup_workspace_with_files(&[("file.txt", b"initial")]);

    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new("/mount").unwrap(),
    )
    .await
    .unwrap();

    for i in 0..10 {
        std::fs::write(src.path().join("file.txt"), format!("content {i}")).unwrap();
    }
    sleep_after_source_modification();

    let summary = ws.refresh().await.unwrap();

    assert!(summary.refreshed >= 1);
    assert!(ws
        .catalog
        .get(&VirtualPath::new("/mount/file.txt").unwrap())
        .is_ok());
    assert_eq!(
        std::fs::read(ws_dir.path().join("mount/file.txt")).unwrap(),
        b"content 9"
    );
}

#[tokio::test]
async fn test_new_subdirectory_with_files_detected() {
    let (src, ws_dir, mut ws) = setup_workspace_with_files(&[("file.txt", b"flat")]);

    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new("/mount").unwrap(),
    )
    .await
    .unwrap();

    std::fs::create_dir(src.path().join("newdir")).unwrap();
    std::fs::write(src.path().join("newdir/newfile.txt"), b"nested").unwrap();
    sleep_after_source_modification();

    let summary = ws.refresh().await.unwrap();

    assert!(summary.added >= 1);
    assert!(ws
        .catalog
        .get(&VirtualPath::new("/mount/newdir/newfile.txt").unwrap())
        .is_ok());
    assert_eq!(
        std::fs::read(ws_dir.path().join("mount/newdir/newfile.txt")).unwrap(),
        b"nested"
    );
}

#[tokio::test]
async fn test_refresh_preserves_virtual_only_directories() {
    let (src, ws_dir, mut ws) = setup_workspace_with_files(&[("file.txt", b"content")]);

    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new("/mount").unwrap(),
    )
    .await
    .unwrap();
    ws.mkdir(&VirtualPath::new("/virtual-dir").unwrap())
        .unwrap();

    let summary = ws.refresh().await.unwrap();

    assert!(summary.errors.is_empty());
    assert!(ws
        .catalog
        .get(&VirtualPath::new("/virtual-dir").unwrap())
        .is_ok());
    assert!(ws_dir.path().join("virtual-dir").exists());
}
