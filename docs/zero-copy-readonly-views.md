# Zero-Copy Read-Only Views on Non-CoW Filesystems

> Feasibility study and decision for GitHub issue
> [#19](https://github.com/NomaDamas/agentdir/issues/19).
> This is the authoritative writeup; the table in `AGENTS.md`
> ("Future / Out of Current Scope") is a pointer to this document.

## Problem & constraints

agentdir materializes a restructured, **read-only** virtual file tree over an
unmodified source tree. On a copy-on-write (CoW) filesystem the materializer
uses reflinks, which are effectively zero-copy. On a non-CoW filesystem
(ext4, NTFS, exFAT, HFS+, …) the materializer falls back to a **byte-copy**,
which duplicates disk usage. For large files, repeated virtual layouts, or many
snapshots, that duplication can become significant.

Any alternative to the byte-copy fallback MUST preserve agentdir's contract:

- The virtual tree is **read-only**. Writes through the virtual tree must not be
  the sanctioned edit path.
- The **original source stays writable**. The source is the sole edit target;
  consumers resolve it via `stat()` / `export_mapping()`.
- Consumers see entries as **normal, usable files** wherever possible.
- **Symlink-skipping tools must not see an empty workspace.** Tools that scan
  with `follow_links: false` (or refuse to descend into symlinks) must still see
  real directory entries with real content.
- **No file content parsing, indexing, or transformation** is introduced.

### Non-goals (out of scope for any approach here)

- Reintroducing **hardlink** materialization. A hardlink shares the source
  inode, so making the materialized entry read-only also makes the source
  read-only, and writes through the materialized path mutate the source. This is
  exactly the corruption hazard agentdir removed; it is not revisited.
- Making **symlink** the default strategy. Symlink mode is a documented
  passthrough/unsafe exception, not a read-only guarantee.
- File-format-specific parsing or transformation.
- Any orchestrator/agent that decides *what* to restructure. agentdir is
  plumbing; the restructuring strategy lives elsewhere.

## Per-platform approaches

The candidate mechanisms, their privileges, deployment requirements, and whether
they need a long-lived daemon:

| Platform | Mechanism | Privileges required | Deployment requirements | Long-lived daemon? |
|----------|-----------|---------------------|-------------------------|--------------------|
| Linux | Read-only **bind mount** (`mount --bind` then `mount -o remount,ro,bind`) | `root` or `CAP_SYS_ADMIN`; can be confined to a private/unprivileged user+mount namespace (`unshare -Urm`) | Standard Linux mounts; one mount entry per bound path; mounts must be torn down on exit | No (kernel-managed mount) |
| Linux | **FUSE passthrough** (libfuse, e.g. a `passthrough_hp`-style filesystem) | Usually unprivileged with the `fuse` kernel module + `/dev/fuse`; `user_allow_other` needed only for cross-user access | `fuse` kernel module present; libfuse available; `fusermount3` for unmount | Yes (the FUSE server process must stay up while the view is mounted) |
| Windows | **ProjFS** (Projected File System / `PrjFlt`) | Standard user once the optional feature is enabled; enabling the feature needs admin | "Windows Projected File System" optional feature enabled (built into Windows 10 1809+/Server 2019+) | Yes (a provider process answers projection callbacks) |
| Windows | **WinFsp** (user-mode FS framework, FUSE-like) | Standard user once installed; installing the driver needs admin | WinFsp driver/package installed (third-party dependency, not in-box) | Yes (the user-mode FS host process must stay up) |
| macOS | **APFS reflink** (`clonefile(2)`) — *already implemented* | None | APFS volume (default on modern macOS) | No |

Notes:

- **CoW reflink is filesystem-level, not macOS-only.** `clonefile`/`FICLONE`
  reflinks work on APFS (macOS) **and** on Btrfs and XFS (Linux). The "non-CoW
  fallback" problem is therefore about the *filesystem*, not the *OS*: a Linux
  user on Btrfs/XFS already gets zero-copy via the existing reflink path; only
  ext4/NTFS/exFAT/HFS+ and similar fall back to byte-copy.
- macOS needs no new mechanism: APFS reflink already satisfies the zero-copy
  read-only contract. Only non-APFS volumes (rare) hit the byte-copy fallback,
  and that is documented as a limitation, not engineered around.

## Normal-file compatibility without copying

Per-mechanism verdict — does it present entries as **normal files** to
symlink-skipping tools **and** avoid copying file data?

- **Linux read-only bind mount** — **Normal files: yes. Zero-copy: yes. But it
  cannot restructure.** A bind mount re-exposes an existing directory subtree at
  a new path; the entries are the same real files, read-only at the new mount.
  Critically, **a plain bind mount mirrors the source layout exactly** — it
  cannot present agentdir's *arbitrary restructured* virtual tree (files pulled
  from many source locations into a new hierarchy) without layering an overlay
  or FUSE filesystem on top. So bind mount alone only solves the degenerate case
  where the virtual layout equals the source layout.
