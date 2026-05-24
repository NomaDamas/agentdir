# agentdir — Virtual File System for Agent-Optimized Exploration

## TL;DR

> **Quick Summary**: Build a Rust library + CLI (`agentdir`) that creates virtual filesystem views of local directories using CoW reflink cloning. Agents see a reorganized file tree optimized for exploration, backed by zero-copy reflinks to the original files. Change tracking automatically detects source file mutations and keeps the virtual tree in sync.
> 
> **Deliverables**:
> - `agentdir` library crate (core VFS logic, catalog, materializer, watcher)
> - `agentdir-cli` binary crate (CLI for managing virtual trees)
> - Abstract backend trait for future extensibility (Google Drive, GitHub, etc.)
> - Local filesystem backend with CoW reflink materialization
> - Event-driven change tracking with auto-sync
> 
> **Estimated Effort**: Large
> **Parallel Execution**: YES — 4 waves
> **Critical Path**: Task 1 → Task 3 → Task 5 → Task 7 → Task 9 → Task 11 → Task 13

---

## Context

### Original Request
Build a Rust implementation of a virtual file system based on a design note describing a "Virtual Catalog + Materialized Sandbox" architecture. The VFS maps virtual paths to real local files via CoW reflinks without copying data. Must support standard POSIX tools (ripgrep, find, cat). Must track changes when source files are modified/added/deleted and auto-update the virtual tree. Future extensibility to Google Drive, GitHub, Slack backends required but not implemented in v1.

### Interview Summary
**Key Discussions**:
- **v1 scope is local-only**: User initially mentioned Google Drive/GitHub but refined to local filesystem only for v1. Abstract trait for extensibility.
- **Materialization required**: Agent needs real files for ripgrep/find/cat. Pure virtual path resolution insufficient.
- **Persistent tree**: Materialized tree survives restarts. Not ephemeral per-session.
- **Full POSIX operations**: mkdir, rmdir, mv, cp, ln, rename on virtual namespace.
- **Change tracking direction**: Physical → Virtual only. Source changes auto-propagate to virtual tree.
- **Change policies**: Delete → remove, Modified → auto-refresh, New file → auto-add.
- **TDD**: RED-GREEN-REFACTOR for all tasks.

**Research Findings**:
- Design note gist: Comprehensive 3-layer architecture with detailed threat model for symlink/hardlink/FUSE pitfalls. CoW reflink is the proven approach.
- Mirage project: Shows multi-backend VFS pattern with resource traits. North star for future extensibility.
- `reflink` crate: macOS `clonefile()` fails if destination exists — must remove before reflinking.
- `notify-debouncer-full` crate: Handles event deduplication, rename correlation. `need_rescan()` flag indicates events may have been dropped.

### Metis Review
**Identified Gaps** (addressed):
- **Remove-before-reflink**: macOS `clonefile()` returns `EEXIST` if destination exists. All reflink ops must go through a wrapper that removes first.
- **Atomic manifest writes**: Write to `.tmp` → `fsync` → `rename` to prevent corruption on crash.
- **`need_rescan()` handling**: When notify signals possible event loss, must trigger full reconciliation.
- **Symlink traversal**: `walkdir` must use `follow_links(false)` by default to prevent infinite loops.
- **Overlapping paths**: Must validate source and materialized roots don't overlap.
- **Case sensitivity**: macOS APFS is case-insensitive; must handle or reject case-conflicting virtual paths.
- **Manifest versioning**: Include `version` field from day 1 for future migration.
- **mtime+size fast path**: Don't hash eagerly; use mtime+size for change detection, hash lazily/on-demand.
- **Rapid bulk changes**: Debouncer + batched materialization for scenarios like `git checkout`.

---

## Work Objectives

### Core Objective
Build a Rust library that creates, manages, and synchronizes virtual filesystem views of local directories, using CoW reflink cloning for zero-copy materialization and event-driven change tracking.

### Concrete Deliverables
- `crates/agentdir/` — Library crate with: catalog, materializer, watcher, backend trait, local backend
- `crates/agentdir-cli/` — CLI binary with: init, map, unmap, mv, cp, ln, mkdir, rmdir, status, refresh, watch commands
- JSON manifest format for persisting virtual catalogs
- Comprehensive test suite (TDD)

### Definition of Done
- [ ] `cargo test --workspace` passes with 0 failures
- [ ] `cargo clippy --workspace -- -D warnings` passes with 0 warnings
- [ ] CLI can: create a virtual tree, map source dirs, materialize with reflinks, detect+auto-sync changes
- [ ] ripgrep/find/cat work on the materialized tree and return correct content
- [ ] Source file modifications are auto-detected and materialized tree is refreshed

### Must Have
- CoW reflink materialization with copy fallback
- Remove-before-reflink wrapper (macOS `clonefile` EEXIST fix)
- Atomic manifest writes (write-tmp → fsync → rename)
- Event-driven change detection via `notify-debouncer-full`
- Full reconciliation on `need_rescan()` events
- Periodic polling fallback
- mtime+size fast-path change detection (lazy SHA-256)
- Overlap validation (source ≠ materialized root)
- Manifest version field from v1
- `walkdir` with `follow_links(false)` by default
- Full POSIX-like virtual operations (mkdir, rmdir, mv, cp, ln, rename)

### Must NOT Have (Guardrails)
- ❌ Remote backends implementation (Google Drive, GitHub, Slack) — trait only
- ❌ Write-back from materialized tree to source
- ❌ Bidirectional sync or conflict resolution
- ❌ Reorganization/auto-organization logic (library provides operations, not intelligence)
- ❌ FUSE mounting or overlay filesystems
- ❌ File content indexing or full-text search
- ❌ Daemon/service management (systemd, launchd) — `agentdir watch` runs in foreground
- ❌ File permission management beyond what reflink/copy inherits
- ❌ Direct calls to `reflink::reflink_or_copy` — always go through wrapper
- ❌ Eager SHA-256 hashing on initial scan
- ❌ Tests that depend on specific filesystem type, require root, or depend on timing (sleep)

---

## Verification Strategy

> **ZERO HUMAN INTERVENTION** — ALL verification is agent-executed. No exceptions.

### Test Decision
- **Infrastructure exists**: NO (greenfield) — will be created in Task 1
- **Automated tests**: TDD (RED-GREEN-REFACTOR)
- **Framework**: `cargo test` (built-in Rust test framework)
- **TDD flow**: Each task writes failing test first → implements to pass → refactors

### QA Policy
Every task MUST include agent-executed QA scenarios.
Evidence saved to `.sisyphus/evidence/task-{N}-{scenario-slug}.{ext}`.

- **Library code**: Use `cargo test` — assert function outputs, file existence, content correctness
- **CLI**: Use `assert_cmd` crate for integration testing of binary
- **Filesystem operations**: Use `tempfile::TempDir` for isolated test fixtures
- **Watcher tests**: Use `tokio::time::timeout` to prevent hanging; assert events via channels

---

## Execution Strategy

### Parallel Execution Waves

```
Wave 1 (Foundation — start immediately, Task 1 first, then 2/3/4 parallel):
├── Task 1: Cargo workspace scaffolding + CI config [quick]
│   then parallel:
├── Task 2: Core data types + manifest schema [quick]
├── Task 3: Backend trait definition [quick]
└── Task 4: Reflink wrapper module [quick]

Wave 2 (After Wave 1 — core modules, ALL 4 PARALLEL):
├── Task 5: Catalog (virtual tree CRUD operations) [deep]
├── Task 6: Local filesystem backend [unspecified-high]
├── Task 7: Materializer engine [deep]
└── Task 8: Manifest persistence (atomic JSON I/O) [quick]

Wave 3 (After Wave 2 — change tracking + integration, SEQUENTIAL dependencies):
├── Task 9: File watcher (notify integration) [unspecified-high]  ← starts immediately after Wave 2
├── Task 10: Change reconciler (diff + propagate) [deep]          ← starts after Task 9
├── Task 11: Workspace integration [deep]                          ← starts after Tasks 9 + 10
└── Task 12: Bulk operations + batched materialization [unspecified-high]  ← starts after Task 11

Wave 4 (CLI + polish — after Task 11):
├── Task 13: CLI binary + core commands [unspecified-high]  ← parallel with 14
├── Task 14: CLI watch command (long-running) [unspecified-high]  ← parallel with 13
└── Task 15: End-to-end integration tests [deep]  ← after 12, 13, 14

Wave FINAL (After ALL tasks — 4 parallel reviews, then user okay):
├── Task F1: Plan compliance audit (oracle)
├── Task F2: Code quality review (unspecified-high)
├── Task F3: Real manual QA (unspecified-high)
└── Task F4: Scope fidelity check (deep)
-> Present results -> Get explicit user okay

Critical Path: Task 1 → Task 2 → Task 5 → Task 9 → Task 10 → Task 11 → Task 12 → Task 15 → F1-F4
```

### Dependency Matrix

| Task | Depends On | Blocks |
|------|-----------|--------|
| 1 | — | 2, 3, 4 |
| 2 | 1 | 5, 6, 7, 8 |
| 3 | 1 | 6 |
| 4 | 1 | 7 |
| 5 | 2 | 9, 10, 11, 13 |
| 6 | 2, 3 | 9, 11 |
| 7 | 2, 4 | 11, 12 |
| 8 | 2 | 11, 13 |
| 9 | 5, 6 | 10, 11, 14 |
| 10 | 5, 9 | 11 |
| 11 | 5, 6, 7, 8, 9, 10 | 12, 13, 14, 15 |
| 12 | 7, 11 | 15 |
| 13 | 5, 8, 11 | 15 |
| 14 | 9, 11, 13 | 15 |
| 15 | 11, 12, 13, 14 | F1-F4 |

> **Note on parallelism within waves**: Tasks 5, 6, 7, 8 in Wave 2 are fully parallel. In Wave 3, Tasks 9 runs in parallel with the start, but Task 10 starts after Task 9, and Tasks 11 and 12 are sequential (11 first, then 12). In Wave 4, Tasks 13 and 14 can be parallel after Task 11, and Task 15 waits for all.

