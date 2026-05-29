# agentdir

Virtual filesystem for agent-optimized file exploration using CoW reflinks.

`agentdir` is a Python binding for the [agentdir](https://github.com/NomaDamas/agentdir) Rust library. It lets you map real directories into a virtual file tree, move and copy entries without touching source files, track source changes, and fork the tree into isolated snapshots via copy-on-write.

- **Version:** 0.1.2
- **License:** MIT
- **Python:** >= 3.9
- **Built with:** PyO3 + maturin (native Rust extension, abi3 wheels)

---

## Installation

```sh
pip install agentdir
```

No extra dependencies. The package ships pre-built abi3 wheels for Linux, macOS, and Windows.

---

## Quick Start

```python
from agentdir import Workspace

ws = Workspace.init("./workspace")
summary = ws.map("./my-docs", "/docs")
print(f"Mapped {summary['entries_added']} entries")

content = ws.read_bytes("/docs/readme.md")
print(content.decode())

ws.mv("/docs/readme.md", "/readme.md")  # source files are untouched
```

All methods are **synchronous**. There is no async API. Internally the library runs a Tokio runtime, but the Python surface is fully blocking.

---

## API Reference

### Import

```python
from agentdir import Workspace, SnapshotWorkspace
```

---

### `Workspace`

#### Static methods

##### `Workspace.init(path: str, strategy: str = "reflink") -> Workspace`

Initialize a new workspace at `path`. The `strategy` controls how files are materialized:

| Value | Behavior |
|---|---|
| `"reflink"` | CoW reflink, falls back to byte-copy if unsupported (default) |
| `"symlink"` | Symbolic links |
| `"hardlink"` | Hard links |
| `"virtual"` | Metadata-only, no materialization |

##### `Workspace.open(path: str) -> Workspace`

Open an existing workspace. Raises `FileNotFoundError` if the workspace does not exist at `path`.

---

#### Instance methods

##### `map(source: str, mount: str) -> dict[str, int]`

Map a source directory into the virtual tree at `mount`. Returns a summary dict:

```python
{
    "entries_added": int,
    "reflinked":     int,
    "copied":        int,
    "symlinked":     int,
    "hardlinked":    int,
    "dirs_created":  int,
    "errors":        int,
}
```

##### `unmap(mount: str) -> dict[str, int]`

Remove the mapping at `mount` and clean up its entries. Returns:

```python
{"entries_removed": int}
```

##### `mv(from_path: str, to_path: str) -> None`

Move a virtual entry. The source file on disk is not touched.

##### `cp(from_path: str, to_path: str) -> None`

Copy a virtual entry. The source file on disk is not touched.

##### `mkdir(path: str) -> None`

Create a virtual directory.

##### `rmdir(path: str, recursive: bool) -> None`

Remove a virtual directory. Pass `recursive=True` to remove non-empty directories.

##### `rename(path: str, new_name: str) -> None`

Rename the last path component of a virtual entry. `new_name` is a bare name, not a full path.

##### `exists(path: str) -> bool`

Return `True` if the virtual path exists.

##### `stat(path: str) -> dict[str, object]`

Return metadata for a virtual path:

```python
{
    "virtual_path":  str,
    "source_path":   str,
    "size_bytes":    int,
    "mtime_ns":      int,
    "entry_type":    str,   # "File" or "Directory"
    "materialized":  bool,
}
```

##### `read_bytes(path: str) -> bytes`

Read the raw bytes of a file at the given virtual path.

##### `refresh() -> dict[str, int]`

Detect changes in source directories and apply them to the virtual tree. Returns:

```python
{
    "added":     int,
    "refreshed": int,
    "removed":   int,
    "errors":    int,
}
```

##### `refresh_with_hash_verification(verify_hashes: bool = False) -> dict[str, int]`

Same as `refresh()`, with an optional SHA-256 pass. When `verify_hashes=True`, files whose mtime and size are unchanged are additionally verified by content hash to catch silent modifications. Returns the same shape as `refresh()`.

##### `status() -> dict[str, object]`

Return workspace-level metadata:

```python
{
    "total_entries":           int,
    "source_roots":            int,
    "materialized_root":       str,
    "last_updated_epoch_secs": int,
}
```

##### `export_mapping(reverse: bool = False, relative_to: str | None = None) -> dict[str, str]`

Export the source-to-virtual path mapping as a plain dict. Pass `reverse=True` to get virtual-to-source instead. Pass `relative_to` to relativize source paths against a base directory.

##### `map_batch(mappings: list[tuple[str, str]]) -> dict[str, object]`

Map multiple files in one call. Each tuple is `(source_path, mount_point)`. Note: batch map accepts **files only**, not directories. Returns:

```python
{
    "entries_added": int,
    "reflinked":     int,
    "copied":        int,
    "symlinked":     int,
    "hardlinked":    int,
    "dirs_created":  int,
}
```

##### `rglob(pattern: str) -> list[str]`

Match virtual paths against a glob pattern. Supports `*` and `**` wildcards (e.g. `"/docs/*.txt"`, `"/src/**/*.py"`). Returns a list of matching virtual paths.

##### `list_snapshots() -> list[str]`

Return the names of all snapshots attached to this workspace.

##### `snapshot(name: str) -> SnapshotWorkspace`

Create a named CoW snapshot of the current virtual tree. The snapshot starts as a fork of the workspace and accepts isolated writes.

##### `open_snapshot(name: str) -> SnapshotWorkspace`

Open an existing named snapshot.

##### `destroy_snapshot(name: str) -> None`

Destroy a named snapshot and remove its files from disk.

---

### `SnapshotWorkspace`

A CoW fork of a `Workspace`. Writes to a snapshot are isolated and do not affect the base workspace or any source files.

##### `exists(path: str) -> bool`

Return `True` if the virtual path exists in this snapshot.

##### `stat(path: str) -> dict[str, object]`

Return metadata for a virtual path. Same shape as `Workspace.stat()`.

##### `read_bytes(path: str) -> bytes`

Read the raw bytes of a file in this snapshot.

##### `write(path: str, content: bytes) -> None`

Write `content` to a file in this snapshot. The write is copy-on-write and does not affect the base workspace.

##### `export_mapping(reverse: bool = False, relative_to: str | None = None) -> dict[str, str]`

Export the path mapping for this snapshot. Same semantics as `Workspace.export_mapping()`.

##### `destroy() -> None`

Destroy this snapshot and remove all its files from disk.

---

## Examples

### Map a directory and read files

```python
from agentdir import Workspace

ws = Workspace.init("./workspace")
summary = ws.map("./my-docs", "/docs")
print(f"Mapped {summary['entries_added']} entries")

content = ws.read_bytes("/docs/readme.md")
print(content.decode())

ws.mv("/docs/readme.md", "/readme.md")  # source files untouched
```

### Snapshots with isolated writes

```python
from agentdir import Workspace

ws = Workspace.init("./workspace")
ws.map("./project", "/src")

snap = ws.snapshot("experiment")
snap.write("/src/config.json", b'{"experimental": true}')

# Base workspace is unaffected:
original = ws.read_bytes("/src/config.json")
modified = snap.read_bytes("/src/config.json")

snap.destroy()
```

---

## License

MIT. See [LICENSE](https://github.com/NomaDamas/agentdir/blob/master/LICENSE) for details.

For the CLI and Rust library, see the main repository: [https://github.com/NomaDamas/agentdir](https://github.com/NomaDamas/agentdir)
