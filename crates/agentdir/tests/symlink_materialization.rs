use agentdir::types::{MaterializeStrategy, SourcePath, VirtualPath};
use agentdir::workspace::Workspace;
use tempfile::TempDir;

#[tokio::test]
async fn test_symlink_mode_creates_symlinks() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    std::fs::write(src.path().join("file.txt"), b"hello symlink").unwrap();

    let mut ws =
        Workspace::init_with_strategy(ws_dir.path().to_path_buf(), MaterializeStrategy::Symlink)
            .unwrap();
    ws.map(
        SourcePath::new(src.path().canonicalize().unwrap()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    let mat = ws_dir.path().join("docs/file.txt");
    assert!(mat.symlink_metadata().unwrap().file_type().is_symlink());
}

#[tokio::test]
async fn test_symlink_mode_content_readable() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    std::fs::write(src.path().join("file.txt"), b"readable").unwrap();

    let mut ws =
        Workspace::init_with_strategy(ws_dir.path().to_path_buf(), MaterializeStrategy::Symlink)
            .unwrap();
    ws.map(
        SourcePath::new(src.path().canonicalize().unwrap()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(
        std::fs::read(ws_dir.path().join("docs/file.txt")).unwrap(),
        b"readable"
    );
}

#[tokio::test]
async fn test_symlink_mode_source_update_visible() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    std::fs::write(src.path().join("file.txt"), b"original").unwrap();

    let mut ws =
        Workspace::init_with_strategy(ws_dir.path().to_path_buf(), MaterializeStrategy::Symlink)
            .unwrap();
    ws.map(
        SourcePath::new(src.path().canonicalize().unwrap()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    std::fs::write(src.path().join("file.txt"), b"updated").unwrap();

    assert_eq!(
        std::fs::read(ws_dir.path().join("docs/file.txt")).unwrap(),
        b"updated"
    );
}

#[tokio::test]
async fn test_symlink_mode_dematerialize() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    std::fs::write(src.path().join("file.txt"), b"content").unwrap();

    let mut ws =
        Workspace::init_with_strategy(ws_dir.path().to_path_buf(), MaterializeStrategy::Symlink)
            .unwrap();
    ws.map(
        SourcePath::new(src.path().canonicalize().unwrap()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    ws.unmap(&VirtualPath::new("/docs").unwrap()).unwrap();
    assert!(!ws_dir.path().join("docs/file.txt").exists());
}

#[tokio::test]
async fn test_hardlink_mode_creates_hardlinks() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    std::fs::write(src.path().join("file.txt"), b"hardlink content").unwrap();

    let mut ws =
        Workspace::init_with_strategy(ws_dir.path().to_path_buf(), MaterializeStrategy::Hardlink)
            .unwrap();
    ws.map(
        SourcePath::new(src.path().canonicalize().unwrap()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    let mat = ws_dir.path().join("docs/file.txt");
    assert!(mat.exists());
    assert!(!mat.symlink_metadata().unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read(&mat).unwrap(), b"hardlink content");

    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let src_ino = std::fs::metadata(src.path().join("file.txt"))
            .unwrap()
            .ino();
        let mat_ino = std::fs::metadata(&mat).unwrap().ino();
        assert_eq!(src_ino, mat_ino);
    }
}

#[tokio::test]
async fn test_virtual_mode_no_files_on_disk() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    std::fs::write(src.path().join("file.txt"), b"virtual content").unwrap();

    let mut ws =
        Workspace::init_with_strategy(ws_dir.path().to_path_buf(), MaterializeStrategy::Virtual)
            .unwrap();
    ws.map(
        SourcePath::new(src.path().canonicalize().unwrap()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    assert!(!ws_dir.path().join("docs/file.txt").exists());
    assert!(ws.exists(&VirtualPath::new("/docs/file.txt").unwrap()));
}

#[tokio::test]
async fn test_virtual_mode_read_bytes_works() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    std::fs::write(src.path().join("file.txt"), b"backend read").unwrap();

    let mut ws =
        Workspace::init_with_strategy(ws_dir.path().to_path_buf(), MaterializeStrategy::Virtual)
            .unwrap();
    ws.map(
        SourcePath::new(src.path().canonicalize().unwrap()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    let bytes = ws
        .read_bytes(&VirtualPath::new("/docs/file.txt").unwrap())
        .await
        .unwrap();
    assert_eq!(bytes, b"backend read");
}

#[tokio::test]
async fn test_strategy_saved_in_manifest() {
    let ws_dir = TempDir::new().unwrap();

    {
        Workspace::init_with_strategy(ws_dir.path().to_path_buf(), MaterializeStrategy::Symlink)
            .unwrap();
    }

    let ws = Workspace::open(ws_dir.path().to_path_buf()).unwrap();
    assert_eq!(ws.catalog.manifest.strategy, MaterializeStrategy::Symlink);
}

#[tokio::test]
async fn test_default_strategy_is_reflink() {
    let ws_dir = TempDir::new().unwrap();
    let ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    assert_eq!(ws.catalog.manifest.strategy, MaterializeStrategy::Reflink);
}

#[tokio::test]
async fn test_legacy_manifest_defaults_to_reflink() {
    let ws_dir = TempDir::new().unwrap();
    Workspace::init(ws_dir.path().to_path_buf()).unwrap();

    let manifest_path = ws_dir.path().join(".agentdir/manifest.json");
    let json = std::fs::read_to_string(&manifest_path).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value.as_object_mut().unwrap().remove("strategy");
    std::fs::write(&manifest_path, serde_json::to_string_pretty(&value).unwrap()).unwrap();

    let ws = Workspace::open(ws_dir.path().to_path_buf()).unwrap();
    assert_eq!(ws.catalog.manifest.strategy, MaterializeStrategy::Reflink);
}

#[tokio::test]
async fn test_reflink_mode_unchanged() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    std::fs::write(src.path().join("file.txt"), b"reflink test").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(
        SourcePath::new(src.path().canonicalize().unwrap()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    let mat = ws_dir.path().join("docs/file.txt");
    assert!(mat.exists());
    assert!(!mat.symlink_metadata().unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read(&mat).unwrap(), b"reflink test");
}
