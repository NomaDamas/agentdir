//! Agent usage scenarios: realistic multi-source workflows, reorganization patterns, tool compatibility.

use agentdir::error::AgentdirError;
use agentdir::types::{SourcePath, VirtualPath};
use agentdir::workspace::Workspace;
use tempfile::TempDir;

fn vp(path: &str) -> VirtualPath {
    VirtualPath::new(path).unwrap()
}

fn source(path: &std::path::Path) -> SourcePath {
    SourcePath::new(path.to_path_buf())
}

#[tokio::test]
async fn test_multi_source_different_mounts() {
    let src = TempDir::new().unwrap();
    let docs = TempDir::new().unwrap();
    let assets = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::create_dir(src.path().join("bin")).unwrap();
    std::fs::write(src.path().join("main.rs"), b"fn main() {}").unwrap();
    std::fs::write(src.path().join("bin/tool.rs"), b"pub fn tool() {}").unwrap();
    std::fs::write(docs.path().join("README.md"), b"# project docs").unwrap();
    std::fs::write(assets.path().join("logo.svg"), b"<svg>agent</svg>").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(source(src.path()), vp("/src")).await.unwrap();
    ws.map(source(docs.path()), vp("/docs")).await.unwrap();
    ws.map(source(assets.path()), vp("/assets")).await.unwrap();

    assert!(ws.catalog.get(&vp("/src/main.rs")).is_ok());
    assert!(ws.catalog.get(&vp("/src/bin/tool.rs")).is_ok());
    assert!(ws.catalog.get(&vp("/docs/README.md")).is_ok());
    assert!(ws.catalog.get(&vp("/assets/logo.svg")).is_ok());
    assert!(matches!(
        ws.catalog.get(&vp("/src/README.md")),
        Err(AgentdirError::EntryNotFound(_))
    ));
    assert!(matches!(
        ws.catalog.get(&vp("/docs/logo.svg")),
        Err(AgentdirError::EntryNotFound(_))
    ));

    assert_eq!(
        std::fs::read(ws_dir.path().join("src/main.rs")).unwrap(),
        b"fn main() {}"
    );
    assert_eq!(
        std::fs::read(ws_dir.path().join("docs/README.md")).unwrap(),
        b"# project docs"
    );
    assert_eq!(
        std::fs::read(ws_dir.path().join("assets/logo.svg")).unwrap(),
        b"<svg>agent</svg>"
    );
}

#[tokio::test]
async fn test_reorganize_flat_to_categorized() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::write(src.path().join("main.rs"), b"fn main() {}").unwrap();
    std::fs::write(src.path().join("lib.rs"), b"pub mod api;").unwrap();
    std::fs::write(src.path().join("README.md"), b"# readme").unwrap();
    std::fs::write(src.path().join("guide.md"), b"usage guide").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(source(src.path()), vp("/flat")).await.unwrap();
    ws.mkdir(&vp("/code")).unwrap();
    ws.mkdir(&vp("/docs")).unwrap();

    ws.mv(&vp("/flat/main.rs"), &vp("/code/main.rs")).unwrap();
    ws.mv(&vp("/flat/lib.rs"), &vp("/code/lib.rs")).unwrap();
    ws.mv(&vp("/flat/README.md"), &vp("/docs/README.md"))
        .unwrap();
    ws.mv(&vp("/flat/guide.md"), &vp("/docs/guide.md")).unwrap();

    for path in [
        "/code/main.rs",
        "/code/lib.rs",
        "/docs/README.md",
        "/docs/guide.md",
    ] {
        assert!(ws.catalog.get(&vp(path)).is_ok(), "missing {path}");
    }
    for path in [
        "/flat/main.rs",
        "/flat/lib.rs",
        "/flat/README.md",
        "/flat/guide.md",
    ] {
        assert!(matches!(
            ws.catalog.get(&vp(path)),
            Err(AgentdirError::EntryNotFound(_))
        ));
        assert!(!ws_dir.path().join(path.trim_start_matches('/')).exists());
    }
    assert_eq!(
        std::fs::read(ws_dir.path().join("code/lib.rs")).unwrap(),
        b"pub mod api;"
    );
    assert_eq!(
        std::fs::read(ws_dir.path().join("docs/guide.md")).unwrap(),
        b"usage guide"
    );
}

