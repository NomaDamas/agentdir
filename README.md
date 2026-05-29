# agentdir

**Virtual filesystem for agent-optimized file exploration using CoW reflinks**

[![crates.io](https://img.shields.io/crates/v/agentdir)](https://crates.io/crates/agentdir)
[![PyPI](https://img.shields.io/pypi/v/agentdir)](https://pypi.org/project/agentdir/)
[![npm](https://img.shields.io/npm/v/@nomadamas/agentdir)](https://www.npmjs.com/package/@nomadamas/agentdir)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

agentdir is infrastructure-level plumbing. It gives you a virtual file tree that maps to real source files via Copy-on-Write (CoW) reflinks, letting any consumer, whether an AI agent, a script, or a human, reorganize that tree independently of the real source layout. It does not parse file contents, make routing decisions, or integrate with any LLM. It tracks changes and materializes copies. That's it.

---

## Features

- **Virtual namespace** — map source directories into a virtual tree at arbitrary mount points, then move, copy, and rename entries without touching the originals
- **CoW materialization** — files are cloned via reflinks on APFS (macOS) and Btrfs/XFS (Linux); falls back to byte-copy on NTFS (Windows)
- **Accurate change tracking** — detects additions, modifications, and deletions in source directories via metadata (mtime + size) and propagates them to the virtual tree automatically
- **Multiple materialization strategies** — `reflink` (default), `symlink`, `hardlink`, `virtual`
- **Snapshot support** — CoW forks of the workspace for isolated concurrent workspaces
- **File-format-agnostic** — works with any file the OS can stat: docx, pptx, images, binaries, source code
- **Cross-platform** — macOS, Linux, Windows; virtual paths always use `/` internally regardless of host OS
- **Three distribution channels** — Rust library, Python bindings (PyO3), Node.js bindings (NAPI-RS)

---

## Installation

### Rust

Add the library to your project:

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

## CLI Usage

The binary is named `agentdir`. Most commands accept a `-w`/`--workspace <dir>` flag to specify the workspace directory; if omitted, the current directory is used.

### Quick start

```sh
# Initialize a new workspace
agentdir init ./workspace

# Map a source directory into the virtual tree
agentdir -w ./workspace map ./my-docs /docs

# Check workspace status
agentdir -w ./workspace status

# Move an entry in the virtual namespace (source files are untouched)
agentdir -w ./workspace mv /docs/readme.md /readme.md
```

### Command reference

| Command | Description |
|---------|-------------|
| `init <path> [--strategy reflink\|symlink\|hardlink\|virtual]` | Initialize a new workspace |
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
