use agentdir::types::{SourcePath, VirtualPath};
use agentdir::workspace::Workspace;
use tempfile::TempDir;

fn setup_base_workspace() -> (TempDir, TempDir, Workspace) {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    std::fs::write(src.path().join("file1.txt"), b"hello").unwrap();
    std::fs::write(src.path().join("file2.txt"), b"world").unwrap();
    let ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    (src, ws_dir, ws)
}

#[tokio::test]
async fn test_snapshot_creation() {
    let (src, _ws_dir, mut ws) = setup_base_workspace();
    ws.map(
        SourcePath::new(src.path().canonicalize().unwrap()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    let snap = ws.snapshot("run_001").unwrap();
    assert_eq!(snap.name, "run_001");
    assert!(snap.snapshot_root.exists());
}

#[tokio::test]
async fn test_snapshot_reads_base_files() {
    let (src, _ws_dir, mut ws) = setup_base_workspace();
    ws.map(
        SourcePath::new(src.path().canonicalize().unwrap()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    let snap = ws.snapshot("run_001").unwrap();
    let content = std::fs::read(
        snap.materializer
            .materialized_path(&VirtualPath::new("/docs/file1.txt").unwrap()),
    )
    .unwrap();
    assert_eq!(content, b"hello");
}

#[tokio::test]
async fn test_snapshot_write_is_local() {
    let (src, ws_dir, mut ws) = setup_base_workspace();
    ws.map(
        SourcePath::new(src.path().canonicalize().unwrap()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    let mut snap = ws.snapshot("run_001").unwrap();
    snap.write(&VirtualPath::new("/output.txt").unwrap(), b"agent output")
        .unwrap();

    let snap_file = snap
        .materializer
        .materialized_path(&VirtualPath::new("/output.txt").unwrap());
    assert!(snap_file.exists());
    assert_eq!(std::fs::read(&snap_file).unwrap(), b"agent output");

    let base_file = ws_dir.path().join("output.txt");
    assert!(!base_file.exists());
}

#[tokio::test]
async fn test_snapshot_write_does_not_leak_to_base() {
    let (src, _ws_dir, mut ws) = setup_base_workspace();
    ws.map(
        SourcePath::new(src.path().canonicalize().unwrap()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    let mut snap = ws.snapshot("run_001").unwrap();
    snap.write(
        &VirtualPath::new("/docs/file1.txt").unwrap(),
        b"overwritten in snapshot",
    )
    .unwrap();

    let snap_content = std::fs::read(
        snap.materializer
            .materialized_path(&VirtualPath::new("/docs/file1.txt").unwrap()),
    )
    .unwrap();
    assert_eq!(snap_content, b"overwritten in snapshot");

    assert_eq!(
        std::fs::read(src.path().join("file1.txt")).unwrap(),
        b"hello"
    );
}

#[tokio::test]
async fn test_concurrent_snapshots_isolated() {
    let (src, _ws_dir, mut ws) = setup_base_workspace();
    ws.map(
        SourcePath::new(src.path().canonicalize().unwrap()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    let mut snap1 = ws.snapshot("run_001").unwrap();
    let mut snap2 = ws.snapshot("run_002").unwrap();

    snap1
        .write(&VirtualPath::new("/snap1_only.txt").unwrap(), b"snap1")
        .unwrap();
    snap2
        .write(&VirtualPath::new("/snap2_only.txt").unwrap(), b"snap2")
        .unwrap();

    assert!(snap1.exists(&VirtualPath::new("/snap1_only.txt").unwrap()));
    assert!(!snap1.exists(&VirtualPath::new("/snap2_only.txt").unwrap()));
    assert!(snap2.exists(&VirtualPath::new("/snap2_only.txt").unwrap()));
    assert!(!snap2.exists(&VirtualPath::new("/snap1_only.txt").unwrap()));
}

#[tokio::test]
async fn test_snapshot_destroy() {
    let (src, _ws_dir, mut ws) = setup_base_workspace();
    ws.map(
        SourcePath::new(src.path().canonicalize().unwrap()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    let snap = ws.snapshot("run_001").unwrap();
    let snap_root = snap.snapshot_root.clone();
    assert!(snap_root.exists());

    snap.destroy().unwrap();
    assert!(!snap_root.exists());
}

#[tokio::test]
async fn test_destroy_does_not_affect_base() {
    let (src, ws_dir, mut ws) = setup_base_workspace();
    ws.map(
        SourcePath::new(src.path().canonicalize().unwrap()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    let snap = ws.snapshot("run_001").unwrap();
    snap.destroy().unwrap();

    assert!(ws_dir.path().join("docs/file1.txt").exists());
    assert_eq!(
        std::fs::read(ws_dir.path().join("docs/file1.txt")).unwrap(),
        b"hello"
    );
    assert!(ws.catalog.len() >= 2);
}

#[tokio::test]
async fn test_list_snapshots() {
    let (src, _ws_dir, mut ws) = setup_base_workspace();
    ws.map(
        SourcePath::new(src.path().canonicalize().unwrap()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    assert!(ws.list_snapshots().unwrap().is_empty());

    let _s1 = ws.snapshot("beta").unwrap();
    let _s2 = ws.snapshot("alpha").unwrap();

    let names = ws.list_snapshots().unwrap();
    assert_eq!(names, vec!["alpha", "beta"]);
}

#[tokio::test]
async fn test_snapshot_duplicate_name_rejected() {
    let (src, _ws_dir, mut ws) = setup_base_workspace();
    ws.map(
        SourcePath::new(src.path().canonicalize().unwrap()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    let _snap = ws.snapshot("run_001").unwrap();
    let result = ws.snapshot("run_001");
    assert!(result.is_err());
}

#[tokio::test]
async fn test_destroy_snapshot_via_workspace() {
    let (src, _ws_dir, mut ws) = setup_base_workspace();
    ws.map(
        SourcePath::new(src.path().canonicalize().unwrap()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    let _snap = ws.snapshot("run_001").unwrap();
    assert_eq!(ws.list_snapshots().unwrap().len(), 1);

    ws.destroy_snapshot("run_001").unwrap();
    assert!(ws.list_snapshots().unwrap().is_empty());
}

#[tokio::test]
async fn test_open_snapshot() {
    let (src, _ws_dir, mut ws) = setup_base_workspace();
    ws.map(
        SourcePath::new(src.path().canonicalize().unwrap()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    {
        let mut snap = ws.snapshot("run_001").unwrap();
        snap.write(&VirtualPath::new("/output.txt").unwrap(), b"persisted")
            .unwrap();
    }

    let snap = ws.open_snapshot("run_001").unwrap();
    assert!(snap.exists(&VirtualPath::new("/output.txt").unwrap()));
    assert!(snap.exists(&VirtualPath::new("/docs/file1.txt").unwrap()));
}
