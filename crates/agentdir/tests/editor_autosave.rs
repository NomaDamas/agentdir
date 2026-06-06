use agentdir::backend::SourceEvent;
use agentdir::reconciler::Reconciler;
use agentdir::types::{SourcePath, VirtualPath};
use agentdir::workspace::Workspace;
use tempfile::TempDir;

fn vp(path: &str) -> VirtualPath {
    VirtualPath::new(path).unwrap()
}

async fn mapped_workspace(initial: &[u8]) -> (TempDir, TempDir, Workspace) {
    let src = TempDir::new().unwrap();
    let ws_dir = TempDir::new().unwrap();
    std::fs::write(src.path().join("note.txt"), initial).unwrap();

    let mut ws = Workspace::init(ws_dir.path().to_path_buf()).unwrap();
    ws.map(SourcePath::new(src.path().to_path_buf()), vp("/docs"))
        .await
        .unwrap();

    (src, ws_dir, ws)
}

#[tokio::test]
async fn refreshes_existing_mapping_when_atomic_save_emits_delete_then_create() {
    // Given: an editor replaces a tracked source file with a same-path temp rename.
    let (src, ws_dir, mut ws) = mapped_workspace(b"before").await;
    let source_path = SourcePath::new(src.path().join("note.txt"));
    std::fs::write(src.path().join("note.txt"), b"after atomic save").unwrap();
    let events = [
        SourceEvent::Deleted {
            path: source_path.clone(),
        },
        SourceEvent::Created { path: source_path },
    ];

    // When: watch-mode reconciliation applies the coalesced event batch.
    let actions = events
        .iter()
        .flat_map(|event| Reconciler::from_event(&ws.catalog, event).unwrap())
        .collect::<Vec<_>>();
    let summary = Reconciler::apply_actions(&mut ws.catalog, &ws.materializer, &actions).unwrap();

    // Then: the existing virtual path is refreshed, not removed or rejected.
    assert!(
        summary.errors.is_empty(),
        "atomic save delete/create should not preflight-fail: {:?}",
        summary.errors
    );
    assert_eq!(summary.added, 0);
    assert_eq!(summary.removed, 0);
    assert_eq!(summary.refreshed, 1);
    assert_eq!(
        std::fs::read(ws_dir.path().join("docs/note.txt")).unwrap(),
        b"after atomic save"
    );
    assert!(ws.catalog.get(&vp("/docs/note.txt")).is_ok());
}

#[tokio::test]
async fn refreshes_existing_mapping_when_atomic_save_emits_temp_rename_to_tracked_path() {
    // Given: an editor writes a temp file and renames it over the tracked path.
    let (src, ws_dir, mut ws) = mapped_workspace(b"before").await;
    let temp_path = SourcePath::new(src.path().join(".note.txt.tmp"));
    let target_path = SourcePath::new(src.path().join("note.txt"));
    std::fs::write(src.path().join("note.txt"), b"after rename save").unwrap();
    let event = SourceEvent::Renamed {
        from: temp_path,
        to: target_path,
    };

    // When: the rename event is reconciled through the watch event path.
    let actions = Reconciler::from_event(&ws.catalog, &event).unwrap();
    let summary = Reconciler::apply_actions(&mut ws.catalog, &ws.materializer, &actions).unwrap();

    // Then: the existing mapping is refreshed instead of trying to add a duplicate.
    assert!(
        summary.errors.is_empty(),
        "temp rename into tracked path should not duplicate-add: {:?}",
        summary.errors
    );
    assert_eq!(summary.added, 0);
    assert_eq!(summary.removed, 0);
    assert_eq!(summary.refreshed, 1);
    assert_eq!(
        std::fs::read(ws_dir.path().join("docs/note.txt")).unwrap(),
        b"after rename save"
    );
    assert!(ws.catalog.get(&vp("/docs/note.txt")).is_ok());
}

