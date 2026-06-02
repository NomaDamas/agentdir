# AGENTS.md — agentdir

## What This Project Is

Read-only, agent-optimized navigation view for general-purpose file trees. agentdir restructures a virtual file tree over original files so AI agents, scripts, and humans can explore and navigate without touching the originals. The materialized view is read-only by design; when a consumer needs to edit, it resolves the original source path via `stat()`/`export_mapping()` and edits the original file directly.
Rust workspace with two crates (`agentdir`, `agentdir-cli`) and Python bindings (`bindings/python`).

## Project Scope

agentdir is **infrastructure-level plumbing** — it is intentionally NOT an AI intelligence layer.

- **Read-only navigation view**: the materialized virtual tree is for exploration only, not editing. Materialized files are set read-only (0o444 on Unix, read-only attribute on Windows) to enforce this contract.
- **Edits go to the original file**: when a consumer needs to modify a file, it resolves the original source path via `stat()` or `export_mapping()` and edits the original file directly. The original file remains writable; the virtual tree never is.
- **Provides restructuring tools**: map, unmap, mv, cp, rename, mkdir, rmdir — enabling any consumer (AI agent, script, human) to reorganize a virtual file tree independently of the real source layout.
- **The restructuring agent is out of scope**: agentdir gives you the tools to restructure; it does not decide *what* to restructure or *why*. That intelligence lives in a separate repository. The "agent" in the name refers to the intended consumer, not something this project implements.
- **Targets all file types**: documents, spreadsheets, presentations, PDFs, images, media, datasets, plain text, binaries — any file the OS can stat. agentdir is file-format-agnostic.
- **No file parsing**: agentdir does not read, interpret, index, or transform file contents. It tracks whether a file has been created, modified, or deleted via metadata (mtime + size), and materializes copies via CoW reflinks.
- **Change tracking is the core value**: accurate, cross-platform detection of original-file mutations — additions, modifications, deletions — propagated to the virtual tree automatically.

## Out of Scope

The following are explicitly **not goals** of this project:

- AI/LLM integration, semantic understanding, or intelligent file routing
- File content parsing, full-text indexing, or search
- The orchestrator/agent that decides how to restructure the virtual tree
- File format conversion or transformation
- Dependency graph analysis, AST parsing, or language-aware features
- Access control, permissions, or multi-tenancy
- Editing files in the virtual tree — the materialized view is read-only by design; all edits are delegated to the original source path

## Module Map (`crates/agentdir/src/`)

| Module | Purpose |
|--------|---------|
| `lib.rs` | Module re-exports, `version()` |
| `types.rs` | `VirtualPath`, `SourcePath`, `ContentHash`, `CatalogEntry`, `EntryType`, `SourceMetadata`, `Manifest` |
| `error.rs` | `AgentdirError` enum via `thiserror` |
| `catalog.rs` | In-memory virtual filesystem catalog with O(1) lookup index |
| `materializer.rs` | Creates real files on disk via CoW reflinks or byte-copy fallback |
| `manifest.rs` | Atomic JSON persistence (write-tmp, fsync, rename) |
| `reflink.rs` | Safe wrapper around `reflink_copy::reflink_or_copy` |
| `backend/mod.rs` | `Backend` trait: scan, metadata, read_bytes, watch |
| `backend/local.rs` | `LocalBackend`: WalkDir scanning, `notify` watcher with debounce |
| `reconciler.rs` | Change detection: source events to `ChangeAction`s, full reconciliation |
| `workspace.rs` | Top-level API facade: init, open, map, unmap, refresh, mv, cp |
| `watcher.rs` | `FileWatcher` with debounced events + periodic polling fallback |

## CLI (`crates/agentdir-cli/src/main.rs`)

Commands: `init`, `map`, `unmap`, `status`, `refresh`, `mv`, `cp`, `mkdir`, `rmdir`, `watch`

## Python Bindings (`bindings/python/`)

