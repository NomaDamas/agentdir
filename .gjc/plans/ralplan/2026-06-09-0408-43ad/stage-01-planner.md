# Plan — Finish remaining work on issues #18 and #19

## RALPLAN-DR Summary

### Principles
1. **Stay in scope.** #19 is research + a decision, NOT a mount-driver implementation (AGENTS.md explicitly lists mount drivers as out of current scope). #18 is two non-destructive CI gates, nothing more.
2. **Single source of truth, no contradictions.** The #19 decision must be reconciled with AGENTS.md so the repo never states two different things about zero-copy views.
3. **Verification without side effects.** No real publishes anywhere. #18 changes are dry-run only; verification is structural (grep/stdlib runner), not a live release.
4. **Honor existing conventions.** Reuse the zero-dependency stdlib QA runner at `crates/agentdir/tests/issue_18_release_qa.py`; do not add a root pytest dependency.
5. **Correctness of placement.** The `agentdir-cli` dry-run must run AFTER the crates.io sparse-index wait, because the dry-run resolves the `agentdir` dependency from the registry.

### Decision Drivers
1. Acceptance-criteria completeness for #19 (four explicit criteria: approaches+privileges, per-platform default, normal-file-compat-without-copy verdict, implementation-vs-limitation decision).
2. Discoverability + non-duplication: where the #19 doc lives so AGENTS.md/README readers find it and nothing contradicts.
3. Low risk / reversibility of the #18 CI edits (must not break or trigger real publishes).

### Viable Options (key decision: where the #19 doc lives + how deep)
- **Option A — Single `docs/zero-copy-readonly-views.md` feasibility+decision doc, AGENTS.md reconciled to link it.**
  - Pros: one discoverable home; room for the full per-platform table + privileges + verdict + decision; AGENTS.md stays a concise pointer; matches "document feasible approaches" acceptance wording.
  - Cons: introduces a `docs/` dir (none today); slight risk of AGENTS.md/doc drift (mitigated by making AGENTS.md a pointer, not a duplicate).
- **Option B — Expand the AGENTS.md "Future / Out of Current Scope" section in place.**
  - Pros: no new dir; single file.
  - Cons: AGENTS.md is an agent-context file, not a research home; a full feasibility analysis bloats it and buries operational guidance; weak fit for "document approaches + privileges/deployment".
- **Option C — Formal ADR under `docs/adr/NNNN-*.md`.**
  - Pros: strong decision provenance.
  - Cons: heavier ceremony than the issue asks; issue wants a feasibility writeup + decision, not just a terse ADR; still needs the prose feasibility body somewhere.

**Chosen: Option A.** Invalidation: B fails the "document approaches/privileges/deployment" depth bar and pollutes agent context; C is over-ceremonious and still needs the feasibility prose Option A already provides (the decision section inside the doc captures ADR-style rationale without a separate adr/ tree).

## Scope

### Must have
- `docs/zero-copy-readonly-views.md` covering all four #19 acceptance criteria.
- AGENTS.md "Future / Out of Current Scope" reconciled to point at the new doc with no contradictory claims.
- `cargo publish -p agentdir-cli --dry-run` added to `release-rust.yml` after the sparse-index wait, before `Publish agentdir-cli`.
- `npm pack --dry-run` added to `release-node.yml` before `npm publish`.
- A zero-dependency stdlib structural check (extend `crates/agentdir/tests/issue_18_release_qa.py`) asserting both new workflow gates exist.

### Must NOT have
- No mount-driver / FUSE / ProjFS / WinFsp implementation, no runtime/library/CLI/binding behavior change.
- No real publish to crates.io / PyPI / npm.
- No root pytest or new dependency; checks stay stdlib-only.
- No symlink-as-default, no hardlink reintroduction, no file parsing/indexing.

## Tasks

### T1 — Author `docs/zero-copy-readonly-views.md` (Issue #19 primary)
- File: `docs/zero-copy-readonly-views.md` (new).
- Sections mapped to acceptance criteria:
  - `## Problem & constraints` — restate read-only virtual tree, writable source, normal-file visibility, symlink-skipping tools must see files, no parsing. State non-goals.
  - `## Per-platform approaches` — a table + prose covering: Linux read-only bind mount (`mount --bind` + `mount -o remount,ro,bind`), Linux FUSE passthrough (e.g. passthrough_hp / libfuse); Windows ProjFS (Projected File System) and WinFsp; macOS APFS reflink (already the implemented path). For each: mechanism, **privileges required** (root/CAP_SYS_ADMIN, mount namespaces, developer-mode/optional Windows feature, driver install, admin), **deployment requirements** (kernel/feature/driver availability), and whether it needs a long-lived daemon.
  - `## Normal-file compatibility without copying` — per mechanism, explicit verdict: does it present entries as normal files to symlink-skipping tools AND avoid data copy? (bind mount: yes, normal files, zero-copy, but mirrors source layout exactly so it cannot restructure into an arbitrary virtual tree without overlay; FUSE: yes normal-file appearance + zero-copy + arbitrary layout, but needs a running daemon and FUSE availability; ProjFS/WinFsp: yes normal-file projection + zero-copy + arbitrary layout, but needs driver/feature + daemon; reflink: zero-copy on APFS/Btrfs/XFS, normal files, but copies on non-CoW.)
  - `## Recommended default per platform` — Linux: keep byte-copy fallback as default today; FUSE passthrough is the recommended *future* zero-copy path; bind mount only when layout == source. Windows: keep byte-copy fallback default; ProjFS recommended future path (built-in, lighter than WinFsp). macOS: APFS reflink already zero-copy; document byte-copy only on non-APFS volumes.
  - `## Decision` — ADR-style: **Decision** (recommend: keep as documented limitation now; spin a *separate, optional* implementation project for FUSE (Linux) + ProjFS (Windows) only if disk-duplication on non-CoW filesystems becomes a real user pain — gated behind a feature flag, never default). Include Drivers, Alternatives considered, Why chosen, Consequences, Follow-ups.
