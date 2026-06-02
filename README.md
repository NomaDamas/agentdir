# agentdir

**Rust-built virtual file tree infrastructure for agent-ready file layouts across macOS, Linux, and Windows**

[![crates.io](https://img.shields.io/crates/v/agentdir)](https://crates.io/crates/agentdir)
[![PyPI](https://img.shields.io/pypi/v/agentdir)](https://pypi.org/project/agentdir/)
[![npm](https://img.shields.io/npm/v/@nomadamas/agentdir)](https://www.npmjs.com/package/@nomadamas/agentdir)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

agentdir lets your tools present the same original files in purpose-built, read-only folder structures without moving the originals. AI agents, scripts, and humans can navigate documents, media, datasets, generated artifacts, plain text, binaries, or any other OS-visible files through a layout optimized for the task at hand.

Built in Rust, agentdir runs on macOS, Linux, and Windows. On CoW-capable filesystems such as APFS, Btrfs, and XFS, alternate layouts and snapshots use reflinks, so large files do not get duplicated just because the folder structure changes. When CoW is unavailable, agentdir falls back to byte-copy materialization.

The point is simple: keep the human-facing file layout stable, give agents a better working layout, and keep the two mapped together as original files change.

---

## Why agentdir

- **Better agent context** — expose task-specific file layouts instead of forcing agents through whatever folder structure happened to grow over time
- **Originals stay put** — rearrange the virtual namespace with `mv`, `cp`, `rename`, `mkdir`, and `rmdir` without moving the source files
- **No duplicate-data tax on CoW filesystems** — big PDFs, images, media, and datasets can appear in multiple layouts without copying file data on supported filesystems
- **General-purpose files, not just developer projects** — works with documents, spreadsheets, presentations, PDFs, images, media, datasets, plain text, binaries, and more
- **Cross-platform native core** — Rust library, CLI, Python bindings, and Node.js bindings

---

## Features

- **Virtual namespace** — map source directories into a virtual tree at arbitrary mount points, then move, copy, and rename entries without touching the originals
- **CoW materialization** — files are cloned via reflinks on APFS (macOS) and Btrfs/XFS (Linux); falls back to byte-copy on NTFS (Windows)
- **Accurate change tracking** — detects additions, modifications, and deletions in source directories via metadata (mtime + size) and propagates them to the virtual tree automatically
- **Multiple materialization strategies** — `reflink` (default), `symlink`, `virtual`
- **Snapshot support** — CoW forks of the workspace for isolated concurrent workspaces
- **File-format-agnostic** — works with any file the OS can stat: documents, spreadsheets, presentations, PDFs, images, media, datasets, plain text, binaries, and more
- **Cross-platform** — macOS, Linux, Windows; virtual paths always use `/` internally regardless of host OS
- **Three distribution channels** — Rust library, Python bindings (PyO3), Node.js bindings (NAPI-RS)

---

## Installation

### Rust

Add the library to your Rust application:

```sh
cargo add agentdir
```

Install the CLI:

```sh
cargo install agentdir-cli
```

This installs a binary named `agentdir`.

### Python

Requires Python >= 3.9.

```sh
pip install agentdir
```

### Node.js

Requires Node >= 18. The package is scoped under `@nomadamas`:

```sh
npm install @nomadamas/agentdir
```

Prebuilt binaries are available for:

- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`
- `x86_64-unknown-linux-gnu`
- `x86_64-unknown-linux-musl`

---

## Keep the View Synced

Installation only installs the library or CLI. After you map original files, you should also choose how the workspace will stay current. This is not a nice-to-have: the virtual tree is a live navigation view, and original-file changes are propagated only when you run reconciliation.

For CLI-driven work, run the watcher next to the agent or script that consumes the workspace:

```sh
agentdir -w ./workspace watch --interval 60
```

`watch` reacts to filesystem events for fast updates and also performs periodic full rescans so missed OS events are recovered. It runs in the foreground; put it under your process manager, terminal multiplexer, service supervisor, or task runner if you need it to stay alive.

If you do not want a long-running watcher, call `refresh` before each agent session, before exporting mappings, or on your own schedule:

```sh
agentdir -w ./workspace refresh
```

Library users should do the same through their runtime surface: call `Workspace.refresh()` whenever the source may have changed, or use `refresh_with_hash_verification(true)` when you want an additional SHA-256 verification pass for unchanged mtime/size metadata.

---

## CLI Usage

The binary is named `agentdir`. Most commands accept a `-w`/`--workspace <dir>` flag to specify the workspace directory; if omitted, the current directory is used.

### Quick start

```sh
# Initialize a new workspace
agentdir init ./workspace

# Map a source directory into the virtual tree
agentdir -w ./workspace map ./team-files /files

# Keep the virtual tree current while agents consume it
agentdir -w ./workspace watch --interval 60

# Check workspace status
agentdir -w ./workspace status

# Move an entry in the virtual namespace (original files are untouched)
agentdir -w ./workspace mv /files/q1-report.pdf /reports/q1-report.pdf
```

### Command reference

| Command | Description |
|---------|-------------|
| `init <path> [--strategy reflink\|symlink\|virtual]` | Initialize a new workspace |
| `map <source> <mount>` | Map a source directory into the virtual tree |
| `map-batch --from-json <file>` | Apply a batch mapping from a JSON file `{"source_path":"virtual_path",...}` |
| `unmap <mount>` | Remove a source mapping |
| `status` | Show workspace status |
| `stat <path>` | Show metadata for a virtual path |
| `cat <path>` | Print file contents via virtual path |
| `refresh` | Detect and apply source changes |
| `mv <from> <to>` | Move an entry in the virtual namespace |
| `cp <from> <to>` | Copy an entry in the virtual namespace |
| `mkdir <path>` | Create a virtual directory |
| `rmdir <path> [-r/--recursive]` | Remove a virtual directory |
| `export-mapping [--format json] [--reverse] [--relative-to <dir>]` | Export source/virtual mapping as JSON |
| `watch [-i/--interval <secs>]` | Watch for source changes and auto-sync (foreground, default interval 60s) |

---

## Library Usage

For full API documentation, see the binding-specific READMEs:

- **Python** — [`bindings/python/README.md`](bindings/python/README.md)
- **Node.js** — [`bindings/node/README.md`](bindings/node/README.md)

The Rust library is documented on [docs.rs](https://docs.rs/agentdir).

---

## How It Works

When you `map` a source directory, agentdir records the mapping in an atomic JSON manifest (written via write-tmp + fsync + rename, so no partial writes). On `refresh` or via the background watcher, it scans source metadata and computes a diff against the last known state. Changed entries are materialized into the workspace directory as CoW clones (or byte-copies where CoW isn't available). The virtual namespace is an in-memory catalog with O(1) lookup; virtual paths always use `/` as the separator on all platforms.

Snapshots are CoW forks of the workspace directory, giving you isolated copies for concurrent workloads without duplicating data on supporting filesystems.

Source symlinks are detected but not followed during scanning.

---

## Non-Goals

agentdir is intentionally narrow in scope. The following are not goals of this project:

- AI/LLM integration, semantic understanding, or intelligent file routing
- File content parsing, full-text indexing, or search
- The orchestrator or agent that decides *what* to restructure or *why*
- File format conversion or transformation
- Dependency graph analysis, AST parsing, or language-aware features
- Access control, permissions, or multi-tenancy

---

## Repository Structure

```
crates/
  agentdir/         Core Rust library
  agentdir-cli/     CLI binary
bindings/
  python/           Python bindings (PyO3 + maturin)
  node/             Node.js bindings (NAPI-RS)
```

---

## License

MIT. See [LICENSE](LICENSE).