- **Linux FUSE passthrough** — **Normal files: yes. Zero-copy: yes. Arbitrary
  layout: yes.** A passthrough FUSE server maps each virtual path to an open
  source fd and serves reads directly, so no data is copied and consumers see
  ordinary files (not symlinks). It can assemble any restructured tree. Cost: a
  FUSE server process must run for the life of the view, plus `fuse` module /
  libfuse availability.
- **Windows ProjFS** — **Normal files: yes. Zero-copy: yes (on access).
  Arbitrary layout: yes.** ProjFS projects a virtual namespace; placeholders
  hydrate on access and appear as normal files to tools. A provider process
  answers callbacks and can map any virtual path to any backing source file.
  Cost: optional feature enabled + a running provider.
- **Windows WinFsp** — **Normal files: yes. Zero-copy: yes. Arbitrary layout:
  yes.** Equivalent capability to FUSE on Windows; entries are normal files and
  reads can be served straight from the source. Cost: third-party driver install
  + a running user-mode FS host.
- **macOS / Linux CoW reflink** — **Normal files: yes. Zero-copy: yes (on CoW
  filesystems).** Reflinks are independent normal files sharing physical extents
  copy-on-write; making the clone read-only does not affect the source. On
  non-CoW filesystems this degrades to a full byte-copy (the problem under
  study).

**Summary:** mechanisms that deliver *all three* of {normal files, zero-copy,
arbitrary restructured layout} are **FUSE (Linux)**, **ProjFS/WinFsp
(Windows)**, and **reflink (CoW filesystems)**. Each non-reflink mechanism
requires a long-lived daemon and an OS feature/driver. A bare bind mount is
zero-copy and normal-file but **not** a general restructuring view.

## Recommended default per platform

- **Linux**
  - Default today: **byte-copy fallback** (current behavior) on non-CoW
    filesystems; **reflink** automatically on Btrfs/XFS.
  - Recommended *future* zero-copy path: **FUSE passthrough**, because it is the
    only mechanism that gives normal files + zero-copy + arbitrary layout
    without root.
  - Read-only **bind mount** only when the virtual layout equals the source
    layout (no restructuring), as a privileged, low-overhead special case.
- **Windows**
  - Default today: **byte-copy fallback** (current behavior).
  - Recommended *future* zero-copy path: **ProjFS**, because it is in-box
    (no third-party driver), unlike WinFsp. WinFsp is the fallback when ProjFS
    is unavailable or richer POSIX-like semantics are needed.
- **macOS**
  - Default: **APFS reflink** — already zero-copy, no change needed. Byte-copy
    only on non-APFS volumes, documented as a limitation.

## Decision

**Decision: this remains a documented limitation. No implementation project is
started now.** The byte-copy fallback (with reflink wherever the filesystem
supports CoW) stays the enforced contract for non-CoW filesystems.

**Trigger conditions that would flip this to an implementation project:**

- Real user reports that byte-copy disk duplication on ext4/NTFS is a blocking
  cost (large files, many snapshots, or many repeated virtual layouts), **and**
- A consumer needs the *restructured* (non-mirror) virtual tree on a non-CoW
  filesystem — i.e. the bind-mount special case is insufficient.

If both hold, scope a **separate, optional** implementation project: **FUSE
passthrough on Linux** and **ProjFS on Windows**, gated behind an explicit
materialization strategy / feature flag, **never the default**. macOS needs
nothing further.

### ADR

- **Decision:** Keep zero-copy-on-non-CoW as a documented limitation; do not
  implement mount/projection drivers now.
- **Drivers:** (1) every viable zero-copy-with-restructuring mechanism requires
  a long-lived daemon plus an OS feature/driver, which is a large operational
  and cross-platform-testing surface; (2) it contradicts agentdir's current
  "infrastructure plumbing, mount drivers out of scope" stance in `AGENTS.md`;
  (3) macOS and CoW-Linux already get zero-copy, so the gap is narrower than
  it first appears.
- **Alternatives considered:**
  - Read-only bind mount as the general answer — rejected: cannot represent a
    restructured tree without an overlay/FUSE layer, and needs root.
  - WinFsp as the Windows default — rejected in favor of in-box ProjFS to avoid
    a mandatory third-party driver install.
  - Hardlink fallback — rejected (out of scope; corrupts the source contract).
  - Implementing FUSE/ProjFS now — rejected: cost/scope outweighs current,
    unquantified demand.
- **Why chosen:** lowest risk, preserves the read-only contract and current
  scope, and keeps the door open via clearly-bounded trigger conditions.
- **Consequences:** non-CoW filesystems continue to pay byte-copy disk cost;
  this is explicit and acceptable until the trigger conditions are met.
- **Follow-ups:** if triggered, open a separate feature-flagged project for FUSE
  (Linux) + ProjFS (Windows); update `AGENTS.md` and this document with the new
  status.
