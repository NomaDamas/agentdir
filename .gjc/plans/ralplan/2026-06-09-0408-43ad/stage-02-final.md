# Plan (rev 2) — Finish remaining work on issues #18 and #19

> Revision incorporating Critic pass-1 items 1-4 and Architect comments C1-C4.

## RALPLAN-DR Summary

### Principles
1. **Stay in scope.** #19 is research + a decision, NOT a mount-driver implementation (AGENTS.md lists mount drivers as out of current scope). #18 is two non-destructive CI gates.
2. **Single source of truth, no contradictions.** The #19 decision is reconciled with AGENTS.md; the repo never states two different things about zero-copy views.
3. **Verification without side effects.** No real publishes; #18 changes are dry-run only; verification is structural (grep/stdlib runner).
4. **Honor existing conventions.** Reuse the zero-dependency stdlib QA runner `crates/agentdir/tests/issue_18_release_qa.py`; no root pytest dependency.
5. **Correctness of placement.** The `agentdir-cli` dry-run runs AFTER the crates.io sparse-index wait, because it resolves the `agentdir` dependency from the registry.

### Decision Drivers
1. Acceptance-criteria completeness for #19 (approaches+privileges, per-platform default, normal-file-compat-without-copy verdict, implementation-vs-limitation decision).
2. Discoverability + non-duplication of the #19 doc; no AGENTS.md contradiction.
3. Low risk / reversibility of #18 CI edits (no breakage, no real publish).

### Viable Options (key decision: where the #19 doc lives + depth)
- **Option A — Single `docs/zero-copy-readonly-views.md` feasibility+decision doc, AGENTS.md reconciled to link it.** Pros: discoverable home; room for full table+privileges+verdict+decision; AGENTS.md stays concise. Cons: new `docs/` dir; drift risk (mitigated: AGENTS.md is a pointer, not a duplicate).
- **Option B — Expand AGENTS.md in place.** Pros: no new dir. Cons: bloats agent-context file; weak fit for privileges/deployment depth.
- **Option C — Formal ADR under `docs/adr/`.** Pros: decision provenance. Cons: over-ceremonious; still needs feasibility prose.

**Chosen: Option A.** Invalidation: B fails the depth bar and pollutes agent context; C is heavier than the issue asks and Option A's `## Decision` section already captures ADR-style rationale.

## Scope

### Must have
- `docs/zero-copy-readonly-views.md` covering all four #19 acceptance criteria.
- AGENTS.md "Future / Out of Current Scope" reconciled to point at the new doc, no contradiction.
- `cargo publish -p agentdir-cli --dry-run` in `release-rust.yml` after the sparse-index wait, before `Publish agentdir-cli`.
- `npm pack --dry-run` in `release-node.yml` before `npm publish`.
- Zero-dependency stdlib structural checks (extend `crates/agentdir/tests/issue_18_release_qa.py`) asserting both new gates.

### Must NOT have
- No mount-driver/FUSE/ProjFS/WinFsp implementation; no library/CLI/binding runtime change.
- No real publish to crates.io/PyPI/npm.
- No root pytest/new dependency; checks stay stdlib-only.
- No symlink-as-default, no hardlink reintroduction, no file parsing/indexing.

## Tasks

