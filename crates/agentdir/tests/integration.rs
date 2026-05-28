//! End-to-end integration tests for agentdir.
//!
//! All tests use tempfile::TempDir for isolation.
//! All tests are #[tokio::test].
//! No test depends on external tools being installed.
//! No test requires root/sudo.

use agentdir::types::{SourcePath, VirtualPath};
use agentdir::workspace::Workspace;
use tempfile::TempDir;

fn create_test_files(dir: &std::path::Path) {
    std::fs::write(dir.join("file1.txt"), b"content of file1").unwrap();
    std::fs::write(dir.join("file2.txt"), b"content of file2").unwrap();
    std::fs::create_dir(dir.join("subdir")).unwrap();
    std::fs::write(dir.join("subdir/nested.txt"), b"nested content").unwrap();
}

/// Test 1: Full lifecycle — init → map → verify → modify → refresh → verify → unmap → verify cleanup
#[tokio::test]
async fn test_full_lifecycle() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    create_test_files(src.path());

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    assert!(ws_dir.path().join(".agentdir/manifest.json").exists());

    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(
        std::fs::read(ws_dir.path().join("docs/file1.txt")).unwrap(),
        b"content of file1"
    );
    assert_eq!(
        std::fs::read(ws_dir.path().join("docs/file2.txt")).unwrap(),
        b"content of file2"
    );
    assert_eq!(
        std::fs::read(ws_dir.path().join("docs/subdir/nested.txt")).unwrap(),
        b"nested content"
    );

    std::thread::sleep(std::time::Duration::from_millis(10));
    std::fs::write(src.path().join("file1.txt"), b"modified file1").unwrap();

    let refresh_summary = ws.refresh().await.unwrap();
    assert!(refresh_summary.refreshed >= 1);
    assert_eq!(
        std::fs::read(ws_dir.path().join("docs/file1.txt")).unwrap(),
        b"modified file1"
    );

    ws.unmap(&VirtualPath::new("/docs").unwrap()).unwrap();
    assert_eq!(ws.catalog.len(), 0);
    assert!(!ws_dir.path().join("docs").exists());
}

/// Test 2: Multi-source mapping
#[tokio::test]
async fn test_multi_source_mapping() {
    let src1 = TempDir::new().unwrap();
    let src2 = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::write(src1.path().join("a.txt"), b"from src1").unwrap();
    std::fs::write(src2.path().join("b.txt"), b"from src2").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();

    ws.map(
        SourcePath::new(src1.path().to_path_buf()),
        VirtualPath::new("/src1").unwrap(),
    )
    .await
    .unwrap();
    ws.map(
        SourcePath::new(src2.path().to_path_buf()),
        VirtualPath::new("/src2").unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(
        std::fs::read(ws_dir.path().join("src1/a.txt")).unwrap(),
        b"from src1"
    );
    assert_eq!(
        std::fs::read(ws_dir.path().join("src2/b.txt")).unwrap(),
        b"from src2"
    );

    ws.unmap(&VirtualPath::new("/src1").unwrap()).unwrap();
    assert!(!ws_dir.path().join("src1/a.txt").exists());
    assert_eq!(
        std::fs::read(ws_dir.path().join("src2/b.txt")).unwrap(),
        b"from src2"
    );
}

/// Test 3: Virtual operations
#[tokio::test]
async fn test_virtual_operations() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::write(src.path().join("file.txt"), b"original").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    ws.mkdir(&VirtualPath::new("/archive").unwrap()).unwrap();
    assert!(ws_dir.path().join("archive").exists());

    ws.mv(
        &VirtualPath::new("/docs/file.txt").unwrap(),
        &VirtualPath::new("/archive/file.txt").unwrap(),
    )
    .unwrap();
    assert!(!ws_dir.path().join("docs/file.txt").exists());
    assert_eq!(
        std::fs::read(ws_dir.path().join("archive/file.txt")).unwrap(),
        b"original"
    );

    ws.cp(
        &VirtualPath::new("/archive/file.txt").unwrap(),
        &VirtualPath::new("/archive/file_copy.txt").unwrap(),
    )
    .unwrap();
    assert_eq!(
        std::fs::read(ws_dir.path().join("archive/file_copy.txt")).unwrap(),
        b"original"
    );
}

/// Test 4: Persistence roundtrip
#[tokio::test]
async fn test_persistence_roundtrip() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    create_test_files(src.path());

    {
        let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
        ws.map(
            SourcePath::new(src.path().to_path_buf()),
            VirtualPath::new("/docs").unwrap(),
        )
        .await
        .unwrap();
    }

    let ws = Workspace::open(ws_dir.path().to_path_buf()).unwrap();
    assert!(ws.catalog.len() > 0);
    assert_eq!(ws.catalog.source_roots().len(), 1);

    assert_eq!(
        std::fs::read(ws_dir.path().join("docs/file1.txt")).unwrap(),
        b"content of file1"
    );
}

