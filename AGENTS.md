# AGENTS.md — agentdir

## What This Project Is

Virtual filesystem for agent-optimized file exploration using CoW reflinks.
Rust workspace with two crates (`agentdir`, `agentdir-cli`) and Python bindings (`bindings/python`).

## Project Scope

agentdir is **infrastructure-level plumbing** — it is intentionally NOT an AI intelligence layer.

- **Provides restructuring tools**: map, unmap, mv, cp, rename, mkdir, rmdir — enabling any consumer (AI agent, script, human) to reorganize a virtual file tree independently of the real source layout.
- **The restructuring agent is out of scope**: agentdir gives you the tools to restructure; it does not decide *what* to restructure or *why*. The "agent" in the name refers to the intended consumer, not something this project implements.
- **Targets all file types**: docx, pptx, images, binaries, source code — any file the OS can stat. agentdir is file-format-agnostic.
- **No file parsing**: agentdir does not read, interpret, index, or transform file contents. It tracks whether a file has been created, modified, or deleted via metadata (mtime + size), and materializes copies via CoW reflinks.
- **Change tracking is the core value**: accurate, cross-platform detection of source file mutations — additions, modifications, deletions — propagated to the virtual tree automatically.

## Out of Scope

The following are explicitly **not goals** of this project:

- AI/LLM integration, semantic understanding, or intelligent file routing
- File content parsing, full-text indexing, or search
- The orchestrator/agent that decides how to restructure the virtual tree
- File format conversion or transformation
- Dependency graph analysis, AST parsing, or language-aware features
- Access control, permissions, or multi-tenancy

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
- **Reflink**: CoW on APFS (macOS), Btrfs/XFS (Linux), byte-copy fallback on NTFS (Windows)
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