#[tokio::test]
async fn ignores_transient_temp_create_when_atomic_save_renames_it_to_tracked_path() {
    // Given: an editor creates a temp file and immediately renames it over a tracked file.
    let (src, ws_dir, mut ws) = mapped_workspace(b"before").await;
    let temp_path = SourcePath::new(src.path().join(".note.txt.tmp"));
    let target_path = SourcePath::new(src.path().join("note.txt"));
    std::fs::write(src.path().join("note.txt"), b"after temp create rename").unwrap();
    let events = [
        SourceEvent::Created {
            path: temp_path.clone(),
        },
        SourceEvent::Renamed {
            from: temp_path,
            to: target_path,
        },
    ];

    // When: watch-mode reconciliation handles the whole editor-save batch.
    let actions = events
        .iter()
        .flat_map(|event| Reconciler::from_event(&ws.catalog, event).unwrap())
        .collect::<Vec<_>>();
    let summary = Reconciler::apply_actions(&mut ws.catalog, &ws.materializer, &actions).unwrap();

    // Then: the vanished temp file is ignored and the tracked file is refreshed.
    assert!(
        summary.errors.is_empty(),
        "transient temp create should not block target refresh: {:?}",
        summary.errors
    );
    assert_eq!(summary.added, 0);
    assert_eq!(summary.removed, 0);
    assert_eq!(summary.refreshed, 1);
    assert_eq!(
        std::fs::read(ws_dir.path().join("docs/note.txt")).unwrap(),
        b"after temp create rename"
    );
    assert!(ws.catalog.get(&vp("/docs/.note.txt.tmp")).is_err());
    assert!(!ws_dir.path().join("docs/.note.txt.tmp").exists());
}

#[tokio::test]
async fn refreshes_all_virtual_aliases_when_atomic_save_replaces_copied_source() {
    // Given: a tracked source has an additional virtual copy.
    let (src, ws_dir, mut ws) = mapped_workspace(b"before").await;
    ws.cp(&vp("/docs/note.txt"), &vp("/copies/note.txt"))
        .unwrap();
    let source_path = SourcePath::new(src.path().join("note.txt"));
    std::fs::write(src.path().join("note.txt"), b"after copied autosave").unwrap();
    let events = [
        SourceEvent::Deleted {
            path: source_path.clone(),
        },
        SourceEvent::Created { path: source_path },
    ];

    // When: the editor-save event batch is applied.
    let actions = events
        .iter()
        .flat_map(|event| Reconciler::from_event(&ws.catalog, event).unwrap())
        .collect::<Vec<_>>();
    let summary = Reconciler::apply_actions(&mut ws.catalog, &ws.materializer, &actions).unwrap();

    // Then: every virtual alias for the source is refreshed and retained.
    assert!(summary.errors.is_empty());
    assert_eq!(summary.added, 0);
    assert_eq!(summary.removed, 0);
    assert_eq!(summary.refreshed, 2);
    for path in ["docs/note.txt", "copies/note.txt"] {
        assert_eq!(
            std::fs::read(ws_dir.path().join(path)).unwrap(),
            b"after copied autosave"
        );
    }
    assert!(ws.catalog.get(&vp("/docs/note.txt")).is_ok());
    assert!(ws.catalog.get(&vp("/copies/note.txt")).is_ok());
}

#[tokio::test]
async fn preserves_virtual_move_when_atomic_save_replaces_source() {
    // Given: a tracked source was moved in the virtual namespace.
    let (src, ws_dir, mut ws) = mapped_workspace(b"before").await;
    ws.mv(&vp("/docs/note.txt"), &vp("/renamed/note.txt"))
        .unwrap();
    let source_path = SourcePath::new(src.path().join("note.txt"));
    std::fs::write(src.path().join("note.txt"), b"after moved autosave").unwrap();
    let events = [
        SourceEvent::Deleted {
            path: source_path.clone(),
        },
        SourceEvent::Created { path: source_path },
    ];

    // When: the editor-save event batch is applied.
    let actions = events
        .iter()
        .flat_map(|event| Reconciler::from_event(&ws.catalog, event).unwrap())
        .collect::<Vec<_>>();
    let summary = Reconciler::apply_actions(&mut ws.catalog, &ws.materializer, &actions).unwrap();

    // Then: the virtual move is preserved and the canonical path is not recreated.
    assert!(summary.errors.is_empty());
    assert_eq!(summary.added, 0);
    assert_eq!(summary.removed, 0);
    assert_eq!(summary.refreshed, 1);
    assert_eq!(
        std::fs::read(ws_dir.path().join("renamed/note.txt")).unwrap(),
        b"after moved autosave"
    );
    assert!(ws.catalog.get(&vp("/renamed/note.txt")).is_ok());
    assert!(ws.catalog.get(&vp("/docs/note.txt")).is_err());
    assert!(!ws_dir.path().join("docs/note.txt").exists());
}