### T1 — Author `docs/zero-copy-readonly-views.md` (Issue #19 primary)
- File: `docs/zero-copy-readonly-views.md` (new).
- Required sections:
  - `## Problem & constraints` — read-only virtual tree, writable source, normal-file visibility, symlink-skipping tools must see files, no parsing. State non-goals (no hardlink, symlink not default, no parsing, no orchestrator).
  - `## Per-platform approaches` — table + prose: Linux read-only **bind mount** (`mount --bind` + `mount -o remount,ro,bind`) and Linux **FUSE** passthrough; Windows **ProjFS** and **WinFsp**; macOS **APFS reflink** (implemented path). For each: mechanism, privileges (root/CAP_SYS_ADMIN, mount namespaces; Windows optional-feature/developer-mode or driver install/admin), deployment requirements (kernel/feature/driver availability), and whether a long-lived daemon is needed.
  - `## Normal-file compatibility without copying` — per-mechanism explicit verdict (normal files to symlink-skipping tools AND no data copy):
    - bind mount: normal files + zero-copy, **BUT mirrors source layout — cannot produce agentdir's arbitrary restructured virtual tree without an overlay/FUSE layer** (Architect C2).
    - FUSE: normal-file appearance + zero-copy + arbitrary layout, needs a running daemon + FUSE availability.
    - ProjFS/WinFsp: normal-file projection + zero-copy + arbitrary layout, needs driver/feature + daemon.
    - reflink: zero-copy on **CoW filesystems (APFS, Btrfs, XFS) — note CoW is filesystem-level, not macOS-only** (Architect C4); byte-copy on non-CoW.
  - `## Recommended default per platform` — Linux: keep byte-copy fallback default today; FUSE passthrough = recommended future zero-copy path; bind mount only when virtual layout == source. Windows: keep byte-copy fallback default; ProjFS = recommended future path (built-in, lighter than WinFsp). macOS: APFS reflink already zero-copy; byte-copy only on non-CoW volumes.
  - `## Decision` — **opens with ONE unambiguous verdict sentence** (Architect C1 / Critic item 1): e.g. "Decision: this remains a documented limitation; no implementation project is started now." Followed by trigger conditions that would flip it (e.g. real user reports of disk duplication on non-CoW filesystems), then ADR fields: Drivers, Alternatives considered, Why chosen, Consequences, Follow-ups. Any future work scoped as a separate optional project (FUSE on Linux, ProjFS on Windows), feature-flagged, never default.
- Acceptance (agent-checkable):
  - `test -f docs/zero-copy-readonly-views.md`
  - `grep -qi "bind mount" docs/zero-copy-readonly-views.md`
  - `grep -qi "FUSE" docs/zero-copy-readonly-views.md`
  - `grep -qi "ProjFS" docs/zero-copy-readonly-views.md && grep -qi "WinFsp" docs/zero-copy-readonly-views.md`
  - `grep -qi "reflink" docs/zero-copy-readonly-views.md`
  - `grep -q "## Recommended default per platform" docs/zero-copy-readonly-views.md`
  - `grep -q "## Decision" docs/zero-copy-readonly-views.md` and the line after it states one verdict (documented-limitation OR implementation-project).
  - Doc contains the bind-mount "cannot restructure without overlay/FUSE" statement and the "CoW is filesystem-level" nuance.

### T2 — Reconcile AGENTS.md "Future / Out of Current Scope"
- File: `AGENTS.md` (existing zero-copy table ~lines 104-112).
- Change: keep the short table; add a sentence linking to `docs/zero-copy-readonly-views.md` as the authoritative feasibility+decision writeup; ensure table claims match the doc. No full-analysis duplication.
- Acceptance: `grep -q "docs/zero-copy-readonly-views.md" AGENTS.md`; no AGENTS.md claim contradicts the doc's decision.

### T3 — release-rust.yml: add `cargo publish -p agentdir-cli --dry-run`
- File: `.github/workflows/release-rust.yml`.
- Change: insert step `Preflight packaging check (agentdir-cli)` running `cargo publish -p agentdir-cli --dry-run` AFTER "Wait for crates.io sparse index" and BEFORE "Publish agentdir-cli".
- Acceptance: `grep -q "cargo publish -p agentdir-cli --dry-run" .github/workflows/release-rust.yml`; step appears after the sparse-index wait and before agentdir-cli publish; no real publish.

### T4 — release-node.yml: add `npm pack --dry-run`
- File: `.github/workflows/release-node.yml`.
- Change: insert step `Preflight packaging check (npm pack)` running `npm pack --dry-run` (working-directory `bindings/node`) after "Move artifacts" and BEFORE "Publish" (`npm publish`).
- Acceptance: `grep -q "npm pack --dry-run" .github/workflows/release-node.yml`; appears before `npm publish`; no real publish.

