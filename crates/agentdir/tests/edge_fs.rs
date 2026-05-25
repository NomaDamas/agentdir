//! File system edge cases: special filenames, binary content, symlinks, permissions, large files.

use agentdir::types::{SourcePath, VirtualPath};
use agentdir::workspace::Workspace;
use tempfile::TempDir;

fn map_source<'a>(
    ws: &'a mut Workspace,
    src: &'a TempDir,
    mount: &'static str,
) -> impl std::future::Future<Output = agentdir::error::Result<agentdir::workspace::MapSummary>> + 'a
{
    ws.map(
        SourcePath::new(src.path().to_path_buf()),
        VirtualPath::new(mount).unwrap(),
    )
}

fn vp(path: &str) -> VirtualPath {
    VirtualPath::new(path).unwrap()
}

fn sleep_for_mtime() {
    std::thread::sleep(std::time::Duration::from_millis(10));
}

#[tokio::test]
async fn test_empty_source_directory_maps_cleanly() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    let summary = map_source(&mut ws, &src, "/empty").await.unwrap();

    assert!(summary.entries_added <= 1);
    assert!(ws.catalog.get(&vp("/empty")).is_ok() || ws.catalog.len() == 0);
    assert_eq!(summary.errors, 0);
}

#[tokio::test]
async fn test_source_dir_with_only_subdirs_no_files() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::create_dir_all(src.path().join("a/b/c")).unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    map_source(&mut ws, &src, "/docs").await.unwrap();

    assert!(ws.catalog.get(&vp("/docs/a")).is_ok());
    assert!(ws.catalog.get(&vp("/docs/a/b")).is_ok());
    assert!(ws.catalog.get(&vp("/docs/a/b/c")).is_ok());
    assert!(ws_dir.path().join("docs/a/b/c").is_dir());
}

#[tokio::test]
async fn test_binary_file_content_preserved() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    let bytes: Vec<u8> = (0..1024).map(|i| (i % 256) as u8).collect();
    std::fs::write(src.path().join("binary.bin"), &bytes).unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    map_source(&mut ws, &src, "/files").await.unwrap();

    assert_eq!(std::fs::read(ws_dir.path().join("files/binary.bin")).unwrap(), bytes);
}

#[tokio::test]
async fn test_large_file_materialization() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    let bytes: Vec<u8> = (0..(10 * 1024 * 1024)).map(|i| (i % 251) as u8).collect();
    std::fs::write(src.path().join("large.bin"), bytes).unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    map_source(&mut ws, &src, "/files").await.unwrap();

    let metadata = std::fs::metadata(ws_dir.path().join("files/large.bin")).unwrap();
    assert_eq!(metadata.len(), 10 * 1024 * 1024);
}

#[tokio::test]
async fn test_zero_byte_file() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::write(src.path().join("empty.txt"), b"").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    map_source(&mut ws, &src, "/docs").await.unwrap();

    let path = ws_dir.path().join("docs/empty.txt");
    assert!(path.exists());
    assert_eq!(std::fs::metadata(path).unwrap().len(), 0);
}

#[tokio::test]
async fn test_filename_with_spaces() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::write(src.path().join("file with spaces.txt"), b"spaces").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    map_source(&mut ws, &src, "/docs").await.unwrap();

    assert_eq!(
        std::fs::read(ws_dir.path().join("docs/file with spaces.txt")).unwrap(),
        b"spaces"
    );
}

#[tokio::test]
async fn test_filename_with_unicode() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::write(src.path().join("日本語ファイル.txt"), b"unicode").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    map_source(&mut ws, &src, "/docs").await.unwrap();

    assert_eq!(
        std::fs::read(ws_dir.path().join("docs/日本語ファイル.txt")).unwrap(),
        b"unicode"
    );
}

