# @nomadamas/agentdir

Virtual filesystem for agent-optimized exploration of general-purpose files using CoW reflinks.

Built with [NAPI-RS](https://napi.rs/). Prebuilt native binaries are bundled per platform via `optionalDependencies`, so no compiler is required.

- **GitHub:** https://github.com/NomaDamas/agentdir
- **License:** MIT
- **Node.js:** >= 18

---

## Installation

```sh
npm install @nomadamas/agentdir
```

Installation only installs the native package. After mapping original files, call `workspace.refresh()` whenever the original file tree may have changed. The Node binding exposes reconciliation through `refresh()` and `refreshWithHashVerification()`; it does not start the CLI watch loop for you.

## Quick Start

```js
import { Workspace } from '@nomadamas/agentdir'

// init and open are synchronous
const ws = Workspace.init('./workspace')

// everything else is async
const summary = await ws.map('./team-files', '/files')
console.log(`Mapped ${summary.entriesAdded} entries`)

const bytes = await ws.readBytes('/files/q1-report.txt')
console.log(bytes.toString())

await ws.mv('/files/q1-report.txt', '/reports/q1-report.txt')  // original files are untouched

// Reconcile source changes before an agent/session depends on the view.
const sync = await ws.refresh()
console.log(sync)
```

Both CommonJS and ESM are supported:

```js
const { Workspace } = require('@nomadamas/agentdir')
// or
import { Workspace, SnapshotWorkspace } from '@nomadamas/agentdir'
```

If you also install the CLI, long-running workflows can run `agentdir -w ./workspace watch --interval 60` in a separate process. The watcher combines filesystem events with periodic full rescans; Node applications should otherwise call `refresh()` on their own schedule or at session boundaries.

---

## API Reference

### `Workspace`

#### Static methods (synchronous)

These two methods are **synchronous** and return a `Workspace` directly, not a `Promise`.

```ts
static init(path: string, strategy?: string): Workspace
```

Initialize a new workspace at `path`. The optional `strategy` controls how files are materialized:

| Value | Behavior |
|---|---|
| `"reflink"` | Copy-on-write clone (default) |
| `"symlink"` | Symbolic link |
| `"virtual"` | No materialization |

```ts
static open(path: string): Workspace
```

Open an existing workspace at `path`.

---

#### Instance methods (all async)

##### Mapping

```ts
map(source: string, mount: string): Promise<MapSummary>
```

Map a source directory to a virtual mount point (e.g. `"/files"`).

```ts
unmap(mount: string): Promise<UnmapSummary>
```

Remove the mapping at the given mount point.

```ts
mapBatch(mappings: Array<Array<string>>): Promise<BatchMapSummary>
```

Map multiple sources in one call. Each element is a `[sourcePath, mountPoint]` tuple. Note: batch map accepts files only, not directories.

---

##### Navigation and structure

```ts
mv(from: string, to: string): Promise<void>
```

Move a virtual entry.

```ts
cp(from: string, to: string): Promise<void>
```

Copy a virtual entry.

```ts
mkdir(path: string): Promise<void>
```

Create a virtual directory.

```ts
rmdir(path: string, recursive: boolean): Promise<void>
```

Remove a virtual directory.

```ts
rename(path: string, newName: string): Promise<void>
```

Rename a virtual entry (last path component only).

---

##### Querying

```ts
exists(path: string): Promise<boolean>
```

Check whether a virtual path exists.

```ts
stat(path: string): Promise<StatResult>
```

Get metadata for a virtual path.

```ts
readBytes(path: string): Promise<Buffer>
```

Read the raw bytes of a file at the given virtual path.

```ts
rglob(pattern: string): Promise<Array<string>>
```

Match virtual paths against a glob pattern (e.g. `"/files/*.pdf"`, `"/media/**/*.png"`). Returns an array of matching virtual paths.

```ts
exportMapping(reverse?: boolean, relativeTo?: string): Promise<Record<string, string>>
```

Export the source-to-virtual path mapping as a plain object. Pass `reverse: true` to get virtual-to-source instead. `relativeTo` sets a base path for relativizing source paths.

---

##### Sync and status

```ts
refresh(): Promise<RefreshSummary>
```

Detect and apply changes from source directories.

```ts
refreshWithHashVerification(verifyHashes: boolean): Promise<RefreshSummary>
```

Refresh with optional SHA-256 verification. When `verifyHashes` is `true`, files whose mtime and size are unchanged are additionally verified via SHA-256 to catch silent modifications.

```ts
status(): Promise<StatusResult>
```

Get a summary of the current workspace state.

---

##### Snapshots

```ts
snapshot(name: string): Promise<SnapshotWorkspace>
```

Create a named CoW snapshot of the current workspace.

```ts
openSnapshot(name: string): Promise<SnapshotWorkspace>
```

Open an existing named snapshot.

```ts
listSnapshots(): Promise<Array<string>>
```

List all snapshot names.

```ts
destroySnapshot(name: string): Promise<void>
```

Destroy a named snapshot.

---

### `SnapshotWorkspace`

A snapshot is a CoW fork of a `Workspace`. Writes to a snapshot are isolated and do not affect the base workspace.

All methods are async.

```ts
exists(path: string): Promise<boolean>
stat(path: string): Promise<StatResult>
readBytes(path: string): Promise<Buffer>
write(path: string, content: Buffer): Promise<void>
exportMapping(reverse?: boolean, relativeTo?: string): Promise<Record<string, string>>
destroy(): Promise<void>
```

`write` materializes a copy-on-write file in the snapshot. The base workspace is unaffected.

`destroy` removes all snapshot files from disk.

---

### Result types

```ts
interface MapSummary {
  entriesAdded: number
  reflinked: number
  copied: number
  symlinked: number
  dirsCreated: number
  errors: number
}

interface BatchMapSummary {
  entriesAdded: number
  reflinked: number
  copied: number
  symlinked: number
  dirsCreated: number
  errors: Array<Array<string>>
}

interface UnmapSummary {
  entriesRemoved: number
}

interface RefreshSummary {
  added: number
  refreshed: number
  removed: number
  errors: number
}

interface StatResult {
  virtualPath: string
  sourcePath: string
  sizeBytes: number
  mtimeNs: number
  entryType: string
  materialized: boolean
}

interface StatusResult {
  totalEntries: number
  sourceRoots: number
  materializedRoot: string
  lastUpdatedEpochSecs: number
}
```

---

## Examples

### Map a directory and read files

```ts
import { Workspace } from '@nomadamas/agentdir'

const ws = Workspace.init('./workspace')           // sync
const summary = await ws.map('./team-files', '/files') // async
console.log(`Mapped ${summary.entriesAdded} entries`)

const bytes = await ws.readBytes('/files/q1-report.txt')
console.log(bytes.toString())

await ws.mv('/files/q1-report.txt', '/reports/q1-report.txt')       // original files untouched
```

### Snapshots with isolated writes

```ts
import { Workspace } from '@nomadamas/agentdir'

const ws = Workspace.init('./workspace')
await ws.map('./team-files', '/files')

const snap = await ws.snapshot('experiment')
await snap.write('/files/q1-report.txt', Buffer.from('snapshot-only draft'))

// The base workspace is unaffected:
const original = await ws.readBytes('/files/q1-report.txt')
const modified  = await snap.readBytes('/files/q1-report.txt')

console.log(original.toString()) // original content
console.log(modified.toString()) // snapshot-only draft

await snap.destroy()
```

### Glob and export mapping

```ts
import { Workspace } from '@nomadamas/agentdir'

const ws = Workspace.open('./workspace')

const pdfFiles = await ws.rglob('/files/**/*.pdf')
console.log(pdfFiles) // ['/files/reports/q1.pdf', '/files/contracts/vendor.pdf', ...]

const mapping = await ws.exportMapping()
// { '/files/reports/q1.pdf': '/absolute/path/to/team-files/reports/q1.pdf', ... }
```

---

## Supported Platforms

Prebuilt binaries are provided for:

| Platform | Architecture |
|---|---|
| macOS | x86_64 (Intel) |
| macOS | aarch64 (Apple Silicon) |
| Windows | x86_64 (MSVC) |
| Linux | x86_64 (GNU) |
| Linux | x86_64 (musl / Alpine) |

On other platforms, you'll need a Rust toolchain to build from source.

---

## Related

This is the official Node.js binding for the `agentdir` project. The main repository also includes a CLI and a Rust library:

https://github.com/NomaDamas/agentdir

---

## License

MIT