### Agent Dispatch Summary

- **Wave 1**: **4 tasks** — T1 `quick`, T2 `quick`, T3 `quick`, T4 `quick`
- **Wave 2**: **4 tasks** — T5 `deep`, T6 `unspecified-high`, T7 `deep`, T8 `quick`
- **Wave 3**: **4 tasks** — T9 `unspecified-high`, T10 `deep`, T11 `deep`, T12 `unspecified-high`
- **Wave 4**: **3 tasks** — T13 `unspecified-high`, T14 `unspecified-high`, T15 `deep`
- **FINAL**: **4 tasks** — F1 `oracle`, F2 `unspecified-high`, F3 `unspecified-high`, F4 `deep`

---

## TODOs

- [x] 1. Cargo Workspace Scaffolding + CI Config

  **What to do**:
  - Create Cargo workspace with root `Cargo.toml` containing `[workspace]` with members `crates/agentdir` and `crates/agentdir-cli`
  - `crates/agentdir/` — library crate with `src/lib.rs` containing a trivial public function and one passing test
  - `crates/agentdir-cli/` — binary crate with `src/main.rs` containing a minimal `fn main()` that prints version
  - Add workspace-level dependencies in root `Cargo.toml` under `[workspace.dependencies]`:
    - `serde = { version = "1", features = ["derive"] }`
    - `serde_json = "1"`
    - `sha2 = "0.10"`
    - `reflink-copy = "0.1"` (note: the actively maintained fork of `reflink`)
    - `notify = "8"`
    - `notify-debouncer-full = "0.5"`
    - `walkdir = "2"`
    - `tokio = { version = "1", features = ["full"] }`
    - `clap = { version = "4", features = ["derive"] }`
    - `thiserror = "2"`
    - `tempfile = "3"`
    - `tracing = "0.1"`
    - `tracing-subscriber = "0.3"`
  - Add `.gitignore` for Rust (`/target`, `Cargo.lock` for lib)
  - Add `rustfmt.toml` with `edition = "2021"`
  - Verify: `cargo test --workspace` passes, `cargo clippy --workspace -- -D warnings` passes

  **Must NOT do**:
  - Do not add any domain logic yet
  - Do not add remote backend dependencies

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with Tasks 2, 3, 4 — but this one must complete first as others depend on Cargo.toml)
  - **Blocks**: Tasks 2, 3, 4
  - **Blocked By**: None

  **References**:
  - **External**: Cargo workspace documentation: https://doc.rust-lang.org/cargo/reference/workspaces.html
  - **External**: `reflink-copy` crate (maintained fork): https://crates.io/crates/reflink-copy

  **Acceptance Criteria**:
  - [ ] `cargo test --workspace` → PASS (at least 1 test)
  - [ ] `cargo clippy --workspace -- -D warnings` → PASS (0 warnings)
  - [ ] `cargo build --workspace` → successful build
  - [ ] `crates/agentdir/src/lib.rs` exists with `pub fn` and `#[cfg(test)]` module
  - [ ] `crates/agentdir-cli/src/main.rs` exists and compiles

  **QA Scenarios**:
  ```
  Scenario: Workspace builds and tests pass
    Tool: Bash
    Preconditions: Fresh clone of repo
    Steps:
      1. Run `cargo build --workspace`
      2. Run `cargo test --workspace`
      3. Run `cargo clippy --workspace -- -D warnings`
    Expected Result: All three commands exit with code 0
    Failure Indicators: Non-zero exit code, compilation errors, clippy warnings
    Evidence: .sisyphus/evidence/task-1-workspace-build.txt

  Scenario: CLI binary runs
    Tool: Bash
    Preconditions: `cargo build --workspace` succeeded
    Steps:
      1. Run `cargo run -p agentdir-cli -- --version`
    Expected Result: Prints version string and exits with code 0
    Failure Indicators: Panic, non-zero exit code
    Evidence: .sisyphus/evidence/task-1-cli-runs.txt
  ```

  **Commit**: YES
  - Message: `chore: initialize cargo workspace with agentdir lib + cli crates`
  - Files: `Cargo.toml`, `crates/agentdir/Cargo.toml`, `crates/agentdir/src/lib.rs`, `crates/agentdir-cli/Cargo.toml`, `crates/agentdir-cli/src/main.rs`, `.gitignore`, `rustfmt.toml`
  - Pre-commit: `cargo test --workspace && cargo clippy --workspace -- -D warnings`

- [x] 2. Core Data Types + Manifest Schema

  **What to do**:
  - **RED**: Write tests for `CatalogEntry` serialization/deserialization, `VirtualPath`/`SourcePath` newtype correctness, `EntryType` variants, `Manifest` schema with version field
  - **GREEN**: Implement in `crates/agentdir/src/types.rs`:
    - `VirtualPath(PathBuf)` — newtype for virtual namespace paths, with normalization (strip trailing slash, resolve `.` and `..`)
    - `SourcePath(PathBuf)` — newtype for real filesystem paths, with canonicalization
    - `ContentHash` — newtype wrapping `[u8; 32]` (SHA-256), with hex display, `Option<ContentHash>` for lazy computation
    - `EntryType` — enum: `File`, `Directory`, `Symlink { target: PathBuf }`
    - `SourceMetadata` — struct: `mtime_ns: u128`, `size_bytes: u64`, `entry_type: EntryType`
    - `CatalogEntry` — struct: `virtual_path: VirtualPath`, `source_path: SourcePath`, `content_hash: Option<ContentHash>`, `metadata: SourceMetadata`, `materialized: bool`
    - `Manifest` — struct: `version: u32` (always 1 for v1), `created_at_epoch_secs: u64`, `updated_at_epoch_secs: u64`, `source_roots: Vec<SourceRoot>`, `entries: Vec<CatalogEntry>` (timestamps are seconds since Unix epoch via `SystemTime::now().duration_since(UNIX_EPOCH)`)
    - `SourceRoot` — struct: `source_path: SourcePath`, `virtual_mount: VirtualPath`, `recursive: bool`
  - Implement in `crates/agentdir/src/error.rs`:
    - `AgentdirError` enum using `thiserror`: `Io`, `ManifestParse`, `ManifestWrite`, `PathOverlap`, `EntryNotFound`, `EntryExists`, `ReflinkFailed`, `WatcherError`, `HashMismatch`
  - **REFACTOR**: Ensure all types derive `Debug, Clone, Serialize, Deserialize` where appropriate. `VirtualPath` and `SourcePath` implement `Display`, `AsRef<Path>`, `From<PathBuf>`.
  - Add `pub mod types;` and `pub mod error;` to `lib.rs`

  **Must NOT do**:
  - Do not implement any logic — these are pure data types
  - Do not add `sha2` computation logic yet (just the hash wrapper type)
  - Do not add chrono/time dependency — use `std::time::SystemTime` or `serde` with `u64` epoch seconds

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES (after Task 1)
  - **Parallel Group**: Wave 1 completion → Wave 2 start (with Tasks 3, 4)
  - **Blocks**: Tasks 5, 6, 7, 8
  - **Blocked By**: Task 1

  **References**:
  - **Pattern**: Design note gist — manifest entry has `virtual_path`, `corpus_path`, `content_hash`
  - **External**: `thiserror` derive macro: https://docs.rs/thiserror
  - **External**: `serde` derive: https://serde.rs/derive.html

  **Acceptance Criteria**:
  - [ ] `cargo test -p agentdir` → PASS: serialization roundtrip tests for all types
  - [ ] `CatalogEntry` serializes to JSON and deserializes back to identical struct
  - [ ] `VirtualPath` normalizes paths (`/foo/bar/` → `/foo/bar`, `/foo/./bar` → `/foo/bar`)
  - [ ] `Manifest` includes `version: 1` field
  - [ ] `AgentdirError` has all specified variants

  **QA Scenarios**:
  ```
  Scenario: CatalogEntry JSON roundtrip
    Tool: Bash (cargo test)
    Preconditions: Types module compiled
    Steps:
      1. Run `cargo test -p agentdir types::tests::test_catalog_entry_roundtrip`
    Expected Result: Test passes — CatalogEntry serializes to JSON and deserializes to identical value
    Failure Indicators: Assertion failure on field comparison
    Evidence: .sisyphus/evidence/task-2-types-roundtrip.txt

  Scenario: VirtualPath normalization edge cases
    Tool: Bash (cargo test)
    Preconditions: Types module compiled
    Steps:
      1. Run `cargo test -p agentdir types::tests::test_virtual_path_normalization`
    Expected Result: Trailing slashes stripped, `.` resolved, `..` resolved, empty path rejected
    Failure Indicators: Assertion failure on normalized path value
    Evidence: .sisyphus/evidence/task-2-path-normalization.txt
  ```

  **Commit**: YES
  - Message: `feat(core): define catalog entry types, manifest schema, and error types`
  - Files: `crates/agentdir/src/types.rs`, `crates/agentdir/src/error.rs`, `crates/agentdir/src/lib.rs`
  - Pre-commit: `cargo test -p agentdir`

