use agentdir::types::{MaterializeStrategy, SourcePath, VirtualPath};
use agentdir::workspace::Workspace;
use tempfile::TempDir;

#[cfg(unix)]
fn permissions_are_enforced() -> bool {
    use std::os::unix::fs::PermissionsExt;

    // Root bypasses file permissions, so permission assertions are meaningless
    // when running as root (e.g., inside Docker containers). Detect by creating
    // a 000 file and checking if we can still read or write it.
    let probe = std::env::temp_dir().join("agentdir_root_probe_readonly");
    std::fs::write(&probe, b"x").unwrap();
    std::fs::set_permissions(&probe, std::fs::Permissions::from_mode(0o000)).unwrap();
    let can_read = std::fs::read(&probe).is_ok();
    let can_write = std::fs::OpenOptions::new().write(true).open(&probe).is_ok();
    std::fs::set_permissions(&probe, std::fs::Permissions::from_mode(0o644)).unwrap();
    let _ = std::fs::remove_file(&probe);
    !(can_read || can_write)
}

#[cfg(unix)]
#[tokio::test]
async fn test_reflink_materialized_file_is_readonly() {
    use std::os::unix::fs::PermissionsExt;

    if !permissions_are_enforced() {
        eprintln!("SKIPPED: running as root (permissions not enforced)");
        return;
    }

    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    std::fs::write(src.path().join("file.txt"), b"readonly content").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(
        SourcePath::new(src.path().canonicalize().unwrap()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    let mat = ws_dir.path().join("docs/file.txt");
    let mode = std::fs::metadata(&mat).unwrap().permissions().mode();
    assert_eq!(mode & 0o222, 0);
}

#[tokio::test]
async fn test_source_stays_writable_after_materialize() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    let source_file = src.path().join("file.txt");
    std::fs::write(&source_file, b"original").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(
        SourcePath::new(src.path().canonicalize().unwrap()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    std::fs::write(&source_file, b"new content").unwrap();
    assert_eq!(std::fs::read(&source_file).unwrap(), b"new content");
}

#[tokio::test]
async fn test_refresh_succeeds_on_readonly_materialized() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    let source_file = src.path().join("file.txt");
    std::fs::write(&source_file, b"original").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(
        SourcePath::new(src.path().canonicalize().unwrap()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&source_file, b"updated").unwrap();

    let summary = ws.refresh().await.unwrap();
    assert!(summary.errors.is_empty());
    assert_eq!(
        std::fs::read(ws_dir.path().join("docs/file.txt")).unwrap(),
        b"updated"
    );
}

#[tokio::test]
async fn test_unmap_succeeds_on_readonly_materialized() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    std::fs::write(src.path().join("file.txt"), b"content").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(
        SourcePath::new(src.path().canonicalize().unwrap()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    let mat = ws_dir.path().join("docs/file.txt");
    assert!(mat.exists());

    ws.unmap(&VirtualPath::new("/docs").unwrap()).unwrap();
    assert!(!mat.exists());
}

#[tokio::test]
async fn test_legacy_hardlink_manifest_is_rejected() {
    let ws_dir = TempDir::new().unwrap();
    Workspace::init(ws_dir.path().to_path_buf()).unwrap();

    let manifest_path = ws_dir.path().join(".agentdir/manifest.json");
    let json = std::fs::read_to_string(&manifest_path).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value["strategy"] = serde_json::Value::String("hardlink".to_string());
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&value).unwrap(),
    )
    .unwrap();

    // Prefer the full open path over a serde-only assertion so the test locks
    // the persisted manifest compatibility behavior observed by callers.
    assert!(Workspace::open(ws_dir.path().to_path_buf()).is_err());
    assert!(serde_json::from_str::<MaterializeStrategy>("\"reflink\"").is_ok());
}