#[tokio::test]
async fn test_map_unmap_remap_same_mount() {
    let src_a = TempDir::new().unwrap();
    let src_b = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::write(src_a.path().join("old.rs"), b"old code").unwrap();
    std::fs::write(src_b.path().join("new.rs"), b"new code").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(source(src_a.path()), vp("/code")).await.unwrap();
    ws.unmap(&vp("/code")).unwrap();
    ws.map(source(src_b.path()), vp("/code")).await.unwrap();

    assert!(matches!(
        ws.catalog.get(&vp("/code/old.rs")),
        Err(AgentdirError::EntryNotFound(_))
    ));
    assert!(ws.catalog.get(&vp("/code/new.rs")).is_ok());
    assert!(!ws_dir.path().join("code/old.rs").exists());
    assert_eq!(
        std::fs::read(ws_dir.path().join("code/new.rs")).unwrap(),
        b"new code"
    );
}

#[tokio::test]
async fn test_cp_then_unmap_preserves_copy() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::write(src.path().join("file.txt"), b"source-backed backup").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(source(src.path()), vp("/src")).await.unwrap();
    ws.mkdir(&vp("/backup")).unwrap();
    ws.cp(&vp("/src/file.txt"), &vp("/backup/file.txt"))
        .unwrap();
    let original_source = source(&src.path().join("file.txt"));

    ws.unmap(&vp("/src")).unwrap();

    assert!(matches!(
        ws.catalog.get(&vp("/src/file.txt")),
        Err(AgentdirError::EntryNotFound(_))
    ));
    let backup = ws.catalog.get(&vp("/backup/file.txt")).unwrap();
    assert_eq!(backup.source_path, original_source);
    assert_eq!(
        std::fs::read(ws_dir.path().join("backup/file.txt")).unwrap(),
        b"source-backed backup"
    );
}

#[tokio::test]
async fn test_virtual_mkdir_tree_for_organizing() {
    let ws_dir = TempDir::new().unwrap();
    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();

    ws.mkdir(&vp("/by-type")).unwrap();
    ws.mkdir(&vp("/by-type/docs")).unwrap();
    ws.mkdir(&vp("/by-type/code")).unwrap();

    for path in ["/by-type", "/by-type/docs", "/by-type/code"] {
        assert!(ws.catalog.get(&vp(path)).is_ok(), "missing {path}");
        assert!(ws_dir.path().join(path.trim_start_matches('/')).is_dir());
    }
}

#[tokio::test]
async fn test_rmdir_non_empty_without_recursive_fails() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::write(src.path().join("file.txt"), b"data").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(source(src.path()), vp("/data")).await.unwrap();

    let result = ws.rmdir(&vp("/data"), false);
    assert!(matches!(result, Err(AgentdirError::EntryExists(_))));
    assert!(ws.catalog.get(&vp("/data/file.txt")).is_ok());
}

#[tokio::test]
async fn test_rmdir_recursive_removes_subtree() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::write(src.path().join("keep.txt"), b"keep").unwrap();
    std::fs::write(src.path().join("move_a.txt"), b"move a").unwrap();
    std::fs::write(src.path().join("move_b.txt"), b"move b").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(source(src.path()), vp("/data")).await.unwrap();
    ws.mkdir(&vp("/temp")).unwrap();
    ws.mv(&vp("/data/move_a.txt"), &vp("/temp/move_a.txt"))
        .unwrap();
    ws.mv(&vp("/data/move_b.txt"), &vp("/temp/move_b.txt"))
        .unwrap();

    ws.rmdir(&vp("/temp"), true).unwrap();

    for path in ["/temp", "/temp/move_a.txt", "/temp/move_b.txt"] {
        assert!(matches!(
            ws.catalog.get(&vp(path)),
            Err(AgentdirError::EntryNotFound(_))
        ));
        assert!(!ws_dir.path().join(path.trim_start_matches('/')).exists());
    }
    assert!(ws.catalog.get(&vp("/data/keep.txt")).is_ok());
    assert_eq!(
        std::fs::read(ws_dir.path().join("data/keep.txt")).unwrap(),
        b"keep"
    );
}