- [x] 3. Backend Trait Definition

  **What to do**:
  - **RED**: Write tests that validate `LocalBackend` (placeholder) implements the trait, test trait object creation
  - **GREEN**: Implement in `crates/agentdir/src/backend.rs`:
    - `SourceEvent` enum: `Created { path: SourcePath }`, `Modified { path: SourcePath }`, `Deleted { path: SourcePath }`, `Renamed { from: SourcePath, to: SourcePath }`, `RescanNeeded`
    - `#[async_trait] pub trait Backend: Send + Sync`:
      - `async fn scan(&self, root: &SourcePath) -> Result<Vec<(SourcePath, SourceMetadata)>>` — list all files under a root
      - `async fn metadata(&self, path: &SourcePath) -> Result<SourceMetadata>` — get metadata for one file
      - `async fn read_bytes(&self, path: &SourcePath) -> Result<Vec<u8>>` — read file content (for hashing)
      - `async fn watch(&self, roots: &[SourcePath], tx: tokio::sync::mpsc::Sender<SourceEvent>) -> Result<WatchHandle>` — start watching for changes
      - `fn name(&self) -> &str` — backend name ("local", "gdrive", etc.)
      - `fn supports_reflink(&self) -> bool` — whether materialization can use reflinks (true for local, false for remote)
    - `WatchHandle` — struct that stops watching on drop (wraps a cancellation token)
  - **REFACTOR**: Ensure trait is object-safe where possible. Add `Box<dyn Backend>` usage test.
  - Add `pub mod backend;` to `lib.rs`

  **Must NOT do**:
  - Do not implement `LocalBackend` logic yet (just a struct that compiles with `todo!()` bodies)
  - Do not implement any remote backend
  - Do not add `notify` dependency usage yet

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Tasks 2, 4 after Task 1)
  - **Parallel Group**: Wave 1 completion
  - **Blocks**: Task 6
  - **Blocked By**: Task 1

  **References**:
  - **Pattern**: Mirage's resource interface — each backend implements a common trait with `read`, `list`, `watch` methods
  - **External**: `async-trait` crate: https://docs.rs/async-trait

  **Acceptance Criteria**:
  - [ ] `Backend` trait compiles and has all specified methods
  - [ ] `LocalBackend` struct exists and implements `Backend` (with `todo!()` bodies)
  - [ ] Trait is `Send + Sync` bounded
  - [ ] `WatchHandle` exists as a type

  **QA Scenarios**:
  ```
  Scenario: Backend trait is implementable
    Tool: Bash (cargo test)
    Preconditions: Backend module compiled
    Steps:
      1. Run `cargo test -p agentdir backend::tests::test_local_backend_implements_trait`
    Expected Result: LocalBackend successfully created as Box<dyn Backend>
    Failure Indicators: Compilation error, trait object safety issues
    Evidence: .sisyphus/evidence/task-3-trait-compiles.txt
  ```

  **Commit**: YES
  - Message: `feat(core): define async backend trait with LocalBackend placeholder`
  - Files: `crates/agentdir/src/backend.rs`, `crates/agentdir/src/lib.rs`
  - Pre-commit: `cargo test -p agentdir`

- [x] 4. Reflink Wrapper Module

  **What to do**:
  - **RED**: Write tests for: reflink-or-copy succeeds on a temp file, remove-before-reflink handles existing destination, copy fallback works, error on non-existent source
  - **GREEN**: Implement in `crates/agentdir/src/reflink.rs`:
    - `pub fn clone_file(src: &Path, dst: &Path) -> Result<CloneResult>` — the safe wrapper:
      1. If `dst` exists, `remove_file(dst)`
      2. If `dst` parent doesn't exist, `create_dir_all(dst.parent())`
      3. Call `reflink_copy::reflink_or_copy(src, dst)`
      4. Return `CloneResult::Reflinked` or `CloneResult::Copied(bytes)`
    - `CloneResult` enum: `Reflinked`, `Copied(u64)`
    - `pub fn clone_file_verified(src: &Path, dst: &Path, expected_hash: Option<&ContentHash>) -> Result<CloneResult>` — clone + optional SHA-256 verification of result
    - Helper: `pub fn compute_hash(path: &Path) -> Result<ContentHash>` — SHA-256 of file content
  - **REFACTOR**: Ensure all filesystem operations use proper error context via `thiserror`. Log reflink vs copy decision via `tracing::debug!`.
  - Add `pub mod reflink;` to `lib.rs`

  **Must NOT do**:
  - Do not call `reflink_copy::reflink` or `reflink_copy::reflink_or_copy` directly anywhere else — this module is the ONLY entry point
  - Do not test for specific reflink behavior (test env may not support it) — test that `clone_file` succeeds regardless of mechanism
  - Do not block on hash computation for large files — the verified variant is opt-in

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Tasks 2, 3 after Task 1)
  - **Parallel Group**: Wave 1 completion
  - **Blocks**: Task 7
  - **Blocked By**: Task 1

  **References**:
  - **Pattern**: Metis identified macOS `clonefile()` EEXIST issue — must remove destination before reflinking
  - **External**: `reflink-copy` crate API: `reflink_or_copy(from, to) -> io::Result<Option<u64>>`
  - **External**: `sha2` crate for SHA-256: https://docs.rs/sha2

  **Acceptance Criteria**:
  - [ ] `clone_file` succeeds on temp files (reflink or copy)
  - [ ] `clone_file` handles pre-existing destination (removes and re-clones)
  - [ ] `clone_file` creates parent directories if missing
  - [ ] `clone_file` returns appropriate `CloneResult` variant
  - [ ] `compute_hash` returns correct SHA-256 for known content
  - [ ] Error returned for non-existent source file

  **QA Scenarios**:
  ```
  Scenario: Clone file with existing destination
    Tool: Bash (cargo test)
    Preconditions: None (uses tempfile)
    Steps:
      1. Run `cargo test -p agentdir reflink::tests::test_clone_overwrites_existing`
    Expected Result: Creates source file, creates destination file with different content, calls clone_file, verifies destination has source content
    Failure Indicators: Destination content doesn't match source, EEXIST error
    Evidence: .sisyphus/evidence/task-4-clone-overwrite.txt

  Scenario: Clone file creates parent dirs
    Tool: Bash (cargo test)
    Preconditions: None (uses tempfile)
    Steps:
      1. Run `cargo test -p agentdir reflink::tests::test_clone_creates_parents`
    Expected Result: Cloning to a/b/c/file.txt where a/b/c/ doesn't exist succeeds
    Failure Indicators: "No such file or directory" error
    Evidence: .sisyphus/evidence/task-4-clone-parents.txt

  Scenario: SHA-256 hash correctness
    Tool: Bash (cargo test)
    Preconditions: None (uses tempfile)
    Steps:
      1. Run `cargo test -p agentdir reflink::tests::test_compute_hash`
    Expected Result: Hash of known content matches pre-computed SHA-256 value
    Failure Indicators: Hash mismatch
    Evidence: .sisyphus/evidence/task-4-hash-correct.txt
  ```

  **Commit**: YES
  - Message: `feat(core): implement reflink wrapper with remove-before-clone and copy fallback`
  - Files: `crates/agentdir/src/reflink.rs`, `crates/agentdir/src/lib.rs`
  - Pre-commit: `cargo test -p agentdir`

- [x] 5. Catalog — Virtual Tree CRUD Operations

  **What to do**:
  - **RED**: Write tests for each virtual tree operation: `map` (single file and directory), `unmap`, `mkdir`, `rmdir`, `mv`, `cp`, `ln` (virtual symlink), `rename`, `list`, `get`, `resolve`. Test overlap rejection (source root ⊆ materialized root or vice versa). Test case-conflict detection on case-insensitive systems.
  - **GREEN**: Implement in `crates/agentdir/src/catalog.rs`:
    - `Catalog` struct containing: `manifest: Manifest`, `entry_index: HashMap<VirtualPath, usize>` (for O(1) lookup)
    - `Catalog::new(materialized_root: PathBuf) -> Self`
    - `Catalog::add_source_root(&mut self, source_root: SourceRoot) -> Result<()>` — register a source root mapping. Validates no overlap with materialized_root.
    - `Catalog::add_entries(&mut self, entries: Vec<CatalogEntry>) -> Result<()>` — add pre-scanned entries to the catalog. Validates no duplicate virtual paths. (The scanning itself is done by `Backend::scan` in `Workspace::map`, NOT by Catalog — Catalog stays pure data.)
    - `Catalog::unmap(&mut self, virtual_mount: &VirtualPath) -> Result<Vec<CatalogEntry>>` — remove all entries under a virtual mount. Returns removed entries.
    - `Catalog::mkdir(&mut self, path: &VirtualPath) -> Result<()>` — create virtual directory (no source backing)
    - `Catalog::rmdir(&mut self, path: &VirtualPath) -> Result<()>` — remove virtual directory (must be empty or recursive flag)
    - `Catalog::mv(&mut self, from: &VirtualPath, to: &VirtualPath) -> Result<()>` — move/rename entry in virtual namespace
    - `Catalog::cp(&mut self, from: &VirtualPath, to: &VirtualPath) -> Result<()>` — copy entry (new virtual path, same source)
    - `Catalog::ln(&mut self, target: &VirtualPath, link: &VirtualPath) -> Result<()>` — virtual symlink
    - `Catalog::rename(&mut self, path: &VirtualPath, new_name: &str) -> Result<()>` — rename without moving
    - `Catalog::list(&self, path: &VirtualPath) -> Result<Vec<&CatalogEntry>>` — list children
    - `Catalog::get(&self, path: &VirtualPath) -> Result<&CatalogEntry>` — lookup single entry
    - `Catalog::resolve(&self, virtual_path: &VirtualPath) -> Result<&SourcePath>` — resolve to source
    - `Catalog::entries(&self) -> &[CatalogEntry]` — all entries
    - `Catalog::validate_no_overlap(source: &Path, materialized: &Path) -> Result<()>` — static check
  - **REFACTOR**: Extract path manipulation helpers. Ensure all mutations update both `manifest.entries` and `entry_index`.
  - Add `pub mod catalog;` to `lib.rs`

  **Must NOT do**:
  - Do not trigger materialization from catalog operations — catalog is pure data
  - Do not interact with the filesystem (no `fs::` calls) — catalog is a virtual data structure
  - Do not implement change detection logic

  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: Core domain logic with many methods, edge cases, and index consistency requirements
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Tasks 6, 7, 8)
  - **Parallel Group**: Wave 2
  - **Blocks**: Tasks 9, 10, 11, 13
  - **Blocked By**: Task 2 (needs types module for CatalogEntry, VirtualPath, etc.)

  **References**:
  - **Type References**: `crates/agentdir/src/types.rs` — `CatalogEntry`, `VirtualPath`, `SourcePath`, `Manifest`
  - **Pattern**: Filesystem namespace operations — this is essentially an in-memory filesystem tree with path-based operations
  - **Edge case**: Metis identified overlap validation, case-sensitivity, and symlink handling

  **Acceptance Criteria**:
  - [ ] `add_source_root` registers a source root and validates no overlap
  - [ ] `add_entries` adds pre-scanned entries to the catalog with correct virtual paths
  - [ ] `unmap` removes all entries under a virtual mount
  - [ ] `mv` changes virtual path, preserves source reference
  - [ ] `cp` creates new entry with same source reference
  - [ ] `ln` creates virtual symlink entry
  - [ ] `list` returns correct children for a directory
  - [ ] `resolve` returns source path for a virtual path
  - [ ] Overlap validation rejects source ⊆ materialized and materialized ⊆ source
  - [ ] All operations maintain entry_index consistency

  **QA Scenarios**:
  ```
  Scenario: Add entries and list directory contents
    Tool: Bash (cargo test)
    Preconditions: Types module available
    Steps:
      1. Run `cargo test -p agentdir catalog::tests::test_add_entries_and_list`
    Expected Result: Adding pre-scanned entries creates catalog entries; listing the mount point returns them
    Failure Indicators: Missing entries, wrong virtual paths
    Evidence: .sisyphus/evidence/task-5-add-list.txt

  Scenario: Move preserves source reference
    Tool: Bash (cargo test)
    Preconditions: Catalog with mapped entries
    Steps:
      1. Run `cargo test -p agentdir catalog::tests::test_mv_preserves_source`
    Expected Result: After mv, old path not found, new path resolves to same source
    Failure Indicators: Source path changed, old path still found
    Evidence: .sisyphus/evidence/task-5-mv-source.txt

  Scenario: Overlap rejection
    Tool: Bash (cargo test)
    Preconditions: None
    Steps:
      1. Run `cargo test -p agentdir catalog::tests::test_overlap_rejection`
    Expected Result: PathOverlap error when source is subdir of materialized root
    Failure Indicators: No error, wrong error variant
    Evidence: .sisyphus/evidence/task-5-overlap.txt
  ```

  **Commit**: YES
  - Message: `feat(catalog): implement virtual tree CRUD operations`
  - Files: `crates/agentdir/src/catalog.rs`, `crates/agentdir/src/lib.rs`
  - Pre-commit: `cargo test -p agentdir`