/// Test 5: Large tree (500 files)
#[tokio::test]
async fn test_large_tree() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    for i in 0..500 {
        std::fs::write(
            src.path().join(format!("file{i:04}.txt")),
            format!("content {i}").as_bytes(),
        )
        .unwrap();
    }

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    let summary = ws
        .map(
            SourcePath::new(src.path().to_path_buf()),
            VirtualPath::new("/files").unwrap(),
        )
        .await
        .unwrap();

    assert!(summary.entries_added >= 500);

    assert_eq!(
        std::fs::read(ws_dir.path().join("files/file0000.txt")).unwrap(),
        b"content 0"
    );
    assert_eq!(
        std::fs::read(ws_dir.path().join("files/file0499.txt")).unwrap(),
        b"content 499"
    );

    std::thread::sleep(std::time::Duration::from_millis(10));
    for i in 0..10 {
        std::fs::write(src.path().join(format!("file{i:04}.txt")), b"modified").unwrap();
    }

    let refresh_summary = ws.refresh().await.unwrap();
    assert!(refresh_summary.refreshed >= 10);

    assert_eq!(
        std::fs::read(ws_dir.path().join("files/file0000.txt")).unwrap(),
        b"modified"
    );
}

/// Test 6: Source deletion propagation
#[tokio::test]
async fn test_source_deletion_propagation() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::write(src.path().join("keep.txt"), b"keep").unwrap();
    std::fs::write(src.path().join("delete.txt"), b"delete me").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    assert!(ws_dir.path().join("docs/delete.txt").exists());

    std::fs::remove_file(src.path().join("delete.txt")).unwrap();

    let summary = ws.refresh().await.unwrap();
    assert!(summary.removed >= 1);

    assert!(!ws_dir.path().join("docs/delete.txt").exists());
    assert!(ws_dir.path().join("docs/keep.txt").exists());
}

/// Test 7: New file auto-addition
#[tokio::test]
async fn test_new_file_auto_addition() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::write(src.path().join("existing.txt"), b"existing").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    std::fs::write(src.path().join("new.txt"), b"new file content").unwrap();

    let summary = ws.refresh().await.unwrap();
    assert!(summary.added >= 1);

    assert_eq!(
        std::fs::read(ws_dir.path().join("docs/new.txt")).unwrap(),
        b"new file content"
    );
}

/// Test 8: Ripgrep compatibility (skipped if rg not installed)
#[tokio::test]
async fn test_ripgrep_compatibility() {
    match std::process::Command::new("rg").arg("--version").output() {
        Ok(o) if o.status.success() => {}
        _ => {
            eprintln!("Skipping ripgrep test: rg not available");
            return;
        }
    }

    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::write(src.path().join("searchable.txt"), b"FINDME in this file").unwrap();
    std::fs::write(src.path().join("other.txt"), b"nothing here").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    let output = std::process::Command::new("rg")
        .arg("FINDME")
        .arg(ws_dir.path().join("docs"))
        .output()
        .unwrap();

    assert!(output.status.success(), "ripgrep should find FINDME");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("FINDME"));
}

/// Test 9: Overlap rejection
#[tokio::test]
async fn test_overlap_rejection() {
    let ws_dir = TempDir::new().unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();

    let result = ws
        .map(
            SourcePath::new(ws_dir.path().to_path_buf()),
            VirtualPath::new("/self").unwrap(),
        )
        .await;

    assert!(
        result.is_err(),
        "Should reject overlapping source and materialized root"
    );
}

/// Test 10: Empty workspace operations
#[tokio::test]
async fn test_empty_workspace_operations() {
    let ws_dir = TempDir::new().unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();

    let status = ws.status();
    assert_eq!(status.total_entries, 0);
    assert_eq!(status.source_roots, 0);

    let summary = ws.refresh().await.unwrap();
    assert_eq!(summary.added, 0);
    assert_eq!(summary.removed, 0);
    assert_eq!(summary.refreshed, 0);
}

/// Test 11: cp'd entries stay in sync when source changes
#[tokio::test]
async fn test_cp_refresh_sync() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::write(src.path().join("file.txt"), b"original").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    ws.cp(
        &VirtualPath::new("/docs/file.txt").unwrap(),
        &VirtualPath::new("/backup/file.txt").unwrap(),
    )
    .unwrap();

    assert_eq!(
        std::fs::read(ws_dir.path().join("docs/file.txt")).unwrap(),
        b"original"
    );
    assert_eq!(
        std::fs::read(ws_dir.path().join("backup/file.txt")).unwrap(),
        b"original"
    );

    std::thread::sleep(std::time::Duration::from_millis(10));
    std::fs::write(src.path().join("file.txt"), b"modified").unwrap();

    let summary = ws.refresh().await.unwrap();
    assert!(
        summary.refreshed >= 2,
        "Both copies should be refreshed, got {}",
        summary.refreshed
    );

    assert_eq!(
        std::fs::read(ws_dir.path().join("docs/file.txt")).unwrap(),
        b"modified"
    );
    assert_eq!(
        std::fs::read(ws_dir.path().join("backup/file.txt")).unwrap(),
        b"modified"
    );
}