PyO3 bindings exposing `Workspace` class with full API: `init`, `open`, `map`, `unmap`, `mv`, `cp`, `mkdir`, `rmdir`, `rename`, `exists`, `stat`, `read_bytes`, `refresh`, `status`, `export_mapping`, `map_batch`, `list_snapshots`, `destroy_snapshot`.

| Path | Purpose |
|------|---------|
| `src/lib.rs` | PyO3 `#[pymodule]` — wraps `agentdir::Workspace` |
| `python/agentdir/__init__.py` | Re-exports from native `_agentdir` module |
| `python/agentdir/_agentdir.pyi` | PEP 561 type stubs |
| `tests/` | 78 pytest tests covering all API methods |
| `pyproject.toml` | maturin build, uv deps, ruff + pytest config |

## Cross-Platform Notes

- **VirtualPath** always uses `/` internally on all platforms
  - `types.rs` skips `Component::Prefix` (Windows drive letters)
  - `virtual_path_for_relative()` normalizes via component iteration — never uses `display()`
- **Reflink/Copy**: CoW on APFS (macOS), Btrfs/XFS (Linux), byte-copy fallback on NTFS (Windows). Materialized files are set read-only (0o444 on Unix, read-only attribute on Windows) to enforce the navigation-only contract.
- **Hardlink strategy removed**: a hardlink shares the source inode, so making it read-only would also make the source read-only; an in-place edit through a hardlink corrupts the source. Hardlink is not a supported materialization strategy.
- **File watcher**: FSEvents (macOS), inotify (Linux), ReadDirectoryChangesW (Windows)
- **Tests**: use `tempfile::TempDir` and `std::env::temp_dir()` — never hardcoded `/tmp`
- **Signal handling**: `tokio::signal::ctrl_c()` works cross-platform

## Build & Test

| Command | What it does |
|---------|-------------|
| `make test` | `cargo test --workspace` |
| `make lint` | `cargo fmt --check` + `cargo clippy` |
| `make ci` | fmt + clippy + test + doc + python-lint + python-test |
| `make python-build` | `cd bindings/python && uv run maturin develop` |
| `make python-test` | `cd bindings/python && uv run pytest -v` |
| `make python-lint` | `cd bindings/python && uv run ruff check . && uv run ruff format --check . && uv run deptry .` |
| `make python-fmt` | `cd bindings/python && uv run ruff format .` |
| `make docker-test` | Full Linux test via Docker |
| `make cross-build` | Windows cross-compilation check (compile-only) |
| `make cross-test` | Windows runtime tests via `cross` + Wine |
| `make cross-install` | Install `cross` tool |

## Key Invariants

1. Virtual paths always use forward slash `/` regardless of host OS
2. Materialized files are CoW clones when the filesystem supports it, byte-copies otherwise
3. Manifest is persisted atomically via write-tmp + fsync + rename (no partial writes)
4. Source symlinks are detected but not followed during scan (`follow_links: false`)
5. All tests use `tempfile::TempDir` for isolation — no global filesystem side effects
6. Source and materialized roots must not overlap (enforced by `validate_no_overlap`)
7. Materialized files are read-only (0o444 on Unix, read-only attribute on Windows). The virtual tree is navigation-only; the original file remains writable and is the sole edit target.

## Future / Out of Current Scope

**True zero-copy read-only view (not yet implemented).** On non-reflink filesystems (ext4, NTFS), the current byte-copy fallback duplicates disk usage. The planned next step is OS-level mount mechanisms that give a read-only view without copying data at all:

| Platform | Mechanism | Notes |
|----------|-----------|-------|
| Linux | bind mount + `remount,ro` | Per-mount read-only; source inode stays writable |
| Windows | WinFsp or ProjFS passthrough | Read-only virtual filesystem driver |
| macOS | Already handled | APFS reflinks are effectively zero-copy |

This is not yet implemented. The current materialization (reflink or byte-copy + chmod 0o444) is the enforced contract today.

**Materialization intelligence is out of scope here.** Deciding what to map where — which files belong in which virtual paths, how to restructure a file collection for agent consumption — is implemented in a separate repository. agentdir provides the primitives; the strategy lives elsewhere.