- [x] 6. Local Filesystem Backend

  **What to do**:
  - **RED**: Write tests for: scanning a temp directory returns correct entries, metadata matches real file stats, read_bytes returns correct content, scan respects `follow_links(false)`, scan handles symlinks as `EntryType::Symlink`
  - **GREEN**: Implement `LocalBackend` in `crates/agentdir/src/backend/local.rs`:
    - `LocalBackend` struct (stateless for now — no config needed for local FS)
    - `scan`: Use `walkdir::WalkDir` with `follow_links(false)`. For each entry, create `(SourcePath, SourceMetadata)` with mtime from `fs::metadata()`, size, and entry type. Symlinks recorded as `EntryType::Symlink { target }`.
    - `metadata`: Call `fs::symlink_metadata` (not `fs::metadata` — don't follow symlinks). Convert to `SourceMetadata`.
    - `read_bytes`: `fs::read(path)` — simple file read.
    - `watch`: Placeholder — `todo!()` for now (implemented in Task 9).
    - `name`: Returns `"local"`.
    - `supports_reflink`: Returns `true`.
  - Refactor `crates/agentdir/src/backend.rs` into `crates/agentdir/src/backend/mod.rs` + `local.rs`
  - **REFACTOR**: Ensure mtime is captured as nanoseconds since epoch (`u128`). Handle platform differences (macOS vs Linux mtime precision).

  **Must NOT do**:
  - Do not implement `watch` yet (Task 9)
  - Do not follow symlinks in `scan`
  - Do not compute SHA-256 during scan (lazy hashing)

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: Filesystem interaction with platform-specific edge cases (symlinks, mtime precision)
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Tasks 5, 7, 8)
  - **Parallel Group**: Wave 2
  - **Blocks**: Tasks 9, 11
  - **Blocked By**: Tasks 2, 3

  **References**:
  - **Trait Reference**: `crates/agentdir/src/backend.rs` — `Backend` trait definition
  - **Type References**: `crates/agentdir/src/types.rs` — `SourcePath`, `SourceMetadata`, `EntryType`
  - **External**: `walkdir` crate — `WalkDir::new(root).follow_links(false)`
  - **Edge case**: Metis identified `follow_links(false)` requirement and symlink handling

  **Acceptance Criteria**:
  - [ ] `scan` returns all files in a temp directory with correct source paths
  - [ ] `scan` does NOT follow symlinks (records them as `EntryType::Symlink`)
  - [ ] `metadata` returns correct size and mtime for a known file
  - [ ] `read_bytes` returns correct content for a known file
  - [ ] `scan` handles empty directories
  - [ ] `scan` handles nested directories

  **QA Scenarios**:
  ```
  Scenario: Scan temp directory
    Tool: Bash (cargo test)
    Preconditions: None (uses tempfile)
    Steps:
      1. Run `cargo test -p agentdir backend::local::tests::test_scan_directory`
    Expected Result: Creates temp dir with files, scans, verifies all files listed with correct metadata
    Failure Indicators: Missing files, wrong metadata
    Evidence: .sisyphus/evidence/task-6-scan-dir.txt

  Scenario: Symlinks not followed
    Tool: Bash (cargo test)
    Preconditions: None (uses tempfile)
    Steps:
      1. Run `cargo test -p agentdir backend::local::tests::test_symlinks_not_followed`
    Expected Result: Symlink recorded as EntryType::Symlink with correct target, not followed
    Failure Indicators: Symlink followed, target file content returned instead of link metadata
    Evidence: .sisyphus/evidence/task-6-symlinks.txt
  ```

  **Commit**: YES
  - Message: `feat(backend): implement LocalBackend with walkdir scanning and metadata`
  - Files: `crates/agentdir/src/backend/mod.rs`, `crates/agentdir/src/backend/local.rs`
  - Pre-commit: `cargo test -p agentdir`

- [x] 7. Materializer Engine

  **What to do**:
  - **RED**: Write tests for: materialize single file (creates reflinked/copied file at virtual path), materialize directory structure, dematerialize (remove) file, dematerialize directory, refresh (re-clone after source change), materialize with missing parent dirs
  - **GREEN**: Implement in `crates/agentdir/src/materializer.rs`:
    - `Materializer` struct: `materialized_root: PathBuf`
    - `Materializer::new(root: PathBuf) -> Result<Self>` — validates root exists, creates if not
    - `pub fn materialize_entry(&self, entry: &CatalogEntry) -> Result<MaterializeResult>`:
      - For `File`: call `reflink::clone_file(entry.source_path, materialized_root.join(entry.virtual_path))`
      - For `Directory`: `fs::create_dir_all(materialized_root.join(entry.virtual_path))`
      - For `Symlink`: `std::os::unix::fs::symlink(target, materialized_root.join(entry.virtual_path))`
    - `pub fn dematerialize_entry(&self, virtual_path: &VirtualPath) -> Result<()>`:
      - Remove the file/dir at `materialized_root.join(virtual_path)`
    - `pub fn refresh_entry(&self, entry: &CatalogEntry) -> Result<MaterializeResult>`:
      - Dematerialize + re-materialize (handles the remove-before-reflink pattern)
    - `pub fn materialize_all(&self, entries: &[CatalogEntry]) -> Result<MaterializeSummary>`:
      - Materialize all entries. Create directories first (sorted by depth), then files.
      - Return summary: `reflinked: usize`, `copied: usize`, `dirs_created: usize`, `errors: Vec<(VirtualPath, AgentdirError)>`
    - `pub fn materialized_path(&self, virtual_path: &VirtualPath) -> PathBuf`:
      - Return `materialized_root.join(virtual_path)`
    - `MaterializeResult` enum: `Reflinked`, `Copied(u64)`, `DirCreated`, `SymlinkCreated`
    - `MaterializeSummary` struct
  - **REFACTOR**: Use the `reflink::clone_file` wrapper exclusively (never direct `reflink_copy` calls). Log each operation via `tracing::info!`.

  **Must NOT do**:
  - Do not interact with the Catalog — materializer only knows about `CatalogEntry` structs
  - Do not implement change detection — materializer is commanded, not autonomous
  - Do not skip the remove-before-reflink wrapper

  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: Core module with filesystem operations, platform differences, error recovery
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Tasks 5, 6, 8)
  - **Parallel Group**: Wave 2
  - **Blocks**: Tasks 11, 12
  - **Blocked By**: Tasks 2, 4

  **References**:
  - **Module Reference**: `crates/agentdir/src/reflink.rs` — `clone_file`, `CloneResult` (MUST use this, not raw reflink_copy)
  - **Type References**: `crates/agentdir/src/types.rs` — `CatalogEntry`, `VirtualPath`, `SourcePath`
  - **Pattern**: Design note gist — materialization creates real files at virtual paths

  **Acceptance Criteria**:
  - [ ] Single file materialization creates a readable file at the virtual path
  - [ ] Content of materialized file matches source file
  - [ ] Directory materialization creates directory structure
  - [ ] `dematerialize_entry` removes the materialized file/dir
  - [ ] `refresh_entry` updates a materialized file after source modification
  - [ ] `materialize_all` handles mixed files and directories correctly
  - [ ] Parent directories are auto-created

  **QA Scenarios**:
  ```
  Scenario: Materialize and verify content
    Tool: Bash (cargo test)
    Preconditions: None (uses tempfile)
    Steps:
      1. Run `cargo test -p agentdir materializer::tests::test_materialize_file_content`
    Expected Result: Source file cloned to virtual path, content matches via fs::read
    Failure Indicators: Content mismatch, file not created
    Evidence: .sisyphus/evidence/task-7-materialize-content.txt

  Scenario: Refresh after source modification
    Tool: Bash (cargo test)
    Preconditions: None (uses tempfile)
    Steps:
      1. Run `cargo test -p agentdir materializer::tests::test_refresh_after_modification`
    Expected Result: Source modified, refresh called, materialized file has new content
    Failure Indicators: Old content still present, EEXIST error
    Evidence: .sisyphus/evidence/task-7-refresh.txt
  ```

  **Commit**: YES
  - Message: `feat(materializer): implement persistent materialization engine with reflink cloning`
  - Files: `crates/agentdir/src/materializer.rs`, `crates/agentdir/src/lib.rs`
  - Pre-commit: `cargo test -p agentdir`