- Acceptance: doc exists; contains headings for each of the four criteria; names Linux bind mount + FUSE, Windows ProjFS + WinFsp, macOS APFS reflink; states a per-platform default; states an explicit normal-file-without-copy verdict per mechanism; ends with a clear implementation-project-vs-documented-limitation decision.

### T2 — Reconcile AGENTS.md "Future / Out of Current Scope"
- File: `AGENTS.md` (the existing zero-copy table, ~lines 104-112).
- Change: keep the short table, add a sentence linking to `docs/zero-copy-readonly-views.md` as the authoritative feasibility+decision writeup; ensure the table's claims match the doc (no contradiction). Do not duplicate the full analysis into AGENTS.md.
- Acceptance: AGENTS.md references `docs/zero-copy-readonly-views.md`; no claim in AGENTS.md contradicts the doc's decision.

### T3 — release-rust.yml: add `cargo publish -p agentdir-cli --dry-run`
- File: `.github/workflows/release-rust.yml`.
- Change: insert a step `Preflight packaging check (agentdir-cli)` running `cargo publish -p agentdir-cli --dry-run` AFTER the "Wait for crates.io sparse index" step and BEFORE the "Publish agentdir-cli" step (so the just-published `agentdir` dep resolves from the registry).
- Acceptance: YAML contains `cargo publish -p agentdir-cli --dry-run`; it appears after the sparse-index wait and before the agentdir-cli publish step; `make lint`/`make test` unaffected (workflow-only change). NO real publish.

### T4 — release-node.yml: add `npm pack --dry-run`
- File: `.github/workflows/release-node.yml`.
- Change: insert a step `Preflight packaging check (npm pack)` running `npm pack --dry-run` (working-directory `bindings/node`) after artifacts are moved and BEFORE the `Publish` (`npm publish`) step.
- Acceptance: YAML contains `npm pack --dry-run` before `npm publish`. NO real publish.

### T5 — Extend zero-dependency QA runner with structural checks
- File: `crates/agentdir/tests/issue_18_release_qa.py`.
- Change: add `test_release_rust_workflow_dry_runs_cli_before_publish()` asserting `"cargo publish -p agentdir-cli --dry-run" in read(".github/workflows/release-rust.yml")`, and `test_release_node_workflow_packs_before_publish()` asserting `"npm pack --dry-run" in read(".github/workflows/release-node.yml")`. Optionally also assert ordering by index comparison. Keep stdlib-only.
- Acceptance: `python3 crates/agentdir/tests/issue_18_release_qa.py` exits 0 and prints PASS for both new checks (after T3/T4).

## Verification strategy

- **Issue #19 (T1, T2):** acceptance-criteria coverage checklist — grep the doc for required headings/mechanisms; manual doc review that each of (a)-(d) is answered; AGENTS.md grep for the doc link; confirm no contradictory wording. No runtime code, no tests to run beyond existence/structure.
- **Issue #18 (T3, T4, T5):** structural grep on the two YAML files for the new dry-run strings + ordering; run `python3 crates/agentdir/tests/issue_18_release_qa.py` and confirm exit 0 with the two new PASS lines; confirm `make test` and `make lint` are unaffected (no Rust/Python source touched besides the stdlib check file, which is not part of cargo build). **Explicitly: no `cargo publish`, `npm publish`, `npm pack`, or `maturin` is executed locally — verification is purely textual/structural.**
- Final: re-read both YAML files to confirm step ordering is correct.

## Sequencing & commits
- T1 → T2 (T2 links the doc, so T1 first). T3, T4, T5 are independent of #19 and of each other (T5 asserts T3/T4 output, so run T5's verification after T3/T4).
- Suggested commits (Conventional Commits; footer `Plan: plans/<this-plan>`):
  1. `docs(materialization): document zero-copy read-only view feasibility and decision (#19)` — T1 + T2.
  2. `ci(release): add agentdir-cli and npm pack dry-run gates (#18)` — T3 + T4 + T5.
