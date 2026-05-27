use agentdir::error::AgentdirError;
use agentdir::types::{EntryType, SourcePath, VirtualPath};
use agentdir::workspace::Workspace;
use tempfile::TempDir;

fn create_test_tree(dir: &std::path::Path) {
    std::fs::write(dir.join("root.txt"), b"root content").unwrap();
    std::fs::write(dir.join("note.md"), b"markdown").unwrap();
    std::fs::create_dir(dir.join("nested")).unwrap();
    std::fs::write(dir.join("nested/child.txt"), b"child content").unwrap();
}

async fn mapped_workspace() -> (TempDir, TempDir, Workspace) {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    create_test_tree(src.path());

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(
        SourcePath::new(src.path().canonicalize().unwrap()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    (src, ws_dir, ws)
}

#[tokio::test]
async fn test_exists_returns_true_for_mapped_file() {
    let (_src, _ws_dir, ws) = mapped_workspace().await;

    assert!(ws.exists(&VirtualPath::new("/docs/root.txt").unwrap()));
}

#[tokio::test]
async fn test_exists_returns_false_for_nonexistent() {
    let (_src, _ws_dir, ws) = mapped_workspace().await;

    assert!(!ws.exists(&VirtualPath::new("/docs/missing.txt").unwrap()));
}

#[tokio::test]
async fn test_exists_returns_true_for_directory() {
    let (_src, _ws_dir, ws) = mapped_workspace().await;

    assert!(ws.exists(&VirtualPath::new("/docs/nested").unwrap()));
}

#[tokio::test]
async fn test_stat_returns_metadata_for_file() {
    let (src, _ws_dir, ws) = mapped_workspace().await;
    let stat = ws.stat(&VirtualPath::new("/docs/root.txt").unwrap()).unwrap();
    let canonical_src = src.path().canonicalize().unwrap();

    assert_eq!(stat.virtual_path.as_str(), "/docs/root.txt");
    assert_eq!(stat.source_path.as_path(), canonical_src.join("root.txt"));
    assert_eq!(stat.size_bytes, 12);
    assert!(stat.mtime_ns > 0);
    assert_eq!(stat.entry_type, EntryType::File);
    assert!(stat.materialized);
}

#[tokio::test]
async fn test_stat_returns_metadata_for_directory() {
    let (src, _ws_dir, ws) = mapped_workspace().await;
    let stat = ws.stat(&VirtualPath::new("/docs/nested").unwrap()).unwrap();
    let canonical_src = src.path().canonicalize().unwrap();

    assert_eq!(stat.virtual_path.as_str(), "/docs/nested");
    assert_eq!(stat.source_path.as_path(), canonical_src.join("nested"));
    assert_eq!(stat.entry_type, EntryType::Directory);
    assert!(stat.materialized);
}

#[tokio::test]
async fn test_stat_errors_on_nonexistent() {
    let (_src, _ws_dir, ws) = mapped_workspace().await;
    let error = ws
        .stat(&VirtualPath::new("/docs/missing.txt").unwrap())
        .unwrap_err();

    assert!(matches!(error, AgentdirError::EntryNotFound(_)));
}

#[tokio::test]
async fn test_read_bytes_returns_source_content() {
    let (_src, _ws_dir, ws) = mapped_workspace().await;
    let bytes = ws
        .read_bytes(&VirtualPath::new("/docs/root.txt").unwrap())
        .await
        .unwrap();

    assert_eq!(bytes, b"root content");
}

#[tokio::test]
async fn test_read_bytes_reads_updated_source() {
    let (src, _ws_dir, ws) = mapped_workspace().await;
    std::fs::write(src.path().join("root.txt"), b"updated source").unwrap();

    let bytes = ws
        .read_bytes(&VirtualPath::new("/docs/root.txt").unwrap())
        .await
        .unwrap();

    assert_eq!(bytes, b"updated source");
}

#[tokio::test]
async fn test_read_bytes_errors_on_directory() {
    let (_src, _ws_dir, ws) = mapped_workspace().await;
    let error = ws
        .read_bytes(&VirtualPath::new("/docs/nested").unwrap())
        .await
        .unwrap_err();

    assert!(matches!(error, AgentdirError::InvalidPath(_)));
}

#[tokio::test]
async fn test_read_bytes_errors_on_nonexistent() {
    let (_src, _ws_dir, ws) = mapped_workspace().await;
    let error = ws
        .read_bytes(&VirtualPath::new("/docs/missing.txt").unwrap())
        .await
        .unwrap_err();

    assert!(matches!(error, AgentdirError::EntryNotFound(_)));
}

#[tokio::test]
async fn test_rglob_star_pattern() {
    let (_src, _ws_dir, ws) = mapped_workspace().await;
    let matches = ws.rglob("/docs/*.txt").unwrap();
    let paths: Vec<_> = matches
        .iter()
        .map(|entry| entry.virtual_path.as_str())
        .collect();

    assert_eq!(paths, vec!["/docs/root.txt"]);
}

#[tokio::test]
async fn test_rglob_recursive_doublestar() {
    let (_src, _ws_dir, ws) = mapped_workspace().await;
    let matches = ws.rglob("/docs/**/*.txt").unwrap();
    let mut paths: Vec<_> = matches
        .iter()
        .map(|entry| entry.virtual_path.as_str())
        .collect();
    paths.sort_unstable();

    assert_eq!(paths, vec!["/docs/nested/child.txt", "/docs/root.txt"]);
}

#[tokio::test]
async fn test_rglob_no_matches() {
    let (_src, _ws_dir, ws) = mapped_workspace().await;
    let matches = ws.rglob("/docs/**/*.rs").unwrap();

    assert!(matches.is_empty());
}

#[tokio::test]
async fn test_rglob_invalid_pattern_errors() {
    let (_src, _ws_dir, ws) = mapped_workspace().await;
    let result = ws.rglob("[invalid");
    assert!(result.is_err());
}

#[tokio::test]
async fn test_rglob_matches_directories() {
    let (_src, _ws_dir, ws) = mapped_workspace().await;
    let matches = ws.rglob("/docs/nested").unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].virtual_path.as_str(), "/docs/nested");
}
