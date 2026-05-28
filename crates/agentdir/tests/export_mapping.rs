use agentdir::types::{MappingDirection, SourcePath, VirtualPath};
use agentdir::workspace::Workspace;
use tempfile::TempDir;

fn create_source_files(dir: &std::path::Path) {
    std::fs::write(dir.join("file1.txt"), b"hello").unwrap();
    std::fs::write(dir.join("file2.txt"), b"world").unwrap();
    std::fs::create_dir(dir.join("subdir")).unwrap();
    std::fs::write(dir.join("subdir/nested.txt"), b"nested").unwrap();
}

#[tokio::test]
async fn test_export_source_to_virtual() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    create_source_files(src.path());

    let canonical_src = src.path().canonicalize().unwrap();
    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(
        SourcePath::new(canonical_src.clone()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    let mapping = ws
        .export_mapping(MappingDirection::SourceToVirtual, None)
        .unwrap();
    assert_eq!(mapping.len(), 3);

    let src_key = canonical_src
        .join("file1.txt")
        .to_string_lossy()
        .to_string();
    assert_eq!(mapping.get(&src_key).unwrap(), "/docs/file1.txt");
}

#[tokio::test]
async fn test_export_virtual_to_source() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    create_source_files(src.path());

    let canonical_src = src.path().canonicalize().unwrap();
    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(
        SourcePath::new(canonical_src.clone()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    let mapping = ws
        .export_mapping(MappingDirection::VirtualToSource, None)
        .unwrap();
    assert_eq!(mapping.len(), 3);

    let expected_source = canonical_src
        .join("file1.txt")
        .to_string_lossy()
        .to_string();
    assert_eq!(mapping.get("/docs/file1.txt").unwrap(), &expected_source);
}

#[tokio::test]
async fn test_export_relative_to() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    create_source_files(src.path());

    let canonical_src = src.path().canonicalize().unwrap();
    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(
        SourcePath::new(canonical_src.clone()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    let mapping = ws
        .export_mapping(MappingDirection::SourceToVirtual, Some(&canonical_src))
        .unwrap();
    assert_eq!(mapping.len(), 3);
    assert_eq!(mapping.get("file1.txt").unwrap(), "/docs/file1.txt");
    assert_eq!(
        mapping.get("subdir/nested.txt").unwrap(),
        "/docs/subdir/nested.txt"
    );
}

#[tokio::test]
async fn test_export_empty_workspace() {
    let ws_dir = TempDir::new().unwrap();
    let ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();

    let mapping = ws
        .export_mapping(MappingDirection::SourceToVirtual, None)
        .unwrap();
    assert!(mapping.is_empty());
}

#[tokio::test]
async fn test_export_after_mv() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    std::fs::write(src.path().join("old.txt"), b"content").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(
        SourcePath::new(src.path().canonicalize().unwrap()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    ws.mv(
        &VirtualPath::new("/docs/old.txt").unwrap(),
        &VirtualPath::new("/docs/new.txt").unwrap(),
    )
    .unwrap();

    let mapping = ws
        .export_mapping(MappingDirection::VirtualToSource, None)
        .unwrap();
    assert!(mapping.contains_key("/docs/new.txt"));
    assert!(!mapping.contains_key("/docs/old.txt"));
}

#[tokio::test]
async fn test_export_directories_excluded() {
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

    ws.mkdir(&VirtualPath::new("/extra_dir").unwrap()).unwrap();

    let mapping = ws
        .export_mapping(MappingDirection::SourceToVirtual, None)
        .unwrap();
    for value in mapping.values() {
        assert_ne!(value, "/extra_dir");
    }
    assert_eq!(mapping.len(), 1);
}

#[tokio::test]
async fn test_export_multi_source() {
    let src1 = TempDir::new().unwrap();
    let src2 = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    std::fs::write(src1.path().join("a.txt"), b"aaa").unwrap();
    std::fs::write(src2.path().join("b.txt"), b"bbb").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(
        SourcePath::new(src1.path().canonicalize().unwrap()),
        VirtualPath::new("/src1").unwrap(),
    )
    .await
    .unwrap();
    ws.map(
        SourcePath::new(src2.path().canonicalize().unwrap()),
        VirtualPath::new("/src2").unwrap(),
    )
    .await
    .unwrap();

    let mapping = ws
        .export_mapping(MappingDirection::VirtualToSource, None)
        .unwrap();
    assert_eq!(mapping.len(), 2);
    assert!(mapping.contains_key("/src1/a.txt"));
    assert!(mapping.contains_key("/src2/b.txt"));
}

#[tokio::test]
async fn test_export_deterministic_order() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    for name in ["z.txt", "a.txt", "m.txt"] {
        std::fs::write(src.path().join(name), name.as_bytes()).unwrap();
    }

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(
        SourcePath::new(src.path().canonicalize().unwrap()),
        VirtualPath::new("/docs").unwrap(),
    )
    .await
    .unwrap();

    let mapping = ws
        .export_mapping(MappingDirection::VirtualToSource, None)
        .unwrap();
    let keys: Vec<&String> = mapping.keys().collect();
    let mut sorted_keys = keys.clone();
    sorted_keys.sort();
    assert_eq!(keys, sorted_keys);
}

#[tokio::test]
async fn test_export_source_to_virtual_rejects_cp_duplicates() {
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

    ws.cp(
        &VirtualPath::new("/docs/file.txt").unwrap(),
        &VirtualPath::new("/backup/file.txt").unwrap(),
    )
    .unwrap();

    let result = ws.export_mapping(MappingDirection::SourceToVirtual, None);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_export_virtual_to_source_works_after_cp() {
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

    ws.cp(
        &VirtualPath::new("/docs/file.txt").unwrap(),
        &VirtualPath::new("/backup/file.txt").unwrap(),
    )
    .unwrap();

    let mapping = ws
        .export_mapping(MappingDirection::VirtualToSource, None)
        .unwrap();
    assert_eq!(mapping.len(), 2);
    assert!(mapping.contains_key("/docs/file.txt"));
    assert!(mapping.contains_key("/backup/file.txt"));
}

#[tokio::test]
async fn test_export_invalid_relative_to_errors() {
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

    let result = ws.export_mapping(
        MappingDirection::SourceToVirtual,
        Some(std::path::Path::new(
            "/nonexistent/path/that/does/not/exist",
        )),
    );
    assert!(result.is_err());
}