#[tokio::test]
async fn test_deeply_nested_directory_tree() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    let nested = (b'a'..=b't')
        .map(|ch| (ch as char).to_string())
        .collect::<Vec<_>>()
        .join("/");
    std::fs::create_dir_all(src.path().join(&nested)).unwrap();
    std::fs::write(src.path().join(&nested).join("leaf.txt"), b"leaf").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    map_source(&mut ws, &src, "/tree").await.unwrap();

    assert_eq!(
        std::fs::read(ws_dir.path().join("tree").join(&nested).join("leaf.txt")).unwrap(),
        b"leaf"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn test_symlink_in_source_is_skipped() {
    use std::os::unix::fs::symlink as unix_symlink;

    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::write(src.path().join("real.txt"), b"real").unwrap();
    unix_symlink(src.path().join("real.txt"), src.path().join("link.txt")).unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    map_source(&mut ws, &src, "/docs").await.unwrap();

    assert!(ws.catalog.get(&vp("/docs/real.txt")).is_ok());
    assert!(ws.catalog.get(&vp("/docs/link.txt")).is_err());
    assert!(!ws_dir.path().join("docs/link.txt").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn test_source_becomes_unreadable_after_map() {
    use std::os::unix::fs::PermissionsExt;

    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    let locked = src.path().join("locked.txt");
    std::fs::write(&locked, b"readable").unwrap();
    std::fs::write(src.path().join("other.txt"), b"other").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    map_source(&mut ws, &src, "/docs").await.unwrap();

    sleep_for_mtime();
    std::fs::write(&locked, b"changed").unwrap();
    let original_permissions = std::fs::metadata(&locked).unwrap().permissions();
    let mut unreadable_permissions = original_permissions.clone();
    unreadable_permissions.set_mode(0o000);
    std::fs::set_permissions(&locked, unreadable_permissions).unwrap();

    let summary = ws.refresh().await.unwrap();

    std::fs::set_permissions(&locked, original_permissions).unwrap();

    assert!(!summary.errors.is_empty());
    assert_eq!(std::fs::read(ws_dir.path().join("docs/other.txt")).unwrap(), b"other");
}

#[tokio::test]
async fn test_materialized_file_manually_deleted() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::write(src.path().join("file.txt"), b"original").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    map_source(&mut ws, &src, "/docs").await.unwrap();

    std::fs::remove_file(ws_dir.path().join("docs/file.txt")).unwrap();
    sleep_for_mtime();
    std::fs::write(src.path().join("file.txt"), b"modified").unwrap();

    let summary = ws.refresh().await.unwrap();

    assert!(summary.refreshed >= 1);
    assert_eq!(std::fs::read(ws_dir.path().join("docs/file.txt")).unwrap(), b"modified");
}

#[tokio::test]
async fn test_source_directory_deleted_entirely() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::write(src.path().join("file.txt"), b"content").unwrap();
    std::fs::create_dir(src.path().join("subdir")).unwrap();
    std::fs::write(src.path().join("subdir/nested.txt"), b"nested").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    map_source(&mut ws, &src, "/docs").await.unwrap();

    std::fs::remove_dir_all(src.path()).unwrap();

    let summary = ws.refresh().await.unwrap();

    assert!(summary.removed >= 2);
    assert_eq!(ws.catalog.len(), 0);
    assert!(!ws_dir.path().join("docs/file.txt").exists());
}

#[tokio::test]
async fn test_filename_with_special_chars() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    for name in ["file#1.txt", "file@2.txt", "file(3).txt"] {
        std::fs::write(src.path().join(name), name.as_bytes()).unwrap();
    }

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    map_source(&mut ws, &src, "/docs").await.unwrap();

    for name in ["file#1.txt", "file@2.txt", "file(3).txt"] {
        assert_eq!(std::fs::read(ws_dir.path().join("docs").join(name)).unwrap(), name.as_bytes());
    }
}

#[tokio::test]
async fn test_dot_files_and_dot_dirs() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::write(src.path().join(".hidden"), b"hidden").unwrap();
    std::fs::create_dir(src.path().join(".config")).unwrap();
    std::fs::write(src.path().join(".config/settings.json"), b"{}").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    map_source(&mut ws, &src, "/docs").await.unwrap();

    assert!(ws.catalog.get(&vp("/docs/.hidden")).is_ok());
    assert!(ws.catalog.get(&vp("/docs/.config/settings.json")).is_ok());
    assert_eq!(std::fs::read(ws_dir.path().join("docs/.hidden")).unwrap(), b"hidden");
    assert_eq!(
        std::fs::read(ws_dir.path().join("docs/.config/settings.json")).unwrap(),
        b"{}"
    );
}
