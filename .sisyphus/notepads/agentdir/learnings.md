- Workspace initialized with a minimal two-crate layout: a library crate and a CLI crate sharing workspace dependencies from the root `Cargo.toml`.
- The workspace built cleanly with the requested dependency set; the CLI binary uses `clap::Parser` only for versioned startup output and avoids adding domain logic.
- Keeping `Cargo.lock` ignored is appropriate here because this is a library-oriented workspace.
- `agentdir` now exposes `error` and `types` modules from `lib.rs`, with serde-serializable manifest/catalog types and a dedicated `AgentdirError` enum.
- `VirtualPath::new()` normalizes `.` and `..`, strips trailing separators, rejects empty input, and is covered by roundtrip/path tests.
- `cargo test -p agentdir` and `cargo clippy -p agentdir -- -D warnings` both completed successfully after adding the core schema types.
- `Backend` uses `async-trait` plus `tokio::sync::mpsc::Sender<SourceEvent>` so future backends can stream watcher events behind a trait object.
- `WatchHandle` must cancel its `CancellationToken` on drop; the cancellation test needs to keep a cloned child token for observation after the handle is moved.
- `reflink::clone_file` should remove an existing destination before calling `reflink_copy::reflink_or_copy`; this avoids macOS `clonefile()` `EEXIST` failures and keeps overwrite semantics predictable.
- `clone_file_verified` should compare hashes only when the caller opts in with `Some(expected_hash)`, so ordinary clones stay fast and hash verification remains explicit.
- Materializer owns only on-disk realization of supplied `CatalogEntry` values: files go through `reflink::clone_file`, directories use `create_dir_all`, symlinks replace existing links/files before creation, and no catalog state is touched.
- `materialize_all` should sort directories ahead of non-directories and then by virtual path depth so parent directories exist before nested entries; per-entry failures are accumulated in `MaterializeSummary` instead of aborting the batch.
- `manifest::save` should use pretty JSON plus `File::sync_all()` before `fs::rename()` so the final manifest path is never written directly and `.json.tmp` is cleaned up by the rename step.
- `manifest::load` must reject schema versions other than `1` even if deserialization succeeds, so version validation stays explicit and forward-incompatible manifests fail with `ManifestParse`.
- The manifest module can be tested in isolation with `cargo test -p agentdir manifest::tests`, which is useful for generating focused evidence without rerunning the full crate suite.

## Task 9 — Watcher (notify-debouncer-full)
- `notify-debouncer-full` 0.5 `Debouncer` implements `Watcher` directly — `.watcher()` is deprecated and returns `()`. Call `debouncer.watch()` not `debouncer.watcher().watch()`.
- `DebouncedEvent` implements `Deref<Target=Event>` so `.kind`, `.paths`, `.need_rescan()` work through deref.
- `new_debouncer(timeout, tick_rate, sender)` — `std::sync::mpsc::Sender<DebounceEventResult>` implements `DebounceEventHandler`.
- `tokio::runtime::Handle::current()` captured before `std::thread::spawn` to bridge sync→async event forwarding.
- All 3 watcher tests pass: file creation detection, cleanup on drop, periodic polling rescan.

## Task 10 — Reconciler
- Reconciler event conversion stays one-way from `SourceEvent` to `ChangeAction`: create/rename-to can synthesize placeholder file metadata, while full reconcile provides authoritative metadata from `Backend::scan`.
- Full reconciliation intentionally diffs only `mtime_ns` and `size_bytes`; content hashes remain lazy and are invalidated on refresh instead of recomputed.
- `Catalog::unmap` removes source roots if called on a mount, so reconciler removes only concrete virtual entry paths and leaves root mappings intact.
- Apply actions should accumulate per-entry materializer/catalog errors in `ReconcileSummary` and continue processing rather than aborting the whole batch.