#[tokio::test]
async fn still_removes_mapping_when_editor_delete_is_not_recreated() {
    // Given: a tracked source file is removed without a replacement.
    let (src, ws_dir, mut ws) = mapped_workspace(b"before").await;
    let source_path = SourcePath::new(src.path().join("note.txt"));
    std::fs::remove_file(src.path().join("note.txt")).unwrap();
    let event = SourceEvent::Deleted { path: source_path };

    // When: the watch event is applied.
    let actions = Reconciler::from_event(&ws.catalog, &event).unwrap();
    let summary = Reconciler::apply_actions(&mut ws.catalog, &ws.materializer, &actions).unwrap();

    // Then: deletion still removes the virtual entry and materialized file.
    assert!(summary.errors.is_empty());
    assert_eq!(summary.added, 0);
    assert_eq!(summary.removed, 1);
    assert_eq!(summary.refreshed, 0);
    assert!(ws.catalog.get(&vp("/docs/note.txt")).is_err());
    assert!(!ws_dir.path().join("docs/note.txt").exists());
}

#[tokio::test]
async fn still_removes_mapping_when_stale_create_is_followed_by_delete() {
    // Given: a stale create event for a tracked path is followed by a real delete event.
    let (src, ws_dir, mut ws) = mapped_workspace(b"before").await;
    let source_path = SourcePath::new(src.path().join("note.txt"));
    std::fs::remove_file(src.path().join("note.txt")).unwrap();
    let events = [
        SourceEvent::Created {
            path: source_path.clone(),
        },
        SourceEvent::Deleted { path: source_path },
    ];

    // When: the stale create/delete batch is applied.
    let actions = events
        .iter()
        .flat_map(|event| Reconciler::from_event(&ws.catalog, event).unwrap())
        .collect::<Vec<_>>();
    let summary = Reconciler::apply_actions(&mut ws.catalog, &ws.materializer, &actions).unwrap();

    // Then: the stale create is ignored and the real delete removes the mapping.
    assert!(summary.errors.is_empty());
    assert_eq!(summary.added, 0);
    assert_eq!(summary.removed, 1);
    assert_eq!(summary.refreshed, 0);
    assert!(ws.catalog.get(&vp("/docs/note.txt")).is_err());
    assert!(!ws_dir.path().join("docs/note.txt").exists());
}

#[tokio::test]
async fn still_removes_mapping_when_stale_modify_is_followed_by_delete() {
    // Given: a stale modify event for a tracked path is followed by a real delete event.
    let (src, ws_dir, mut ws) = mapped_workspace(b"before").await;
    let source_path = SourcePath::new(src.path().join("note.txt"));
    std::fs::remove_file(src.path().join("note.txt")).unwrap();
    let events = [
        SourceEvent::Modified {
            path: source_path.clone(),
        },
        SourceEvent::Deleted { path: source_path },
    ];

    // When: the stale modify/delete batch is applied.
    let actions = events
        .iter()
        .flat_map(|event| Reconciler::from_event(&ws.catalog, event).unwrap())
        .collect::<Vec<_>>();
    let summary = Reconciler::apply_actions(&mut ws.catalog, &ws.materializer, &actions).unwrap();

    // Then: the stale modify is ignored and the real delete removes the mapping.
    assert!(summary.errors.is_empty());
    assert_eq!(summary.added, 0);
    assert_eq!(summary.removed, 1);
    assert_eq!(summary.refreshed, 0);
    assert!(ws.catalog.get(&vp("/docs/note.txt")).is_err());
    assert!(!ws_dir.path().join("docs/note.txt").exists());
}
