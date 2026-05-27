//! Persistence edge cases: manifest roundtrip, corruption handling, atomic save, workspace reopen.

use agentdir::error::AgentdirError;
use agentdir::manifest::{self, manifest_path};
use agentdir::types::{SourcePath, VirtualPath};
use agentdir::workspace::Workspace;
use tempfile::TempDir;

fn create_test_files(dir: &std::path::Path) {
    std::fs::write(dir.join("file1.txt"), b"content of file1").unwrap();
    std::fs::write(dir.join("file2.txt"), b"content of file2").unwrap();
    std::fs::create_dir(dir.join("subdir")).unwrap();
    std::fs::write(dir.join("subdir/nested.txt"), b"nested content").unwrap();
}

#[tokio::test]
async fn test_manifest_roundtrip_preserves_all_fields() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    create_test_files(src.path());

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();
    ws.save().unwrap();

    let loaded = manifest::load(&manifest_path(ws_dir.path())).unwrap();
    assert_eq!(loaded.entries.len(), ws.catalog.len());
    assert_eq!(loaded.source_roots.len(), ws.catalog.source_roots().len());
    assert_eq!(loaded.version, 1);
}

#[tokio::test]
async fn test_manifest_corrupt_json_returns_parse_error() {
    let ws_dir = TempDir::new().unwrap();
    Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    std::fs::write(manifest_path(ws_dir.path()), "{{{{not json at all").unwrap();

    let result = Workspace::open(ws_dir.path().to_path_buf());
    assert!(matches!(result, Err(AgentdirError::ManifestParse(_))));
}

#[tokio::test]
async fn test_manifest_wrong_version_rejected() {
    let ws_dir = TempDir::new().unwrap();
    Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    let bad_json = r#"{"version":99,"created_at_epoch_secs":0,"updated_at_epoch_secs":0,"source_roots":[],"entries":[]}"#;
    std::fs::write(manifest_path(ws_dir.path()), bad_json).unwrap();

    let result = Workspace::open(ws_dir.path().to_path_buf());
    assert!(matches!(result, Err(AgentdirError::ManifestParse(_))));
}

#[tokio::test]
async fn test_manifest_missing_file_returns_io_error() {
    let ws_dir = TempDir::new().unwrap();

    let result = Workspace::open(ws_dir.path().to_path_buf());
    assert!(matches!(result, Err(AgentdirError::Io(_))));
}

#[tokio::test]
async fn test_persist_after_map_then_reopen() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    create_test_files(src.path());

    let entry_count = {
        let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
        ws.map(
            SourcePath::new(src.path().to_path_buf()),
            VirtualPath::new("/docs").unwrap(),
        )
        .await
        .unwrap();
        ws.catalog.len()
    };

    let ws = Workspace::open(ws_dir.path().to_path_buf()).unwrap();
    assert_eq!(ws.catalog.len(), entry_count);
    assert_eq!(ws.catalog.source_roots().len(), 1);
}

#[tokio::test]
async fn test_persist_after_mv_then_reopen() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    std::fs::write(src.path().join("file.txt"), b"original").unwrap();

    let old_path = VirtualPath::new("/docs/file.txt").unwrap();
    let new_path = VirtualPath::new("/archive/file.txt").unwrap();

    {
        let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
        ws.map(
            SourcePath::new(src.path().to_path_buf()),
            VirtualPath::new("/docs").unwrap(),
        )
        .await
        .unwrap();
        ws.mv(&old_path, &new_path).unwrap();
        ws.save().unwrap();
    }

    let ws = Workspace::open(ws_dir.path().to_path_buf()).unwrap();
    assert!(ws.catalog.get(&new_path).is_ok());
    assert!(ws.catalog.get(&old_path).is_err());
}

#[tokio::test]
async fn test_persist_after_cp_then_reopen() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    std::fs::write(src.path().join("file.txt"), b"original").unwrap();

    let original_path = VirtualPath::new("/docs/file.txt").unwrap();
    let copied_path = VirtualPath::new("/docs/file_copy.txt").unwrap();

    let entry_count = {
        let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
        ws.map(
            SourcePath::new(src.path().to_path_buf()),
            VirtualPath::new("/docs").unwrap(),
        )
        .await
        .unwrap();
        let before_cp = ws.catalog.len();
        ws.cp(&original_path, &copied_path).unwrap();
        ws.save().unwrap();
        before_cp
    };

    let ws = Workspace::open(ws_dir.path().to_path_buf()).unwrap();
    assert!(ws.catalog.get(&original_path).is_ok());
    assert!(ws.catalog.get(&copied_path).is_ok());
    assert_eq!(ws.catalog.len(), entry_count + 1);
}

#[tokio::test]
async fn test_persist_after_unmap_then_reopen() {
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
        ws.unmap(&VirtualPath::new("/docs").unwrap()).unwrap();
        ws.save().unwrap();
    }

    let ws = Workspace::open(ws_dir.path().to_path_buf()).unwrap();
    assert_eq!(ws.catalog.len(), 0);
    assert_eq!(ws.catalog.source_roots().len(), 0);
}

#[tokio::test]
async fn test_no_tmp_file_left_after_save() {
    let ws_dir = TempDir::new().unwrap();
    let ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.save().unwrap();

    assert!(!manifest_path(ws_dir.path())
        .with_extension("json.tmp")
        .exists());
}

#[tokio::test]
async fn test_manifest_updated_at_advances() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    std::fs::write(src.path().join("file.txt"), b"content").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    let first_updated_at = ws.catalog.manifest.updated_at_epoch_secs;

    std::thread::sleep(std::time::Duration::from_millis(1100));

    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();
    let second_updated_at = ws.catalog.manifest.updated_at_epoch_secs;

    assert!(second_updated_at > first_updated_at);
}
