use agentdir::types::{MappingDirection, SourcePath, VirtualPath};
use agentdir::workspace::Workspace;
use tempfile::TempDir;

fn create_source_files(dir: &std::path::Path) {
    std::fs::write(dir.join("a.txt"), b"aaa").unwrap();
    std::fs::write(dir.join("b.txt"), b"bbb").unwrap();
    std::fs::write(dir.join("c.txt"), b"ccc").unwrap();
}

#[tokio::test]
async fn test_batch_map_single_file() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    std::fs::write(src.path().join("file.txt"), b"content").unwrap();
    let canonical_src = src.path().canonicalize().unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    let summary = ws
        .map_batch(vec![(
            SourcePath::new(canonical_src.join("file.txt")),
            VirtualPath::new("/organized/file.txt").unwrap(),
        )])
        .await
        .unwrap();

    assert_eq!(summary.entries_added, 1);
    assert!(ws.exists(&VirtualPath::new("/organized/file.txt").unwrap()));
    assert_eq!(
        std::fs::read(ws_dir.path().join("organized/file.txt")).unwrap(),
        b"content"
    );
}

#[tokio::test]
async fn test_batch_map_multiple_files() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    create_source_files(src.path());
    let canonical_src = src.path().canonicalize().unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    let mappings = vec![
        (
            SourcePath::new(canonical_src.join("a.txt")),
            VirtualPath::new("/docs/alpha.txt").unwrap(),
        ),
        (
            SourcePath::new(canonical_src.join("b.txt")),
            VirtualPath::new("/docs/beta.txt").unwrap(),
        ),
        (
            SourcePath::new(canonical_src.join("c.txt")),
            VirtualPath::new("/other/gamma.txt").unwrap(),
        ),
    ];

    let summary = ws.map_batch(mappings).await.unwrap();
    assert_eq!(summary.entries_added, 3);
    assert!(summary.errors.is_empty());
    assert_eq!(ws.catalog.len(), 3);
}

#[tokio::test]
async fn test_batch_map_creates_parent_dirs() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    std::fs::write(src.path().join("file.txt"), b"data").unwrap();
    let canonical_src = src.path().canonicalize().unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map_batch(vec![(
        SourcePath::new(canonical_src.join("file.txt")),
        VirtualPath::new("/deep/nested/path/file.txt").unwrap(),
    )])
    .await
    .unwrap();

    assert!(ws_dir.path().join("deep/nested/path/file.txt").exists());
}

#[tokio::test]
async fn test_batch_map_rejects_nonexistent_source() {
    let ws_dir = TempDir::new().unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    let result = ws
        .map_batch(vec![(
            SourcePath::new(std::path::PathBuf::from("/nonexistent/file.txt")),
            VirtualPath::new("/docs/file.txt").unwrap(),
        )])
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_batch_map_rejects_duplicate_virtual() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    std::fs::write(src.path().join("a.txt"), b"aaa").unwrap();
    std::fs::write(src.path().join("b.txt"), b"bbb").unwrap();
    let canonical_src = src.path().canonicalize().unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    let result = ws
        .map_batch(vec![
            (
                SourcePath::new(canonical_src.join("a.txt")),
                VirtualPath::new("/docs/same.txt").unwrap(),
            ),
            (
                SourcePath::new(canonical_src.join("b.txt")),
                VirtualPath::new("/docs/same.txt").unwrap(),
            ),
        ])
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_batch_map_rejects_existing_virtual() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    std::fs::write(src.path().join("a.txt"), b"aaa").unwrap();
    std::fs::write(src.path().join("b.txt"), b"bbb").unwrap();
    let canonical_src = src.path().canonicalize().unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map_batch(vec![(
        SourcePath::new(canonical_src.join("a.txt")),
        VirtualPath::new("/docs/existing.txt").unwrap(),
    )])
    .await
    .unwrap();

    let result = ws
        .map_batch(vec![(
            SourcePath::new(canonical_src.join("b.txt")),
            VirtualPath::new("/docs/existing.txt").unwrap(),
        )])
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_batch_map_empty_vec() {
    let ws_dir = TempDir::new().unwrap();
    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();

    let summary = ws.map_batch(vec![]).await.unwrap();
    assert_eq!(summary.entries_added, 0);
    assert!(summary.errors.is_empty());
}

#[tokio::test]
async fn test_batch_map_preserves_existing_entries() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    create_source_files(src.path());
    let canonical_src = src.path().canonicalize().unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(
        SourcePath::new(canonical_src.clone()),
        VirtualPath::new("/existing").unwrap(),
    )
    .await
    .unwrap();
    let count_before = ws.catalog.len();

    ws.map_batch(vec![(
        SourcePath::new(canonical_src.join("a.txt")),
        VirtualPath::new("/new/a.txt").unwrap(),
    )])
    .await
    .unwrap();

    assert_eq!(ws.catalog.len(), count_before + 1);
    assert!(ws.exists(&VirtualPath::new("/existing/a.txt").unwrap()));
    assert!(ws.exists(&VirtualPath::new("/new/a.txt").unwrap()));
}

#[tokio::test]
async fn test_batch_map_then_export() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    create_source_files(src.path());
    let canonical_src = src.path().canonicalize().unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map_batch(vec![
        (
            SourcePath::new(canonical_src.join("a.txt")),
            VirtualPath::new("/organized/alpha.txt").unwrap(),
        ),
        (
            SourcePath::new(canonical_src.join("b.txt")),
            VirtualPath::new("/organized/beta.txt").unwrap(),
        ),
    ])
    .await
    .unwrap();

    let mapping = ws
        .export_mapping(MappingDirection::VirtualToSource, None)
        .unwrap();
    assert_eq!(mapping.len(), 2);
    assert!(mapping.contains_key("/organized/alpha.txt"));
    assert!(mapping.contains_key("/organized/beta.txt"));
}

#[tokio::test]
async fn test_batch_map_manifest_persisted() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    std::fs::write(src.path().join("file.txt"), b"content").unwrap();
    let canonical_src = src.path().canonicalize().unwrap();

    {
        let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
        ws.map_batch(vec![(
            SourcePath::new(canonical_src.join("file.txt")),
            VirtualPath::new("/docs/file.txt").unwrap(),
        )])
        .await
        .unwrap();
    }

    let ws = Workspace::open(ws_dir.path().to_path_buf()).unwrap();
    assert_eq!(ws.catalog.len(), 1);
    assert!(ws.exists(&VirtualPath::new("/docs/file.txt").unwrap()));
}

#[tokio::test]
async fn test_batch_map_rejects_relative_virtual_path() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    std::fs::write(src.path().join("file.txt"), b"content").unwrap();
    let canonical_src = src.path().canonicalize().unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    let result = ws
        .map_batch(vec![(
            SourcePath::new(canonical_src.join("file.txt")),
            VirtualPath::new("relative/path.txt").unwrap(),
        )])
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_batch_map_rejects_directory_source() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    std::fs::create_dir(src.path().join("subdir")).unwrap();
    let canonical_src = src.path().canonicalize().unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    let result = ws
        .map_batch(vec![(
            SourcePath::new(canonical_src.join("subdir")),
            VirtualPath::new("/docs/subdir").unwrap(),
        )])
        .await;

    assert!(result.is_err());
}