- [x] 8. Manifest Persistence (Atomic JSON I/O)

  **What to do**:
  - **RED**: Write tests for: save manifest to file, load manifest from file, roundtrip (save → load → compare), atomic write survives simulated crash (temp file exists, rename succeeds), load rejects manifest with unknown version
  - **GREEN**: Implement in `crates/agentdir/src/manifest.rs`:
    - `pub fn save(manifest: &Manifest, path: &Path) -> Result<()>`:
      1. Serialize manifest to JSON with `serde_json::to_string_pretty`
      2. Write to `path.with_extension("json.tmp")` via `std::fs::write`
      3. Call `File::open(tmp_path)?.sync_all()?` (fsync)
      4. `std::fs::rename(tmp_path, path)` (atomic on POSIX)
    - `pub fn load(path: &Path) -> Result<Manifest>`:
      1. `std::fs::read_to_string(path)`
      2. `serde_json::from_str` → `Manifest`
      3. Validate `version == 1` (reject unknown versions with `ManifestParse` error)
    - `pub fn manifest_path(workspace_root: &Path) -> PathBuf`:
      - Returns `workspace_root/.agentdir/manifest.json`
    - `pub fn ensure_workspace_dir(workspace_root: &Path) -> Result<PathBuf>`:
      - Creates `.agentdir/` dir if it doesn't exist, returns path
  - **REFACTOR**: Add `tracing::info!` for save/load operations with file size.

  **Must NOT do**:
  - Do not use `serde_json::to_writer` directly to the manifest file (not atomic)
  - Do not leave `.json.tmp` files on success (rename replaces atomically)
  - Do not accept manifests with `version != 1`

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: Small module, straightforward I/O with atomic write pattern
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Tasks 5, 6, 7)
  - **Parallel Group**: Wave 2
  - **Blocks**: Tasks 5 (catalog needs to persist), 11, 13
  - **Blocked By**: Task 2

  **References**:
  - **Type References**: `crates/agentdir/src/types.rs` — `Manifest`
  - **Pattern**: Metis directive — atomic write: write-tmp → fsync → rename
  - **External**: POSIX atomicity guarantee: `rename()` is atomic on same filesystem

  **Acceptance Criteria**:
  - [ ] `save` creates a valid JSON file
  - [ ] `load` parses the JSON back to identical `Manifest`
  - [ ] Roundtrip: `save` → `load` produces equal manifest (test with `assert_eq!`)
  - [ ] `load` rejects manifest with `version: 2` (returns `ManifestParse` error)
  - [ ] No `.json.tmp` file left after successful save
  - [ ] `.agentdir/` directory is created if missing

  **QA Scenarios**:
  ```
  Scenario: Atomic save and load roundtrip
    Tool: Bash (cargo test)
    Preconditions: None (uses tempfile)
    Steps:
      1. Run `cargo test -p agentdir manifest::tests::test_save_load_roundtrip`
    Expected Result: Manifest with entries saved and loaded back identically
    Failure Indicators: Field differences after deserialization
    Evidence: .sisyphus/evidence/task-8-roundtrip.txt

  Scenario: Reject unknown manifest version
    Tool: Bash (cargo test)
    Preconditions: None (uses tempfile)
    Steps:
      1. Run `cargo test -p agentdir manifest::tests::test_reject_unknown_version`
    Expected Result: Loading manifest with version:2 returns ManifestParse error
    Failure Indicators: No error, wrong error variant
    Evidence: .sisyphus/evidence/task-8-version-reject.txt
  ```

  **Commit**: YES
  - Message: `feat(manifest): implement atomic JSON persistence with version field`
  - Files: `crates/agentdir/src/manifest.rs`, `crates/agentdir/src/lib.rs`
  - Pre-commit: `cargo test -p agentdir`

- [x] 9. File Watcher (notify integration)

  **What to do**:
  - **RED**: Write tests for: watcher detects file creation, watcher detects file modification, watcher detects file deletion, watcher detects rename, watcher emits `RescanNeeded` when `need_rescan()` is true, watcher can be stopped via `WatchHandle` drop
  - **GREEN**: Implement `LocalBackend::watch` in `crates/agentdir/src/backend/local.rs` and add watcher module `crates/agentdir/src/watcher.rs`:
    - Complete the `watch` method on `LocalBackend`:
      - Create `notify_debouncer_full::new_debouncer` with configurable timeout (default 1 second)
      - Add all source roots with `RecursiveMode::Recursive`
      - Spawn a tokio task that reads debounced events and converts to `SourceEvent`:
        - `EventKind::Create` → `SourceEvent::Created`
        - `EventKind::Modify(ModifyKind::Data(_))` → `SourceEvent::Modified`
        - `EventKind::Remove` → `SourceEvent::Deleted`
        - `EventKind::Modify(ModifyKind::Name(RenameMode::Both))` → `SourceEvent::Renamed`
        - `event.need_rescan() == true` → `SourceEvent::RescanNeeded`
      - Return `WatchHandle` wrapping a `CancellationToken` from `tokio_util`
    - `WatchHandle` implementation:
      - `pub fn cancel(&self)` — signal cancellation
      - `impl Drop for WatchHandle` — calls cancel on drop
    - Add `FileWatcher` struct in `watcher.rs` as higher-level wrapper:
      - `FileWatcher::new(backend: Arc<dyn Backend>, roots: Vec<SourcePath>) -> Result<Self>`
      - `pub async fn start(&self) -> Result<(mpsc::Receiver<SourceEvent>, WatchHandle)>`
      - Adds periodic polling fallback: every N seconds (configurable, default 60s), emit `RescanNeeded` to trigger reconciliation
  - **REFACTOR**: Ensure watcher task is properly cancelled on handle drop. Use `tokio::select!` for clean shutdown.

  **Must NOT do**:
  - Do not handle events (just detect and forward) — handling is Task 10's job
  - Do not use raw `notify::Watcher` — use `notify-debouncer-full` for event deduplication
  - Do not use `std::thread::sleep` in tests — use `tokio::time::timeout`

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: Async event handling, platform-specific filesystem events, cancellation patterns
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Tasks 10, 11, 12)
  - **Parallel Group**: Wave 3
  - **Blocks**: Tasks 10, 11, 14
  - **Blocked By**: Tasks 5, 6

  **References**:
  - **Trait Reference**: `crates/agentdir/src/backend.rs` — `Backend::watch` method signature, `SourceEvent` enum, `WatchHandle` type
  - **Module Reference**: `crates/agentdir/src/backend/local.rs` — `LocalBackend` with `todo!()` watch
  - **External**: `notify-debouncer-full` crate: event debouncing with rename correlation
  - **External**: `tokio_util::sync::CancellationToken` for clean async shutdown
  - **Edge case**: Metis identified `need_rescan()` handling — must map to `RescanNeeded` event

  **Acceptance Criteria**:
  - [ ] File creation in watched dir produces `SourceEvent::Created`
  - [ ] File modification produces `SourceEvent::Modified`
  - [ ] File deletion produces `SourceEvent::Deleted`
  - [ ] `WatchHandle` drop stops the watcher (no resource leak)
  - [ ] Periodic polling emits `RescanNeeded` at configured interval
  - [ ] All watcher tests use `tokio::time::timeout` to prevent hanging

  **QA Scenarios**:
  ```
  Scenario: Detect file creation
    Tool: Bash (cargo test)
    Preconditions: None (uses tempfile)
    Steps:
      1. Run `cargo test -p agentdir watcher::tests::test_detect_file_creation -- --nocapture`
    Expected Result: Create watcher on temp dir, create file, receive Created event within 5s timeout
    Failure Indicators: Timeout, wrong event type
    Evidence: .sisyphus/evidence/task-9-detect-creation.txt

  Scenario: Watcher cleanup on drop
    Tool: Bash (cargo test)
    Preconditions: None
    Steps:
      1. Run `cargo test -p agentdir watcher::tests::test_watcher_cleanup`
    Expected Result: WatchHandle dropped, no lingering tasks or file descriptors
    Failure Indicators: Resource leak, panic on drop
    Evidence: .sisyphus/evidence/task-9-cleanup.txt
  ```

  **Commit**: YES
  - Message: `feat(watcher): integrate notify-debouncer-full for filesystem event detection`
  - Files: `crates/agentdir/src/watcher.rs`, `crates/agentdir/src/backend/local.rs`, `crates/agentdir/src/lib.rs`
  - Pre-commit: `cargo test -p agentdir`