### T5 — Extend zero-dependency QA runner (stdlib contains-checks)
- File: `crates/agentdir/tests/issue_18_release_qa.py`.
- Change: add two `test_*` functions using plain `in` contains-checks (Critic item 3 / Architect C3 — no brittle index ordering):
  - `test_release_rust_workflow_dry_runs_cli_before_publish()`: assert `"cargo publish -p agentdir-cli --dry-run" in read(".github/workflows/release-rust.yml")`.
  - `test_release_node_workflow_packs_before_publish()`: assert `"npm pack --dry-run" in read(".github/workflows/release-node.yml")`.
- Acceptance: `python3 crates/agentdir/tests/issue_18_release_qa.py` exits 0 with PASS for both new checks (after T3/T4). Stdlib-only.

## Verification strategy

- **Issue #19 (T1, T2) — agent-executable grep checklist (Critic item 2):**
  - `test -f docs/zero-copy-readonly-views.md`
  - `grep -qi "bind mount" docs/zero-copy-readonly-views.md`
  - `grep -qi "FUSE" docs/zero-copy-readonly-views.md`
  - `grep -qi "ProjFS" docs/zero-copy-readonly-views.md && grep -qi "WinFsp" docs/zero-copy-readonly-views.md`
  - `grep -qi "reflink" docs/zero-copy-readonly-views.md`
  - `grep -q "## Recommended default per platform" docs/zero-copy-readonly-views.md`
  - `grep -q "## Decision" docs/zero-copy-readonly-views.md`
  - `grep -q "docs/zero-copy-readonly-views.md" AGENTS.md`
  - Manual read confirming each of #19 (a)-(d) is answered and the Decision is one verdict + triggers.
- **Issue #18 (T3, T4, T5):**
  - `grep -q "cargo publish -p agentdir-cli --dry-run" .github/workflows/release-rust.yml`
  - `grep -q "npm pack --dry-run" .github/workflows/release-node.yml`
  - `python3 crates/agentdir/tests/issue_18_release_qa.py` exits 0 (all PASS, incl. 2 new).
  - `make test` and `make lint` unaffected (no cargo-built source changed).
  - **Explicitly NO `cargo publish`, `npm publish`, `npm pack`, or `maturin` executed locally; verification is purely textual/structural.**
- Final: re-read both YAML files to confirm step ordering.

## Sequencing & commits
- T1 → T2 (T2 links the doc). T3, T4, T5 independent of #19; run T5 verification after T3/T4.
- Commits (Conventional Commits; footer references this plan file):
  1. `docs(materialization): document zero-copy read-only view feasibility and decision (#19)` — T1 + T2.
  2. `ci(release): add agentdir-cli and npm pack dry-run gates (#18)` — T3 + T4 + T5.

## ADR (final)
- **Decision:** Address the two issue-#18 release-workflow gaps with non-destructive dry-run gates, and resolve issue #19 by delivering a feasibility+decision document (Option A) that recommends keeping zero-copy-on-non-CoW as a documented limitation, with a clearly-scoped optional future project.
- **Drivers:** acceptance-criteria completeness; scope discipline (no driver impl); zero side effects.
- **Alternatives considered:** AGENTS.md-in-place (B), formal ADR tree (C), changing workflows to run full preflight without `--metadata-only` (rejected: would run the cli dry-run before `agentdir` is on crates.io and fail).
- **Why chosen:** satisfies #19 depth + decision in a discoverable doc; #18 fix is minimal, correctly ordered, reversible, no publish.
- **Consequences:** new `docs/` dir; CI release runs gain two dry-run gates (longer release runtime, earlier failure detection); AGENTS.md gains a pointer.
- **Follow-ups:** if disk-duplication on non-CoW becomes a real pain, open a separate FUSE(Linux)+ProjFS(Windows) implementation project behind a feature flag.