#[tokio::test]
async fn test_status_reflects_operations() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::write(src.path().join("one.txt"), b"1").unwrap();
    std::fs::write(src.path().join("two.txt"), b"2").unwrap();
    std::fs::write(src.path().join("three.txt"), b"3").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    let initial = ws.status();
    assert_eq!(initial.total_entries, 0);
    assert_eq!(initial.source_roots, 0);
    assert_eq!(initial.materialized_root, ws_dir.path());
    assert!(initial.last_updated_epoch_secs > 0);

    ws.map(source(src.path()), vp("/status")).await.unwrap();
    let mapped = ws.status();
    assert!(mapped.total_entries > 0);
    assert_eq!(mapped.source_roots, 1);

    ws.unmap(&vp("/status")).unwrap();
    let unmapped = ws.status();
    assert_eq!(unmapped.total_entries, 0);
    assert_eq!(unmapped.source_roots, 0);
}

#[tokio::test]
async fn test_workspace_with_500_files_operations() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    for i in 0..500 {
        std::fs::write(
            src.path().join(format!("file{i:04}.txt")),
            format!("agent file {i}").as_bytes(),
        )
        .unwrap();
    }

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(source(src.path()), vp("/files")).await.unwrap();
    let after_map = ws.catalog.len();

    ws.mkdir(&vp("/moved")).unwrap();
    ws.mkdir(&vp("/copied")).unwrap();
    let after_dirs = ws.catalog.len();

    for i in 0..50 {
        ws.mv(
            &vp(&format!("/files/file{i:04}.txt")),
            &vp(&format!("/moved/file{i:04}.txt")),
        )
        .unwrap();
    }
    for i in 50..100 {
        ws.cp(
            &vp(&format!("/files/file{i:04}.txt")),
            &vp(&format!("/copied/file{i:04}.txt")),
        )
        .unwrap();
    }

    assert_eq!(ws.catalog.len(), after_dirs + 50);
    assert_eq!(after_dirs, after_map + 2);
    assert_eq!(
        std::fs::read(ws_dir.path().join("moved/file0000.txt")).unwrap(),
        b"agent file 0"
    );
    assert_eq!(
        std::fs::read(ws_dir.path().join("copied/file0050.txt")).unwrap(),
        b"agent file 50"
    );
    assert!(!ws_dir.path().join("files/file0000.txt").exists());
    assert!(ws_dir.path().join("files/file0050.txt").exists());

    std::thread::sleep(std::time::Duration::from_millis(10));
    let summary = ws.refresh().await.unwrap();
    assert_eq!(summary.added, 0);
    assert_eq!(summary.removed, 0);
    assert_eq!(summary.refreshed, 0);
}

#[tokio::test]
async fn test_rename_preserves_source_reference() {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();

    std::fs::write(src.path().join("old_name.txt"), b"rename me").unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(source(src.path()), vp("/docs")).await.unwrap();
    let old_path = vp("/docs/old_name.txt");
    let original_source = ws.catalog.get(&old_path).unwrap().source_path.clone();

    ws.rename(&old_path, "new_name.txt").unwrap();

    let renamed = ws.catalog.get(&vp("/docs/new_name.txt")).unwrap();
    assert_eq!(renamed.source_path, original_source);
    assert!(matches!(
        ws.catalog.get(&old_path),
        Err(AgentdirError::EntryNotFound(_))
    ));
    assert!(!ws_dir.path().join("docs/old_name.txt").exists());
    assert_eq!(
        std::fs::read(ws_dir.path().join("docs/new_name.txt")).unwrap(),
        b"rename me"
    );
}