- [x] 10. Change Reconciler (Diff + Propagate)

  **What to do**:
  - **RED**: Write tests for: reconcile detects new source files, reconcile detects deleted source files, reconcile detects modified source files (mtime+size changed), reconcile handles `RescanNeeded` by doing full diff, reconcile produces correct list of `ChangeAction`s
  - **GREEN**: Implement in `crates/agentdir/src/reconciler.rs`:
    - `ChangeAction` enum:
      - `Add { source: SourcePath, virtual_path: VirtualPath, metadata: SourceMetadata }`
      - `Remove { virtual_path: VirtualPath }`
      - `Refresh { virtual_path: VirtualPath, source: SourcePath, new_metadata: SourceMetadata }`
    - `Reconciler` struct:
      - `pub fn from_event(catalog: &Catalog, event: &SourceEvent) -> Result<Vec<ChangeAction>>`:
        - `Created`: Find which source root contains this path → compute virtual path → `Add`
        - `Modified`: Find catalog entry by source path → `Refresh`
        - `Deleted`: Find catalog entry by source path → `Remove`
        - `Renamed`: `Remove` old + `Add` new
        - `RescanNeeded`: delegate to `full_reconcile`
      - `pub fn full_reconcile(catalog: &Catalog, backend: &dyn Backend, roots: &[SourceRoot]) -> Result<Vec<ChangeAction>>`:
        - Scan all source roots via backend
        - Compare scanned entries vs catalog entries:
          - Present in scan but not catalog → `Add`
          - Present in catalog but not scan → `Remove`
          - Present in both but mtime or size differs → `Refresh`
        - Use mtime+size as fast-path comparison (NOT sha256)
      - `pub fn apply_actions(catalog: &mut Catalog, materializer: &Materializer, actions: &[ChangeAction]) -> Result<ReconcileSummary>`:
        - For `Add`: add to catalog + materialize
        - For `Remove`: remove from catalog + dematerialize
        - For `Refresh`: update catalog metadata + refresh materialization
        - Return `ReconcileSummary { added: usize, removed: usize, refreshed: usize, errors: Vec<...> }`
  - **REFACTOR**: Ensure source-to-virtual path translation is consistent with how `Catalog::map` works.

  **Must NOT do**:
  - Do not compute SHA-256 during reconciliation (use mtime+size only)
  - Do not interact with the watcher (reconciler receives events, doesn't create them)
  - Do not implement conflict resolution (one-way sync only)

  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: Core sync logic with multiple code paths, state transitions, and edge cases
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: NO — depends on Task 9 (SourceEvent types and watcher integration)
  - **Parallel Group**: Wave 3 (starts after Task 9 completes)
  - **Blocks**: Task 11
  - **Blocked By**: Tasks 5, 9

  **References**:
  - **Module References**: 
    - `crates/agentdir/src/catalog.rs` — `Catalog` struct and all CRUD methods
    - `crates/agentdir/src/materializer.rs` — `Materializer` for apply_actions
    - `crates/agentdir/src/backend.rs` — `SourceEvent` enum, `Backend::scan`
  - **Type References**: `crates/agentdir/src/types.rs` — `SourceRoot`, `SourceMetadata`
  - **Pattern**: Metis identified mtime+size fast-path, avoid eager hashing

  **Acceptance Criteria**:
  - [ ] `from_event(Created)` produces `Add` action with correct virtual path
  - [ ] `from_event(Deleted)` produces `Remove` action
  - [ ] `from_event(Modified)` produces `Refresh` action
  - [ ] `full_reconcile` detects files added, removed, and modified since last scan
  - [ ] `apply_actions` updates both catalog and materialized tree
  - [ ] No SHA-256 computation during reconciliation

  **QA Scenarios**:
  ```
  Scenario: Full reconcile detects new file
    Tool: Bash (cargo test)
    Preconditions: None (uses tempfile)
    Steps:
      1. Run `cargo test -p agentdir reconciler::tests::test_full_reconcile_new_file`
    Expected Result: New file in source dir produces Add action with correct virtual path
    Failure Indicators: Missing action, wrong virtual path
    Evidence: .sisyphus/evidence/task-10-reconcile-new.txt

  Scenario: Apply actions updates catalog and tree
    Tool: Bash (cargo test)
    Preconditions: None (uses tempfile)
    Steps:
      1. Run `cargo test -p agentdir reconciler::tests::test_apply_actions`
    Expected Result: After applying Add action, file exists in materialized tree and catalog
    Failure Indicators: File missing from tree or catalog
    Evidence: .sisyphus/evidence/task-10-apply-actions.txt
  ```

  **Commit**: YES
  - Message: `feat(sync): implement change reconciler with mtime+size diff and propagation`
  - Files: `crates/agentdir/src/reconciler.rs`, `crates/agentdir/src/lib.rs`
  - Pre-commit: `cargo test -p agentdir`

- [x] 11. Workspace — Catalog ↔ Materializer ↔ Watcher Integration

  **What to do**:
  - **RED**: Write integration tests for: create workspace from source dir (catalog + materialize all), verify materialized files readable, modify source file and verify refresh via reconciler, delete source file and verify removal, add new source file and verify addition, save and load workspace (persistence roundtrip)
  - **GREEN**: Implement in `crates/agentdir/src/workspace.rs`:
    - `Workspace` struct — the top-level API surface:
      - `catalog: Catalog`
      - `materializer: Materializer`
      - `backend: Arc<dyn Backend>`
      - `manifest_path: PathBuf`
    - `Workspace::init(workspace_root: PathBuf) -> Result<Self>`:
      - Create `.agentdir/` dir
      - Create empty catalog with `materialized_root = workspace_root`
      - Create materializer
      - Create `LocalBackend`
      - Save empty manifest
    - `Workspace::open(workspace_root: PathBuf) -> Result<Self>`:
      - Load manifest from `.agentdir/manifest.json`
      - Reconstruct catalog, materializer, backend
    - `Workspace::map(&mut self, source: SourcePath, mount: VirtualPath) -> Result<MapSummary>`:
      - Scan source via backend
      - Add entries to catalog
      - Materialize all new entries
      - Save manifest
    - `Workspace::unmap(&mut self, mount: &VirtualPath) -> Result<UnmapSummary>`:
      - Remove entries from catalog
      - Dematerialize all removed entries
      - Save manifest
    - Virtual operations (delegate to catalog + materializer):
      - `mkdir`, `rmdir`, `mv`, `cp`, `ln`, `rename` — update catalog, update materialization, save manifest
    - `Workspace::refresh(&mut self) -> Result<ReconcileSummary>`:
      - Run `full_reconcile` → `apply_actions` → save manifest
    - `Workspace::status(&self) -> WorkspaceStatus`:
      - Summary: `total_entries`, `source_roots`, `materialized_root`, `last_updated`
    - `Workspace::save(&self) -> Result<()>` — explicit manifest save
  - **REFACTOR**: Every mutation (map, unmap, mv, etc.) must save the manifest atomically after success. If materialization fails partially, the manifest should reflect only the successfully materialized entries.

  **Must NOT do**:
  - Do not implement watch loop here (Task 14 adds the watch loop)
  - Do not implement CLI argument parsing
  - Do not add any reorganization intelligence

  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: Integration of all core modules, partial failure handling, transactional consistency
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: NO — depends on all Wave 2 tasks
  - **Parallel Group**: Wave 3 (can start only after 5, 6, 7, 8, 9, 10 are done)
  - **Blocks**: Tasks 12, 13, 14, 15
  - **Blocked By**: Tasks 5, 6, 7, 8, 9, 10

  **References**:
  - **Module References** (ALL core modules):
    - `crates/agentdir/src/catalog.rs` — `Catalog` and all CRUD operations
    - `crates/agentdir/src/materializer.rs` — `Materializer` and all materialization methods
    - `crates/agentdir/src/backend/local.rs` — `LocalBackend` for scanning
    - `crates/agentdir/src/manifest.rs` — `save`, `load`, `manifest_path`
    - `crates/agentdir/src/reconciler.rs` — `full_reconcile`, `apply_actions`
  - **Pattern**: This is the facade pattern — `Workspace` is the single entry point for all operations

  **Acceptance Criteria**:
  - [ ] `Workspace::init` creates workspace dir with empty manifest
  - [ ] `Workspace::map` scans source, catalogs entries, materializes files, saves manifest
  - [ ] Materialized files are readable with correct content (verified via `fs::read`)
  - [ ] `Workspace::refresh` detects and applies source changes
  - [ ] `Workspace::open` loads persisted workspace correctly
  - [ ] `Workspace::mv` updates virtual path in catalog and moves materialized file
  - [ ] All operations save manifest atomically

  **QA Scenarios**:
  ```
  Scenario: Init, map, and verify materialized content
    Tool: Bash (cargo test)
    Preconditions: None (uses tempfile)
    Steps:
      1. Run `cargo test -p agentdir workspace::tests::test_init_map_verify`
    Expected Result: Init workspace, map source dir, read materialized file → content matches source
    Failure Indicators: Content mismatch, missing files
    Evidence: .sisyphus/evidence/task-11-init-map.txt

  Scenario: Persist and reload workspace
    Tool: Bash (cargo test)
    Preconditions: None (uses tempfile)
    Steps:
      1. Run `cargo test -p agentdir workspace::tests::test_persist_reload`
    Expected Result: Init + map + save, then open from same path → same entries and files
    Failure Indicators: Missing entries after reload
    Evidence: .sisyphus/evidence/task-11-persist-reload.txt

  Scenario: Refresh detects source modification
    Tool: Bash (cargo test)
    Preconditions: None (uses tempfile)
    Steps:
      1. Run `cargo test -p agentdir workspace::tests::test_refresh_source_modification`
    Expected Result: Modify source file, call refresh, materialized file has new content
    Failure Indicators: Old content persists
    Evidence: .sisyphus/evidence/task-11-refresh-modify.txt
  ```

  **Commit**: YES
  - Message: `feat(core): integrate catalog, materializer, watcher, and reconciler into Workspace`
  - Files: `crates/agentdir/src/workspace.rs`, `crates/agentdir/src/lib.rs`
  - Pre-commit: `cargo test -p agentdir`

- [x] 12. Bulk Operations + Batched Materialization

  **What to do**:
  - **RED**: Write tests for: materialize 100+ files in batch, batch handles partial failures (some files fail, rest succeed), progress reporting during batch, batch respects directory creation order (parents before children)
  - **GREEN**: Enhance `crates/agentdir/src/materializer.rs` and `crates/agentdir/src/workspace.rs`:
    - `Materializer::materialize_batch(&self, entries: &[CatalogEntry], progress: Option<&dyn ProgressReporter>) -> Result<BatchResult>`:
      - Sort entries: directories first (by depth, ascending), then files
      - Process in configurable chunk size (default 50)
      - Report progress after each chunk
      - Continue on individual file errors (collect into `errors` list)
      - Return `BatchResult { succeeded: usize, failed: usize, errors: Vec<(VirtualPath, AgentdirError)> }`
    - `Materializer::dematerialize_batch(&self, paths: &[VirtualPath]) -> Result<BatchResult>`:
      - Remove files first, then empty directories (reverse depth order)
    - `pub trait ProgressReporter: Send + Sync`:
      - `fn report(&self, completed: usize, total: usize, current: &VirtualPath)`
    - `LogProgressReporter` — default impl that logs via `tracing::info!`
    - Update `Workspace::map` and `Workspace::refresh` to use batch operations
  - **REFACTOR**: Ensure batch operations are atomic at the manifest level — manifest is only saved after the entire batch completes (or with partial results + error list).

  **Must NOT do**:
  - Do not parallelize file cloning (sequential is fine for v1 — reflink is nearly instant)
  - Do not add async streaming — sync batch iteration is sufficient

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: Performance optimization, partial failure handling, progress reporting
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: NO — depends on Task 11 (Workspace API that it enhances)
  - **Parallel Group**: Wave 3 (starts after Task 11 completes)
  - **Blocks**: Task 15
  - **Blocked By**: Tasks 7, 11

  **References**:
  - **Module Reference**: `crates/agentdir/src/materializer.rs` — existing `Materializer` struct
  - **Module Reference**: `crates/agentdir/src/workspace.rs` — `Workspace::map` and `Workspace::refresh`
  - **Edge case**: Metis identified rapid bulk changes (git checkout) needing batched materialization

  **Acceptance Criteria**:
  - [ ] Batch of 100 files materializes successfully
  - [ ] Partial failure: 98 succeed, 2 fail → BatchResult reflects both counts
  - [ ] Progress reporter receives callbacks with correct counts
  - [ ] Directories created before files (correct ordering)
  - [ ] Dematerialize batch removes files then empty dirs

  **QA Scenarios**:
  ```
  Scenario: Batch materialize 100 files
    Tool: Bash (cargo test)
    Preconditions: None (uses tempfile)
    Steps:
      1. Run `cargo test -p agentdir materializer::tests::test_batch_materialize_100`
    Expected Result: 100 files created at correct virtual paths, BatchResult shows 100 succeeded
    Failure Indicators: Missing files, incorrect count
    Evidence: .sisyphus/evidence/task-12-batch-100.txt

  Scenario: Batch handles partial failure
    Tool: Bash (cargo test)
    Preconditions: None (uses tempfile)
    Steps:
      1. Run `cargo test -p agentdir materializer::tests::test_batch_partial_failure`
    Expected Result: Some entries with invalid source paths fail, rest succeed, errors collected
    Failure Indicators: Entire batch aborts on first error
    Evidence: .sisyphus/evidence/task-12-partial-failure.txt
  ```

  **Commit**: YES
  - Message: `feat(core): implement batched materialization for bulk change scenarios`
  - Files: `crates/agentdir/src/materializer.rs`, `crates/agentdir/src/workspace.rs`
  - Pre-commit: `cargo test -p agentdir`

- [x] 13. CLI Binary — Core Commands

  **What to do**:
  - **RED**: Write integration tests using `assert_cmd` crate for: `agentdir init <path>`, `agentdir map <source> <mount>`, `agentdir unmap <mount>`, `agentdir status`, `agentdir refresh`, `agentdir mv <from> <to>`, `agentdir cp <from> <to>`, `agentdir ln <target> <link>`, `agentdir mkdir <path>`, `agentdir rmdir <path>`
  - **GREEN**: Implement in `crates/agentdir-cli/src/main.rs` + subcommand modules:
    - Use `clap` derive mode for CLI parsing:
      ```
      #[derive(Parser)]
      #[command(name = "agentdir", version, about = "Virtual filesystem for agent-optimized exploration")]
      struct Cli {
          #[command(subcommand)]
          command: Commands,
          /// Workspace root (default: current directory)
          #[arg(short, long, global = true)]
          workspace: Option<PathBuf>,
      }
      ```
    - `Commands` enum:
      - `Init { path: PathBuf }` — create workspace
      - `Map { source: PathBuf, mount: String }` — map source to virtual mount
      - `Unmap { mount: String }` — remove mapping
      - `Status` — show workspace status
      - `Refresh` — manual reconciliation
      - `Mv { from: String, to: String }` — move in virtual namespace
      - `Cp { from: String, to: String }` — copy in virtual namespace
      - `Ln { target: String, link: String }` — virtual symlink
      - `Mkdir { path: String }` — create virtual directory
      - `Rmdir { path: String }` — remove virtual directory
    - Each command:
      1. Resolve workspace root (from `--workspace` flag or cwd)
      2. Open workspace (or init for `Init`)
      3. Execute operation on `Workspace`
      4. Print human-readable result to stdout
      5. Exit with code 0 on success, 1 on error (print error to stderr)
    - Initialize `tracing_subscriber` with `EnvFilter` (respects `RUST_LOG`)
  - **REFACTOR**: Extract command handlers into separate functions for testability.
  - Add `assert_cmd` to dev-dependencies of `agentdir-cli`

  **Must NOT do**:
  - Do not implement `watch` command (Task 14)
  - Do not add colors/formatting libraries — plain text output for v1
  - Do not add interactive prompts — all operations are non-interactive

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: Multiple subcommands, integration with workspace API, CLI testing patterns
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Task 14)
  - **Parallel Group**: Wave 4
  - **Blocks**: Task 15
  - **Blocked By**: Tasks 5, 8, 11

  **References**:
  - **Module Reference**: `crates/agentdir/src/workspace.rs` — `Workspace` API (all public methods)
  - **External**: `clap` derive mode: https://docs.rs/clap/latest/clap/_derive/index.html
  - **External**: `assert_cmd` for CLI integration tests: https://docs.rs/assert_cmd
  - **External**: `tracing-subscriber` with `EnvFilter`: https://docs.rs/tracing-subscriber

  **Acceptance Criteria**:
  - [ ] `agentdir init /tmp/test-ws` creates workspace dir with `.agentdir/manifest.json`
  - [ ] `agentdir map /source/dir /docs` materializes source files under `/tmp/test-ws/docs/`
  - [ ] `agentdir status` prints entry count, source roots, materialized root
  - [ ] `agentdir refresh` detects and applies source changes
  - [ ] `agentdir mv /docs/old.txt /docs/new.txt` renames in virtual namespace
  - [ ] All commands exit 0 on success, 1 on error
  - [ ] `--version` prints version string
  - [ ] `--help` prints usage for each subcommand

  **QA Scenarios**:
  ```
  Scenario: Full CLI workflow (init → map → status → verify)
    Tool: Bash
    Preconditions: `cargo build -p agentdir-cli`
    Steps:
      1. Create temp source dir with 3 test files
      2. Run `agentdir init /tmp/agentdir-test-ws`
      3. Run `agentdir -w /tmp/agentdir-test-ws map /tmp/source /docs`
      4. Run `agentdir -w /tmp/agentdir-test-ws status`
      5. Verify `cat /tmp/agentdir-test-ws/docs/test1.txt` returns correct content
      6. Verify `find /tmp/agentdir-test-ws/docs/ -type f | wc -l` returns 3
    Expected Result: All commands exit 0, status shows 3 entries, files readable
    Failure Indicators: Non-zero exit, missing files, wrong content
    Evidence: .sisyphus/evidence/task-13-cli-workflow.txt

  Scenario: CLI error handling
    Tool: Bash
    Preconditions: `cargo build -p agentdir-cli`
    Steps:
      1. Run `agentdir -w /nonexistent status` (workspace doesn't exist)
      2. Capture exit code
    Expected Result: Exit code 1, error message to stderr mentioning "workspace not found" or similar
    Failure Indicators: Exit code 0, panic/backtrace instead of clean error
    Evidence: .sisyphus/evidence/task-13-cli-errors.txt
  ```

  **Commit**: YES
  - Message: `feat(cli): implement core CLI commands`
  - Files: `crates/agentdir-cli/src/main.rs`, `crates/agentdir-cli/src/commands/*.rs`, `crates/agentdir-cli/Cargo.toml`
  - Pre-commit: `cargo test --workspace`

- [x] 14. CLI Watch Command (Long-Running)

  **What to do**:
  - **RED**: Write tests for: watch command starts and detects file creation, watch command handles Ctrl+C gracefully (SIGINT), watch command triggers reconciliation on events
  - **GREEN**: Add `Watch` command to CLI:
    - `Commands::Watch { interval: Option<u64> }` — `interval` is polling interval in seconds (default 60)
    - Implementation:
      1. Open workspace
      2. Start watcher via `FileWatcher::start` on all source roots
      3. Enter event loop:
         - Receive `SourceEvent` from watcher channel
         - Batch events over a short window (100ms)
         - Run `Reconciler::from_event` for each event → collect `ChangeAction`s
         - Run `Reconciler::apply_actions` → update catalog + materialized tree
         - Save manifest
         - Log summary via `tracing::info!`
      4. Handle `SIGINT`/`SIGTERM` via `tokio::signal::ctrl_c()`:
         - Log "Shutting down..."
         - Drop `WatchHandle` (stops watcher)
         - Save manifest one final time
         - Exit cleanly with code 0
    - Print startup message: "Watching N source roots. Press Ctrl+C to stop."
    - Print each reconciliation: "Synced: +2 added, ~1 refreshed, -0 removed"
  - **REFACTOR**: Use `tokio::select!` for clean multiplexing of events and shutdown signal.

  **Must NOT do**:
  - Do not daemonize (no fork/setsid) — runs in foreground
  - Do not add systemd/launchd integration
  - Do not buffer events indefinitely — process in small batches

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: Async event loop, signal handling, graceful shutdown patterns
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Task 13)
  - **Parallel Group**: Wave 4
  - **Blocks**: Task 15
  - **Blocked By**: Tasks 9, 11, 13

  **References**:
  - **Module Reference**: `crates/agentdir/src/watcher.rs` — `FileWatcher::start`, `WatchHandle`
  - **Module Reference**: `crates/agentdir/src/reconciler.rs` — `from_event`, `apply_actions`
  - **Module Reference**: `crates/agentdir/src/workspace.rs` — `Workspace` API
  - **External**: `tokio::signal::ctrl_c()` for graceful shutdown
  - **External**: `tokio::select!` macro for multiplexing

  **Acceptance Criteria**:
  - [ ] `agentdir watch` starts and prints startup message
  - [ ] Creating a file in source dir triggers materialization in watched workspace
  - [ ] Modifying a source file triggers refresh of materialized file
  - [ ] Ctrl+C stops the watcher cleanly (exit 0, manifest saved)
  - [ ] Periodic polling triggers at configured interval

  **QA Scenarios**:
  ```
  Scenario: Watch detects live file creation
    Tool: Bash (interactive_bash / tmux)
    Preconditions: Workspace initialized with mapped source dir
    Steps:
      1. Start `agentdir -w /tmp/ws watch` in background
      2. Wait 2 seconds for watcher startup
      3. Create `/tmp/source/newfile.txt` with content "live test"
      4. Wait 5 seconds for detection + materialization
      5. Verify `cat /tmp/ws/docs/newfile.txt` returns "live test"
      6. Send SIGINT to agentdir process
      7. Verify process exits cleanly (exit 0)
    Expected Result: New file detected, materialized, readable. Clean shutdown.
    Failure Indicators: File not materialized, crash on shutdown, non-zero exit
    Evidence: .sisyphus/evidence/task-14-watch-live.txt

  Scenario: Watch graceful shutdown
    Tool: Bash (interactive_bash / tmux)
    Preconditions: Workspace initialized
    Steps:
      1. Start `agentdir -w /tmp/ws watch`
      2. Send SIGINT
      3. Verify exit code 0
      4. Verify manifest.json is valid JSON (not corrupted)
    Expected Result: Clean exit, valid manifest
    Failure Indicators: Non-zero exit, corrupted manifest, panic
    Evidence: .sisyphus/evidence/task-14-watch-shutdown.txt
  ```

  **Commit**: YES
  - Message: `feat(cli): implement watch command with graceful shutdown`
  - Files: `crates/agentdir-cli/src/main.rs` or `crates/agentdir-cli/src/commands/watch.rs`
  - Pre-commit: `cargo test --workspace`

- [x] 15. End-to-End Integration Tests

  **What to do**:
  - **RED + GREEN**: Write comprehensive integration tests in `crates/agentdir/tests/integration.rs`:
    - **Test 1: Full lifecycle** — init workspace → map source dir → verify materialized tree → modify source → refresh → verify update → unmap → verify cleanup
    - **Test 2: Multi-source mapping** — map two different source dirs to different mounts → verify both materialized → unmap one → verify other unchanged
    - **Test 3: Virtual operations** — map source → mv file → cp file → mkdir → ln → verify all operations reflected in materialized tree
    - **Test 4: Persistence roundtrip** — init → map → close workspace → reopen → verify entries and materialized files intact
    - **Test 5: Large tree** — create 500 files in source, map, verify all materialized, modify 10, refresh, verify exactly 10 refreshed
    - **Test 6: Source deletion propagation** — map source → delete source file → refresh → verify removed from materialized tree
    - **Test 7: New file auto-addition** — map source dir → add new file to source → refresh → verify appears in materialized tree
    - **Test 8: Ripgrep compatibility** — map source with text files → run `ripgrep` (if available) on materialized tree → verify it finds expected matches
    - **Test 9: Overlap rejection** — attempt to map source that overlaps with materialized root → verify error
    - **Test 10: Empty workspace operations** — init → status → verify empty → refresh → verify no errors
  - All tests use `tempfile::TempDir` for isolation
  - All tests are `#[tokio::test]`

  **Must NOT do**:
  - Do not test watcher (async event loop tests are in task 9/14)
  - Do not require specific filesystem type
  - Do not require root/sudo
  - Do not depend on external tools being installed (ripgrep test should be `#[ignore]` if `rg` not found)

  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: Comprehensive test suite covering all integration points and edge cases
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: NO — depends on all previous tasks
  - **Parallel Group**: Wave 4 (after Tasks 11, 12, 13, 14)
  - **Blocks**: F1-F4
  - **Blocked By**: Tasks 11, 12, 13, 14

  **References**:
  - **Module Reference**: `crates/agentdir/src/workspace.rs` — primary API under test
  - **Module Reference**: All modules in `crates/agentdir/src/` — tested indirectly through Workspace
  - **External**: `tempfile` for isolated test directories
  - **Pattern**: Integration tests in `tests/` dir (separate compilation unit in Rust)

  **Acceptance Criteria**:
  - [ ] All 10 integration tests pass
  - [ ] `cargo test --workspace` includes integration tests
  - [ ] No test depends on external tools (except marked `#[ignore]`)
  - [ ] No test leaves temporary files behind
  - [ ] Total test suite runs in < 30 seconds

  **QA Scenarios**:
  ```
  Scenario: Full integration test suite
    Tool: Bash
    Preconditions: All previous tasks completed
    Steps:
      1. Run `cargo test --workspace -- --nocapture`
      2. Capture output and exit code
    Expected Result: All tests pass (exit 0), including integration tests
    Failure Indicators: Any test failure, timeout, panic
    Evidence: .sisyphus/evidence/task-15-full-suite.txt

  Scenario: Ripgrep on materialized tree (if rg available)
    Tool: Bash
    Preconditions: Workspace with mapped text files materialized
    Steps:
      1. Check if `rg` is installed: `which rg`
      2. If installed: create workspace, map source dir with a file containing "FINDME"
      3. Run `rg FINDME /workspace/mount/`
      4. Verify match found
    Expected Result: ripgrep finds the string in the materialized file
    Failure Indicators: No matches, file not readable, permission error
    Evidence: .sisyphus/evidence/task-15-ripgrep.txt
  ```

  **Commit**: YES
  - Message: `test: add end-to-end integration tests for full agentdir workflow`
  - Files: `crates/agentdir/tests/integration.rs`
  - Pre-commit: `cargo test --workspace`

---

## Final Verification Wave (MANDATORY — after ALL implementation tasks)

> 4 review agents run in PARALLEL. ALL must APPROVE. Present consolidated results to user and get explicit "okay" before completing.

- [ ] F1. **Plan Compliance Audit** — `oracle`
  Read the plan end-to-end. For each "Must Have": verify implementation exists (read file, run command). For each "Must NOT Have": search codebase for forbidden patterns — reject with file:line if found. Check evidence files exist in .sisyphus/evidence/. Compare deliverables against plan.
  Output: `Must Have [N/N] | Must NOT Have [N/N] | Tasks [N/N] | VERDICT: APPROVE/REJECT`

- [ ] F2. **Code Quality Review** — `unspecified-high`
  Run `cargo clippy --workspace -- -D warnings` + `cargo test --workspace`. Review all files for: `unwrap()` in non-test code, `as any` equivalent patterns, empty error handling, `println!` in library code (should use `tracing`), unused imports. Check AI slop: excessive comments, over-abstraction, generic names.
  Output: `Build [PASS/FAIL] | Clippy [PASS/FAIL] | Tests [N pass/N fail] | Files [N clean/N issues] | VERDICT`

- [ ] F3. **Real Manual QA** — `unspecified-high`
  Start from clean state. Clone repo, `cargo build --workspace`. Create a temp source directory with varied files. Run full CLI workflow: `agentdir init`, map source dir, verify materialized tree with ripgrep/find/cat, modify source files, verify auto-refresh. Test edge cases: source deletion, new file creation, rapid modifications.
  Output: `Scenarios [N/N pass] | Integration [N/N] | Edge Cases [N tested] | VERDICT`

- [ ] F4. **Scope Fidelity Check** — `deep`
  For each task: read "What to do", read actual implementation. Verify 1:1 — everything in spec was built, nothing beyond spec was built. Check "Must NOT do" compliance. Flag any remote backend code, write-back logic, FUSE code, or reorganization intelligence.
  Output: `Tasks [N/N compliant] | Contamination [CLEAN/N issues] | Unaccounted [CLEAN/N files] | VERDICT`

---

## Commit Strategy

- **Task 1**: `chore: initialize cargo workspace with agentdir lib + cli crates`
- **Task 2**: `feat(core): define catalog entry types, manifest schema, and error types`
- **Task 3**: `feat(core): define async backend trait with LocalBackend placeholder`
- **Task 4**: `feat(core): implement reflink wrapper with remove-before-clone and copy fallback`
- **Task 5**: `feat(catalog): implement virtual tree CRUD operations (map, unmap, mkdir, mv, cp, ln, rename, rmdir)`
- **Task 6**: `feat(backend): implement LocalBackend with walkdir scanning and metadata`
- **Task 7**: `feat(materializer): implement persistent materialization engine with reflink cloning`
- **Task 8**: `feat(manifest): implement atomic JSON persistence with version field`
- **Task 9**: `feat(watcher): integrate notify-debouncer-full for filesystem event detection`
- **Task 10**: `feat(sync): implement change reconciler with mtime+size diff and propagation`
- **Task 11**: `feat(core): integrate catalog, materializer, watcher, and reconciler into Workspace`
- **Task 12**: `feat(core): implement batched materialization for bulk change scenarios`
- **Task 13**: `feat(cli): implement core CLI commands (init, map, unmap, mv, cp, ln, mkdir, rmdir, status, refresh)`
- **Task 14**: `feat(cli): implement watch command with graceful shutdown`
- **Task 15**: `test: add end-to-end integration tests for full agentdir workflow`

---

## Success Criteria

### Verification Commands
```bash
cargo test --workspace                           # Expected: all tests pass
cargo clippy --workspace -- -D warnings          # Expected: 0 warnings
cargo build --release --workspace                # Expected: successful build

# End-to-end smoke test:
mkdir -p /tmp/agentdir-test/source
echo "hello" > /tmp/agentdir-test/source/test.txt
agentdir init /tmp/agentdir-test/workspace
agentdir map /tmp/agentdir-test/source /docs
cat /tmp/agentdir-test/workspace/docs/test.txt   # Expected: "hello"
rg hello /tmp/agentdir-test/workspace/            # Expected: finds match
echo "modified" > /tmp/agentdir-test/source/test.txt
agentdir refresh
cat /tmp/agentdir-test/workspace/docs/test.txt   # Expected: "modified"
```

### Final Checklist
- [ ] All "Must Have" items present and verified
- [ ] All "Must NOT Have" items absent from codebase
- [ ] All tests pass (`cargo test --workspace`)
- [ ] Clippy clean (`cargo clippy --workspace -- -D warnings`)
- [ ] CLI workflow complete: init → map → verify → modify source → refresh → verify
- [ ] ripgrep works on materialized tree
- [ ] Source file deletion detected and propagated
- [ ] Source file modification detected and re-materialized
- [ ] New source file detected and auto-added
