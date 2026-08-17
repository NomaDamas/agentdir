# Issue 18 Cross-Platform Release QA

## TL;DR
> Summary:      Add native Linux/macOS/Windows release QA for Rust tests, CLI behavior, materialization strategies, release metadata, and Python wheel installs without changing agentdir runtime behavior.
> Deliverables: cross-OS CI matrix; CLI and strategy smoke scripts; release preflight script and workflow gates; Python wheel post-build smoke job; tmux evidence for every task.
> Effort:       Large
> Risk:         Medium - native OS behavior, Windows symlink privileges, and cross-arch wheel artifacts can make CI flaky unless scripts define exact pass/skip rules.

## Scope
### Must have
- PR CI runs `cargo test --workspace` on `ubuntu-latest`, `macos-latest`, and `windows-latest`.
- PR CI runs a reusable CLI smoke matrix on all three native OSes covering `init`, `map`, `stat`, `cat`, `refresh`, `export-mapping`, and default read-only materialization.
- PR CI runs explicit strategy smoke coverage for `reflink`, `virtual`, and `symlink`; the `symlink` case must assert and label writes through the materialized tree as passthrough/unsafe because they mutate the source.
- Release workflows check version/metadata synchronization before publishing.
- Rust release preflight covers both crates: keep `agentdir` dry-run and add `agentdir-cli` dry-run before publishing `agentdir-cli`.
- Node release keeps the existing native OS runtime test matrix and adds metadata/package preflight, not a redundant runtime matrix.
- Python release keeps the existing wheel build matrix and adds a post-build import/API smoke on each native runner using a host-compatible wheel selected by `pip`.
- All new checks are zero-dependency except existing repo toolchains; no root `pytest` or YAML parser dependency is introduced.
- TDD evidence is captured: add the relevant check first, capture RED, implement, then capture GREEN.

### Must NOT have (guardrails, anti-slop, scope boundaries)
- Do not change library, CLI, Python binding, or Node binding runtime semantics.
- Do not make `symlink` mode sound read-only; it is explicitly a passthrough/unsafe exception.
- Do not treat Wine/cross as the Windows release signal; issue #18 requires native `windows-latest`.
- Do not publish to crates.io, PyPI, or npm during verification.
- Do not add root package metadata, root Python dependencies, or a new release framework if plain scripts are sufficient.
- Do not add file parsing/indexing, AI behavior, mount drivers, or any out-of-scope functionality from `AGENTS.md`.

## Verification strategy
> Zero human intervention - all verification is agent-executed.
- Test decision: TDD + zero-dependency Python structural checks in `tests/test_issue_18_release_qa.py` and executable smoke scripts in `scripts/ci/`
- QA policy: every task has agent-executed scenarios
- Evidence: `evidence/task-<N>-<slug>.<ext>`

## Execution strategy
### Parallel execution waves
> Target 5-8 tasks per wave. <3 per wave (except final) = under-splitting.
> Extract shared dependencies as Wave-1 tasks to maximize parallelism.

Wave 1 (no dependencies):
- Task 1: Add the issue-18 check runner and empty discovery package

Wave 2 (after Wave 1):
- Task 2: depends [1] - add the cargo workspace OS-matrix check and CI implementation
- Task 3: depends [1] - add the CLI smoke script and local check
- Task 4: depends [1] - add the strategy smoke script and local check
- Task 6: depends [1] - add release preflight script and release workflow gates

Wave 3 (after Wave 2):
- Task 5: depends [2, 3, 4] - wire CLI and strategy smoke matrices into CI
- Task 7: depends [6] - add Python wheel post-build smoke script and release workflow job

Critical path: Task 1 -> Task 3 -> Task 5 -> Final verification

### Dependency matrix
| Task | Depends on | Blocks | Can parallelize with |
|------|------------|--------|----------------------|
| 1 | none | 2, 3, 4, 6 | none |
| 2 | 1 | 5 | 3, 4, 6 |
| 3 | 1 | 5 | 2, 4, 6 |
| 4 | 1 | 5 | 2, 3, 6 |
| 5 | 2, 3, 4 | F1, F3 | 7 |
| 6 | 1 | 7 | 2, 3, 4 |
| 7 | 6 | F1, F3 | 5 |

## Todos
> Implementation + Test = ONE task. Never separate.
> Every task MUST have: References + Acceptance Criteria + QA Scenarios + Commit.

- [ ] 1. Add issue-18 check runner

  What to do: Create `tests/test_issue_18_release_qa.py` as a zero-dependency Python runner with `--list` and `-k <substring>` support. It must discover functions named `test_*` from modules under `tests/issue_18_checks/` and print `PASS <name>` / `FAIL <name>` lines. Create `tests/issue_18_checks/__init__.py`. Do not add actual issue checks yet; later tasks own one check module each so they can do RED/GREEN without committing unrelated failures.
  Must NOT do: Do not add `pytest`, root `pyproject.toml`, or root package metadata. Do not create any scripts under `scripts/ci/` in this task.

  Parallelization: Can parallel: NO | Wave 1 | Blocks: [2, 3, 4, 6] | Blocked by: []

  References (executor has NO interview context - be exhaustive):
  - Pattern:  `bindings/python/pyproject.toml:46` - existing pytest config is scoped to Python bindings only, so root checks should avoid depending on it.
  - Pattern:  `.omo/ulw-loop/issue-18-notepad.md:14` - issue binding criteria use function-style checks such as `tests/test_issue_18_release_qa.py::test_ci_runs_rust_workspace_tests_on_linux_macos_windows`.
  - Pattern:  `.omo/ulw-loop/issue-18-notepad.md:16` - manual QA examples run the top-level test file with a `-k` selector, so the runner must support this directly.
  - External: `https://docs.python.org/3/library/argparse.html` - use standard-library argument parsing only.

  Acceptance criteria (agent-executable only):
  - [ ] `python3 tests/test_issue_18_release_qa.py --list` exits 0 and prints `No issue-18 checks discovered` when no check modules exist.
  - [ ] `python3 tests/test_issue_18_release_qa.py -k does_not_exist` exits nonzero and prints `No checks matched: does_not_exist`.
  - [ ] `python3 -m compileall -q tests/test_issue_18_release_qa.py tests/issue_18_checks` exits 0.

  QA scenarios (MANDATORY - task incomplete without these):
  > Name the exact tool AND its exact invocation - not "verify it works". Browser use: use Chrome to drive the page; if Chrome is not available, download and use agent-browser (https://github.com/vercel-labs/agent-browser). Computer use: OS-level GUI automation for a non-browser desktop app.
  ```
  Scenario: runner lists with no checks
    Tool:     tmux
    Steps:    tmux new-session -d -s issue18_t1 'cd /Users/jeffrey/Projects-dev/agentdir && mkdir -p evidence && python3 tests/test_issue_18_release_qa.py --list > evidence/task-1-runner.txt 2>&1; printf "%s" "$?" > evidence/task-1-runner.exit'; while tmux has-session -t issue18_t1 2>/dev/null; do sleep 1; done; test "$(cat evidence/task-1-runner.exit)" = "0"; grep -q "No issue-18 checks discovered" evidence/task-1-runner.txt
    Expected: command exits 0 and transcript contains `No issue-18 checks discovered`
    Evidence: evidence/task-1-runner.txt

  Scenario: unmatched selector fails clearly
    Tool:     tmux
    Steps:    tmux new-session -d -s issue18_t1_error 'cd /Users/jeffrey/Projects-dev/agentdir && mkdir -p evidence && python3 tests/test_issue_18_release_qa.py -k does_not_exist > evidence/task-1-runner-error.txt 2>&1; printf "%s" "$?" > evidence/task-1-runner-error.exit'; while tmux has-session -t issue18_t1_error 2>/dev/null; do sleep 1; done; test "$(cat evidence/task-1-runner-error.exit)" != "0"; grep -q "No checks matched: does_not_exist" evidence/task-1-runner-error.txt
    Expected: command exits nonzero and transcript names the unmatched selector
    Evidence: evidence/task-1-runner-error.txt
  ```

  Commit: YES | Message: `test(release): add issue 18 check runner` | Files: [`tests/test_issue_18_release_qa.py`, `tests/issue_18_checks/__init__.py`]

- [ ] 2. Add native cargo workspace OS matrix in CI

  What to do: First add `tests/issue_18_checks/ci_matrix.py` with `test_ci_runs_rust_workspace_tests_on_linux_macos_windows`. Capture RED against the current workflow. Then update `.github/workflows/ci.yml` so the Rust job runs `cargo test --workspace` on `ubuntu-latest`, `macos-latest`, and `windows-latest`. Keep `cargo fmt --check` and `cargo clippy --workspace -- -D warnings` as Ubuntu-only gates or otherwise avoid multiplying lint failures across OSes. Preserve `PYO3_USE_ABI3_FORWARD_COMPATIBILITY: "1"` because the current CI sets it at `.github/workflows/ci.yml:8`.
  Must NOT do: Do not move Python or Node jobs to a cross-OS matrix in this task. Do not remove existing Python or Node checks.

  Parallelization: Can parallel: YES | Wave 2 | Blocks: [5] | Blocked by: [1]

  References (executor has NO interview context - be exhaustive):
  - Pattern:  `.github/workflows/ci.yml:13` - current Rust job is the PR/push Rust gate.
  - Pattern:  `.github/workflows/ci.yml:15` - current Rust job runs only on `ubuntu-latest`.
  - Pattern:  `.github/workflows/ci.yml:25` - current Rust job already runs `cargo fmt --check`.
  - Pattern:  `.github/workflows/ci.yml:26` - current Rust job already runs clippy with `-D warnings`.
  - Pattern:  `.github/workflows/ci.yml:27` - current Rust job already runs `cargo test --workspace`, but only on Ubuntu.
  - Pattern:  `.github/workflows/ci.yml:29` - Python job currently depends on `rust`; keep dependency semantics after matrix conversion.
  - Pattern:  `.github/workflows/ci.yml:66` - Node job currently depends on `rust`; keep dependency semantics after matrix conversion.
  - External: `https://docs.github.com/en/actions/writing-workflows/workflow-syntax-for-github-actions#jobsjob_idstrategy` - matrix strategy syntax.
  - External: `https://docs.github.com/en/actions/writing-workflows/workflow-syntax-for-github-actions#jobsjob_idruns-on` - `runs-on: ${{ matrix.os }}`.

  Acceptance criteria (agent-executable only):
  - [ ] RED evidence exists: `python3 tests/test_issue_18_release_qa.py -k ci_runs_rust_workspace_tests_on_linux_macos_windows` failed before editing `.github/workflows/ci.yml`, with output captured in `evidence/task-2-ci-matrix-red.txt`.
  - [ ] `python3 tests/test_issue_18_release_qa.py -k ci_runs_rust_workspace_tests_on_linux_macos_windows` exits 0 after the workflow edit.
  - [ ] The check asserts all three strings exist in `.github/workflows/ci.yml`: `ubuntu-latest`, `macos-latest`, `windows-latest`.
  - [ ] The check asserts `cargo test --workspace` is inside a job using `runs-on: ${{ matrix.os }}` or equivalent matrix include syntax.
  - [ ] `git diff --check -- .github/workflows/ci.yml tests/issue_18_checks/ci_matrix.py` exits 0.

  QA scenarios (MANDATORY - task incomplete without these):
  ```
  Scenario: CI matrix check passes
    Tool:     tmux
    Steps:    tmux new-session -d -s issue18_t2 'cd /Users/jeffrey/Projects-dev/agentdir && mkdir -p evidence && python3 tests/test_issue_18_release_qa.py -k ci_runs_rust_workspace_tests_on_linux_macos_windows > evidence/task-2-ci-matrix.txt 2>&1; printf "%s" "$?" > evidence/task-2-ci-matrix.exit'; while tmux has-session -t issue18_t2 2>/dev/null; do sleep 1; done; test "$(cat evidence/task-2-ci-matrix.exit)" = "0"; grep -q "PASS test_ci_runs_rust_workspace_tests_on_linux_macos_windows" evidence/task-2-ci-matrix.txt
    Expected: command exits 0 and transcript contains the PASS line
    Evidence: evidence/task-2-ci-matrix.txt

  Scenario: RED was captured before workflow edit
    Tool:     bash
    Steps:    test -s evidence/task-2-ci-matrix-red.txt && grep -Eq "FAIL|AssertionError|missing|expected" evidence/task-2-ci-matrix-red.txt
    Expected: RED transcript exists and shows the pre-implementation failure
    Evidence: evidence/task-2-ci-matrix-red.txt
  ```

  Commit: YES | Message: `ci(rust): test workspace on native os matrix` | Files: [`.github/workflows/ci.yml`, `tests/issue_18_checks/ci_matrix.py`]

- [ ] 3. Add reusable CLI smoke script

  What to do: First add `tests/issue_18_checks/cli_smoke_artifact.py` with `test_cli_smoke_script_exercises_required_commands_and_readonly_check`; capture RED while `scripts/ci/cli_smoke.py` is missing. Then create `scripts/ci/cli_smoke.py` as a cross-platform Python script using only the standard library. It must accept `--agentdir <path>` and optional `--keep-temp`. It must create temporary source/workspace directories, run the CLI through `init`, `map`, `stat`, `cat`, `refresh`, and `export-mapping`, assert default materialized files are read-only by metadata, assert source edits/additions/deletions are reflected after `refresh`, and print exactly `CLI smoke passed`.
  Must NOT do: Do not use shell-only constructs inside the script. Do not edit the CLI implementation unless a real bug prevents the smoke from passing; if that happens, pause and spawn a worker with a new implementation task.

  Parallelization: Can parallel: YES | Wave 2 | Blocks: [5] | Blocked by: [1]

  References (executor has NO interview context - be exhaustive):
  - Pattern:  `crates/agentdir-cli/src/main.rs:27` - command enum defines the CLI surface.
  - Pattern:  `crates/agentdir-cli/src/main.rs:167` - `init` implementation prints initialized workspace and manifest path.
  - Pattern:  `crates/agentdir-cli/src/main.rs:178` - `map` implementation canonicalizes the source and prints summary counts.
  - Pattern:  `crates/agentdir-cli/src/main.rs:253` - `stat` prints path/source/size/mtime/type/materialized fields.
  - Pattern:  `crates/agentdir-cli/src/main.rs:266` - `cat` writes source bytes to stdout.
  - Pattern:  `crates/agentdir-cli/src/main.rs:274` - `refresh` prints added/refreshed/removed/error counts.
  - Pattern:  `crates/agentdir-cli/src/main.rs:321` - `export-mapping` emits pretty JSON.
  - Pattern:  `crates/agentdir/tests/query_apis.rs:51` - API-level stat metadata behavior to mirror from the CLI.
  - Pattern:  `crates/agentdir/tests/query_apis.rs:89` - API-level read-bytes behavior to mirror from `cat`.
  - Pattern:  `crates/agentdir/tests/export_mapping.rs:12` - source-to-virtual export behavior.
  - Pattern:  `crates/agentdir/tests/readonly_materialization.rs:22` - read-only file contract for default materialization.
  - API/Type: `crates/agentdir/src/materializer.rs:86` - read-only enforcement uses `0o444` on Unix and read-only attribute on Windows.
  - External: `https://docs.python.org/3/library/tempfile.html` - temp directory isolation.
  - External: `https://docs.python.org/3/library/subprocess.html` - process execution with explicit exit-code assertions.

  Acceptance criteria (agent-executable only):
  - [ ] RED evidence exists: `python3 tests/test_issue_18_release_qa.py -k cli_smoke_script_exercises_required_commands_and_readonly_check` failed before creating `scripts/ci/cli_smoke.py`, captured in `evidence/task-3-cli-smoke-red.txt`.
  - [ ] `python3 tests/test_issue_18_release_qa.py -k cli_smoke_script_exercises_required_commands_and_readonly_check` exits 0.
  - [ ] `cargo build -p agentdir-cli` exits 0.
  - [ ] `AGENTDIR_BIN=target/debug/agentdir; test -x "$AGENTDIR_BIN" || AGENTDIR_BIN=target/debug/agentdir.exe; python3 scripts/ci/cli_smoke.py --agentdir "$AGENTDIR_BIN"` exits 0 and prints `CLI smoke passed`.
  - [ ] `python3 scripts/ci/cli_smoke.py --agentdir /no/such/agentdir` exits nonzero and prints `agentdir binary not found`.

  QA scenarios (MANDATORY - task incomplete without these):
  ```
  Scenario: CLI smoke happy path
    Tool:     tmux
    Steps:    tmux new-session -d -s issue18_t3 'cd /Users/jeffrey/Projects-dev/agentdir && mkdir -p evidence && cargo build -p agentdir-cli >/tmp/issue18_t3_build.log 2>&1 && AGENTDIR_BIN=target/debug/agentdir; test -x "$AGENTDIR_BIN" || AGENTDIR_BIN=target/debug/agentdir.exe; python3 scripts/ci/cli_smoke.py --agentdir "$AGENTDIR_BIN" --keep-temp > evidence/task-3-cli-smoke.txt 2>&1; printf "%s" "$?" > evidence/task-3-cli-smoke.exit'; while tmux has-session -t issue18_t3 2>/dev/null; do sleep 1; done; test "$(cat evidence/task-3-cli-smoke.exit)" = "0"; grep -q "CLI smoke passed" evidence/task-3-cli-smoke.txt
    Expected: command exits 0 and transcript contains `CLI smoke passed`
    Evidence: evidence/task-3-cli-smoke.txt

  Scenario: missing binary fails before temp workspace setup
    Tool:     tmux
    Steps:    tmux new-session -d -s issue18_t3_error 'cd /Users/jeffrey/Projects-dev/agentdir && mkdir -p evidence && python3 scripts/ci/cli_smoke.py --agentdir /no/such/agentdir > evidence/task-3-cli-smoke-error.txt 2>&1; printf "%s" "$?" > evidence/task-3-cli-smoke-error.exit'; while tmux has-session -t issue18_t3_error 2>/dev/null; do sleep 1; done; test "$(cat evidence/task-3-cli-smoke-error.exit)" != "0"; grep -q "agentdir binary not found" evidence/task-3-cli-smoke-error.txt
    Expected: command exits nonzero and transcript contains `agentdir binary not found`
    Evidence: evidence/task-3-cli-smoke-error.txt
  ```

  Commit: YES | Message: `test(cli): add cross-platform smoke script` | Files: [`scripts/ci/cli_smoke.py`, `tests/issue_18_checks/cli_smoke_artifact.py`]

- [ ] 4. Add explicit strategy smoke script

  What to do: First add `tests/issue_18_checks/strategy_smoke_artifact.py` with `test_strategy_smoke_script_covers_reflink_virtual_and_symlink_contracts`; capture RED while `scripts/ci/strategy_smoke.py` is missing. Then create `scripts/ci/strategy_smoke.py` as a cross-platform standard-library Python script accepting `--agentdir <path>`, optional `--keep-temp`, and optional `--allow-symlink-skip`. It must run separate workspaces for `reflink`, `virtual`, and `symlink`. It must print `reflink strategy passed`, `virtual strategy passed`, `symlink source mutation observed (passthrough/unsafe)`, and `Strategy smoke passed` on success.
  Must NOT do: Do not assert read-only protection for `symlink`. Do not silently skip symlink in CI; skipping is only allowed when `--allow-symlink-skip` is passed or `AGENTDIR_ALLOW_SYMLINK_SKIP=1`.

  Parallelization: Can parallel: YES | Wave 2 | Blocks: [5] | Blocked by: [1]

  References (executor has NO interview context - be exhaustive):
  - Pattern:  `crates/agentdir-cli/src/main.rs:32` - `init` exposes `--strategy reflink|symlink|virtual`.
  - Pattern:  `crates/agentdir-cli/src/main.rs:115` - invalid strategy error wording.
  - Pattern:  `crates/agentdir-cli/src/main.rs:124` - CLI maps strategy strings to `MaterializeStrategy`.
  - API/Type: `crates/agentdir/src/types.rs:169` - `MaterializeStrategy` variants are `Reflink`, `Symlink`, `Virtual`.
  - Pattern:  `crates/agentdir/src/materializer.rs:101` - `virtual` materialization returns without creating files.
  - Pattern:  `crates/agentdir/src/materializer.rs:114` - symlink mode creates filesystem symlinks.
  - Pattern:  `crates/agentdir/src/materializer.rs:139` - reflink mode uses clone/copy then read-only enforcement.
  - Test:     `crates/agentdir/tests/symlink_materialization.rs:5` - symlink mode creates symlinks.
  - Test:     `crates/agentdir/tests/symlink_materialization.rs:91` - virtual mode leaves no physical file but keeps catalog access.
  - Test:     `crates/agentdir/tests/symlink_materialization.rs:147` - default strategy is reflink.
  - Test:     `crates/agentdir/tests/symlink_materialization.rs:173` - reflink mode physical materialization remains non-symlink.
  - External: `https://docs.python.org/3/library/pathlib.html#pathlib.Path.is_symlink` - cross-platform symlink detection API.

  Acceptance criteria (agent-executable only):
  - [ ] RED evidence exists: `python3 tests/test_issue_18_release_qa.py -k strategy_smoke_script_covers_reflink_virtual_and_symlink_contracts` failed before creating `scripts/ci/strategy_smoke.py`, captured in `evidence/task-4-strategy-smoke-red.txt`.
  - [ ] `python3 tests/test_issue_18_release_qa.py -k strategy_smoke_script_covers_reflink_virtual_and_symlink_contracts` exits 0.
  - [ ] `cargo build -p agentdir-cli` exits 0.
  - [ ] `AGENTDIR_BIN=target/debug/agentdir; test -x "$AGENTDIR_BIN" || AGENTDIR_BIN=target/debug/agentdir.exe; python3 scripts/ci/strategy_smoke.py --agentdir "$AGENTDIR_BIN"` exits 0 and prints all required pass lines.
  - [ ] `python3 scripts/ci/strategy_smoke.py --agentdir /no/such/agentdir` exits nonzero and prints `agentdir binary not found`.

  QA scenarios (MANDATORY - task incomplete without these):
  ```
  Scenario: strategy smoke happy path
    Tool:     tmux
    Steps:    tmux new-session -d -s issue18_t4 'cd /Users/jeffrey/Projects-dev/agentdir && mkdir -p evidence && cargo build -p agentdir-cli >/tmp/issue18_t4_build.log 2>&1 && AGENTDIR_BIN=target/debug/agentdir; test -x "$AGENTDIR_BIN" || AGENTDIR_BIN=target/debug/agentdir.exe; python3 scripts/ci/strategy_smoke.py --agentdir "$AGENTDIR_BIN" --keep-temp > evidence/task-4-strategy-smoke.txt 2>&1; printf "%s" "$?" > evidence/task-4-strategy-smoke.exit'; while tmux has-session -t issue18_t4 2>/dev/null; do sleep 1; done; test "$(cat evidence/task-4-strategy-smoke.exit)" = "0"; grep -q "reflink strategy passed" evidence/task-4-strategy-smoke.txt; grep -q "virtual strategy passed" evidence/task-4-strategy-smoke.txt; grep -q "symlink source mutation observed (passthrough/unsafe)" evidence/task-4-strategy-smoke.txt; grep -q "Strategy smoke passed" evidence/task-4-strategy-smoke.txt
    Expected: command exits 0 and transcript contains all strategy pass lines
    Evidence: evidence/task-4-strategy-smoke.txt

  Scenario: missing binary fails clearly
    Tool:     tmux
    Steps:    tmux new-session -d -s issue18_t4_error 'cd /Users/jeffrey/Projects-dev/agentdir && mkdir -p evidence && python3 scripts/ci/strategy_smoke.py --agentdir /no/such/agentdir > evidence/task-4-strategy-smoke-error.txt 2>&1; printf "%s" "$?" > evidence/task-4-strategy-smoke-error.exit'; while tmux has-session -t issue18_t4_error 2>/dev/null; do sleep 1; done; test "$(cat evidence/task-4-strategy-smoke-error.exit)" != "0"; grep -q "agentdir binary not found" evidence/task-4-strategy-smoke-error.txt
    Expected: command exits nonzero and transcript contains `agentdir binary not found`
    Evidence: evidence/task-4-strategy-smoke-error.txt
  ```

  Commit: YES | Message: `test(cli): add materialization strategy smoke` | Files: [`scripts/ci/strategy_smoke.py`, `tests/issue_18_checks/strategy_smoke_artifact.py`]

- [ ] 5. Wire CLI and strategy smoke matrices into CI

  What to do: First add `tests/issue_18_checks/ci_smoke_jobs.py` with checks for CLI and strategy smoke matrix jobs in `.github/workflows/ci.yml`; capture RED. Then update `.github/workflows/ci.yml` with native OS smoke jobs that run after the Rust test matrix. Each smoke job must use `strategy.fail-fast: false`, matrix `os: [ubuntu-latest, macos-latest, windows-latest]`, `runs-on: ${{ matrix.os }}`, Rust stable, cache, `cargo build -p agentdir-cli`, and then invoke the relevant Python smoke script with the built binary. Use `shell: bash` for path selection so Windows uses Git Bash consistently.
  Must NOT do: Do not inline all smoke logic into YAML; keep reusable scripts as the source of truth. Do not use `cross` or Wine as the Windows signal.

  Parallelization: Can parallel: YES | Wave 3 | Blocks: [F1, F3] | Blocked by: [2, 3, 4]

  References (executor has NO interview context - be exhaustive):
  - Pattern:  `.github/workflows/ci.yml:19` - current Rust setup uses `dtolnay/rust-toolchain@stable`.
  - Pattern:  `.github/workflows/ci.yml:23` - current CI uses `Swatinem/rust-cache@v2`.
  - Pattern:  `scripts/ci/cli_smoke.py` - new reusable CLI smoke script from Task 3.
  - Pattern:  `scripts/ci/strategy_smoke.py` - new reusable strategy smoke script from Task 4.
  - External: `https://docs.github.com/en/actions/writing-workflows/workflow-syntax-for-github-actions#jobsjob_idneeds` - use `needs` to wait for Rust tests.
  - External: `https://docs.github.com/en/actions/writing-workflows/workflow-syntax-for-github-actions#jobsjob_idstepsshell` - explicitly set Bash for cross-platform path selection.

  Acceptance criteria (agent-executable only):
  - [ ] RED evidence exists: `python3 tests/test_issue_18_release_qa.py -k ci_has_cli_and_strategy_smoke_matrices` failed before editing `.github/workflows/ci.yml`, captured in `evidence/task-5-ci-smoke-matrix-red.txt`.
  - [ ] `python3 tests/test_issue_18_release_qa.py -k ci_has_cli_and_strategy_smoke_matrices` exits 0.
  - [ ] `.github/workflows/ci.yml` contains `CLI smoke (${{ matrix.os }})` or an equivalent job name and invokes `scripts/ci/cli_smoke.py --agentdir`.
  - [ ] `.github/workflows/ci.yml` contains `Strategy smoke (${{ matrix.os }})` or an equivalent job name and invokes `scripts/ci/strategy_smoke.py --agentdir`.
  - [ ] `python3 scripts/ci/cli_smoke.py --agentdir target/debug/agentdir` and `python3 scripts/ci/strategy_smoke.py --agentdir target/debug/agentdir` both pass locally after `cargo build -p agentdir-cli`.

  QA scenarios (MANDATORY - task incomplete without these):
  ```
  Scenario: CI smoke matrix structural check passes
    Tool:     tmux
    Steps:    tmux new-session -d -s issue18_t5 'cd /Users/jeffrey/Projects-dev/agentdir && mkdir -p evidence && python3 tests/test_issue_18_release_qa.py -k ci_has_cli_and_strategy_smoke_matrices > evidence/task-5-ci-smoke-matrix.txt 2>&1; printf "%s" "$?" > evidence/task-5-ci-smoke-matrix.exit'; while tmux has-session -t issue18_t5 2>/dev/null; do sleep 1; done; test "$(cat evidence/task-5-ci-smoke-matrix.exit)" = "0"; grep -q "PASS test_ci_has_cli_and_strategy_smoke_matrices" evidence/task-5-ci-smoke-matrix.txt
    Expected: command exits 0 and transcript contains the PASS line
    Evidence: evidence/task-5-ci-smoke-matrix.txt

  Scenario: local scripts still pass after workflow wiring
    Tool:     tmux
    Steps:    tmux new-session -d -s issue18_t5_local 'cd /Users/jeffrey/Projects-dev/agentdir && mkdir -p evidence && cargo build -p agentdir-cli >/tmp/issue18_t5_build.log 2>&1 && AGENTDIR_BIN=target/debug/agentdir; test -x "$AGENTDIR_BIN" || AGENTDIR_BIN=target/debug/agentdir.exe; python3 scripts/ci/cli_smoke.py --agentdir "$AGENTDIR_BIN" > evidence/task-5-cli-local.txt 2>&1 && python3 scripts/ci/strategy_smoke.py --agentdir "$AGENTDIR_BIN" > evidence/task-5-strategy-local.txt 2>&1; printf "%s" "$?" > evidence/task-5-local.exit'; while tmux has-session -t issue18_t5_local 2>/dev/null; do sleep 1; done; test "$(cat evidence/task-5-local.exit)" = "0"; grep -q "CLI smoke passed" evidence/task-5-cli-local.txt; grep -q "Strategy smoke passed" evidence/task-5-strategy-local.txt
    Expected: both smoke scripts pass locally with the built CLI
    Evidence: evidence/task-5-cli-local.txt and evidence/task-5-strategy-local.txt
  ```

  Commit: YES | Message: `ci(cli): run smoke matrices on native oses` | Files: [`.github/workflows/ci.yml`, `tests/issue_18_checks/ci_smoke_jobs.py`]

- [ ] 6. Add release metadata preflight and workflow gates

  What to do: First add `tests/issue_18_checks/release_preflight_artifact.py` with `test_release_preflight_checks_versions_and_packaging_commands`; capture RED. Then create `scripts/ci/release_preflight.py` and wire it into release workflows. The script must support `--metadata-only` and optional `--expected-version <version>`. Metadata checks must assert a single version across `crates/agentdir/Cargo.toml`, `crates/agentdir-cli/Cargo.toml`, `bindings/python/Cargo.toml`, `bindings/node/Cargo.toml`, `bindings/python/pyproject.toml`, `bindings/python/uv.lock`, `bindings/node/package.json`, `bindings/node/package-lock.json`, `Cargo.lock`, and the expected version strings embedded in `bindings/node/index.js`. Workflow gates must run metadata preflight before publish in Rust, Node, and Python release workflows; Rust must also add `cargo publish -p agentdir-cli --dry-run` after the sparse-index wait and before publishing `agentdir-cli`; Node must run `npm pack --dry-run` before `npm publish`.
  Must NOT do: Do not publish packages. Do not require package-lock or uv lock regeneration unless the preflight detects a mismatch. Do not validate README badge versions in this task.

  Parallelization: Can parallel: YES | Wave 2 | Blocks: [7] | Blocked by: [1]

  References (executor has NO interview context - be exhaustive):
  - Pattern:  `.github/workflows/release-rust.yml:21` - existing Rust release only dry-runs `agentdir`.
  - Pattern:  `.github/workflows/release-rust.yml:29` - existing workflow waits for the `agentdir` sparse index before publishing CLI.
  - Pattern:  `.github/workflows/release-rust.yml:49` - `agentdir-cli` publish currently has no preceding dry-run.
  - Pattern:  `.github/workflows/release-node.yml:83` - Node release already has native OS tests; keep them.
  - Pattern:  `.github/workflows/release-node.yml:124` - Node publish depends on build/test and is the right place for metadata/package preflight.
  - Pattern:  `.github/workflows/release-python.yml:65` - Python publish job currently downloads artifacts and publishes without metadata preflight.
  - API/Type: `crates/agentdir/Cargo.toml:3` - core crate version source.
  - API/Type: `crates/agentdir-cli/Cargo.toml:3` - CLI crate version source.
  - API/Type: `crates/agentdir-cli/Cargo.toml:15` - CLI dependency version on core crate.
  - API/Type: `bindings/python/pyproject.toml:7` - Python project version.
  - API/Type: `bindings/python/Cargo.toml:3` - Python Rust crate version.
  - API/Type: `bindings/node/Cargo.toml:3` - Node Rust crate version.
  - API/Type: `bindings/node/package.json:3` - Node package version.
  - API/Type: `bindings/node/package-lock.json:2` - Node lockfile package version.
  - API/Type: `bindings/node/index.js:80` - generated NAPI loader embeds expected package version strings.
  - API/Type: `Cargo.lock:5` - workspace lock entries include package versions for all local Rust crates.
  - API/Type: `bindings/python/uv.lock:9` - uv lockfile records the Python project version.
  - External: `https://doc.rust-lang.org/cargo/commands/cargo-publish.html` - `cargo publish --dry-run`.
  - External: `https://doc.rust-lang.org/cargo/commands/cargo-package.html` - package verification behavior reused by publish dry-runs.
  - External: `https://docs.npmjs.com/cli/v10/commands/npm-pack` - `npm pack --dry-run`.
  - External: `https://www.maturin.rs/distribution.html#build-wheels` - maturin build/sdist artifact output.

  Acceptance criteria (agent-executable only):
  - [ ] RED evidence exists: `python3 tests/test_issue_18_release_qa.py -k release_preflight_checks_versions_and_packaging_commands` failed before creating the script/workflow gates, captured in `evidence/task-6-release-preflight-red.txt`.
  - [ ] `python3 scripts/ci/release_preflight.py --metadata-only` exits 0 and prints `Release metadata preflight passed`.
  - [ ] `python3 scripts/ci/release_preflight.py --metadata-only --expected-version 0.0.0` exits nonzero and prints `version mismatch`.
  - [ ] `python3 tests/test_issue_18_release_qa.py -k release_preflight_checks_versions_and_packaging_commands` exits 0.
  - [ ] `.github/workflows/release-rust.yml` includes metadata preflight and `cargo publish -p agentdir-cli --dry-run` before the `agentdir-cli` publish step.
  - [ ] `.github/workflows/release-node.yml` includes metadata preflight and `npm pack --dry-run` before `npm publish`.
  - [ ] `.github/workflows/release-python.yml` includes metadata preflight before PyPI publish.

  QA scenarios (MANDATORY - task incomplete without these):
  ```
  Scenario: metadata preflight passes
    Tool:     tmux
    Steps:    tmux new-session -d -s issue18_t6 'cd /Users/jeffrey/Projects-dev/agentdir && mkdir -p evidence && python3 scripts/ci/release_preflight.py --metadata-only > evidence/task-6-release-preflight.txt 2>&1; printf "%s" "$?" > evidence/task-6-release-preflight.exit'; while tmux has-session -t issue18_t6 2>/dev/null; do sleep 1; done; test "$(cat evidence/task-6-release-preflight.exit)" = "0"; grep -q "Release metadata preflight passed" evidence/task-6-release-preflight.txt
    Expected: command exits 0 and transcript contains `Release metadata preflight passed`
    Evidence: evidence/task-6-release-preflight.txt

  Scenario: forced version mismatch fails
    Tool:     tmux
    Steps:    tmux new-session -d -s issue18_t6_error 'cd /Users/jeffrey/Projects-dev/agentdir && mkdir -p evidence && python3 scripts/ci/release_preflight.py --metadata-only --expected-version 0.0.0 > evidence/task-6-release-preflight-error.txt 2>&1; printf "%s" "$?" > evidence/task-6-release-preflight-error.exit'; while tmux has-session -t issue18_t6_error 2>/dev/null; do sleep 1; done; test "$(cat evidence/task-6-release-preflight-error.exit)" != "0"; grep -q "version mismatch" evidence/task-6-release-preflight-error.txt
    Expected: command exits nonzero and transcript contains `version mismatch`
    Evidence: evidence/task-6-release-preflight-error.txt
  ```

  Commit: YES | Message: `ci(release): add metadata preflight gates` | Files: [`scripts/ci/release_preflight.py`, `.github/workflows/release-rust.yml`, `.github/workflows/release-node.yml`, `.github/workflows/release-python.yml`, `tests/issue_18_checks/release_preflight_artifact.py`]

- [ ] 7. Add Python wheel post-build import smoke

  What to do: First add `tests/issue_18_checks/python_wheel_smoke_artifact.py` with `test_python_release_workflow_import_smokes_built_wheels_per_os`; capture RED. Then create `scripts/ci/python_wheel_smoke.py` and update `.github/workflows/release-python.yml`. The script must import `agentdir`, create temp source/workspace directories, call `Workspace.init`, `map`, `read_bytes`, `export_mapping`, mutate the source, call `refresh`, and print `Python wheel smoke passed`. The release workflow must add a `smoke` job after `build`, before `publish`, with matrix `os: [ubuntu-latest, macos-latest, windows-latest]`, download wheel artifacts, install a host-compatible wheel using `python -m pip install --no-index --find-links dist agentdir`, and run `python scripts/ci/python_wheel_smoke.py`. The smoke job must not try to import incompatible cross-arch wheels directly; `pip` should select the compatible wheel from `dist`.
  Must NOT do: Do not remove the existing aarch64 wheel builds. Do not smoke the sdist as a replacement for wheel smoke. Do not run a local editable install in the wheel smoke job.

  Parallelization: Can parallel: YES | Wave 3 | Blocks: [F1, F3] | Blocked by: [6]

  References (executor has NO interview context - be exhaustive):
  - Pattern:  `.github/workflows/release-python.yml:11` - current build job builds wheels across target matrix.
  - Pattern:  `.github/workflows/release-python.yml:36` - current workflow uses `PyO3/maturin-action@v1`.
  - Pattern:  `.github/workflows/release-python.yml:43` - current workflow uploads wheel artifacts.
  - Pattern:  `.github/workflows/release-python.yml:65` - publish job currently depends on build and sdist only; add smoke to `needs`.
  - API/Type: `bindings/python/python/agentdir/__init__.py:3` - public import surface exports `Workspace` and `SnapshotWorkspace`.
  - API/Type: `bindings/python/src/lib.rs:42` - `Workspace.init(path, strategy="reflink")` binding.
  - API/Type: `bindings/python/src/lib.rs:57` - `Workspace.map`.
  - API/Type: `bindings/python/src/lib.rs:134` - `Workspace.read_bytes`.
  - API/Type: `bindings/python/src/lib.rs:140` - `Workspace.refresh`.
  - API/Type: `bindings/python/src/lib.rs:168` - `Workspace.export_mapping`.
  - Test:     `bindings/python/tests/test_init_open.py:8` - existing binding init smoke.
  - Test:     `bindings/python/tests/test_export_mapping.py:6` - existing binding export mapping smoke.
  - External: `https://www.maturin.rs/distribution.html#github-actions` - maturin GitHub Actions wheel workflow pattern.
  - External: `https://www.maturin.rs/develop.html` - distinguishes editable/develop installs from built wheel validation.
  - External: `https://packaging.python.org/en/latest/tutorials/installing-packages/` - official `python -m pip install` guidance.
  - External: `https://docs.github.com/en/actions/using-workflows/storing-workflow-data-as-artifacts` - artifact upload/download between jobs.

  Acceptance criteria (agent-executable only):
  - [ ] RED evidence exists: `python3 tests/test_issue_18_release_qa.py -k python_release_workflow_import_smokes_built_wheels_per_os` failed before creating the script/workflow job, captured in `evidence/task-7-python-wheel-smoke-red.txt`.
  - [ ] `python3 tests/test_issue_18_release_qa.py -k python_release_workflow_import_smokes_built_wheels_per_os` exits 0.
  - [ ] `.github/workflows/release-python.yml` contains a `smoke` job with `runs-on: ${{ matrix.os }}`, `needs: build`, and matrix values for Ubuntu, macOS, and Windows.
  - [ ] The smoke job uses `actions/download-artifact` for wheel artifacts and installs with `python -m pip install --no-index --find-links dist agentdir`.
  - [ ] `cd bindings/python && uv run maturin develop && cd ../.. && python3 scripts/ci/python_wheel_smoke.py` exits 0 locally and prints `Python wheel smoke passed`.
  - [ ] `python3 -S scripts/ci/python_wheel_smoke.py` exits nonzero and prints `agentdir import failed` for the negative-path check.

  QA scenarios (MANDATORY - task incomplete without these):
  ```
  Scenario: local Python smoke passes against developed extension
    Tool:     tmux
    Steps:    tmux new-session -d -s issue18_t7 'cd /Users/jeffrey/Projects-dev/agentdir && mkdir -p evidence && cd bindings/python && uv run maturin develop >/tmp/issue18_t7_maturin.log 2>&1 && cd ../.. && python3 scripts/ci/python_wheel_smoke.py > evidence/task-7-python-wheel-smoke.txt 2>&1; printf "%s" "$?" > evidence/task-7-python-wheel-smoke.exit'; while tmux has-session -t issue18_t7 2>/dev/null; do sleep 1; done; test "$(cat evidence/task-7-python-wheel-smoke.exit)" = "0"; grep -q "Python wheel smoke passed" evidence/task-7-python-wheel-smoke.txt
    Expected: command exits 0 and transcript contains `Python wheel smoke passed`
    Evidence: evidence/task-7-python-wheel-smoke.txt

  Scenario: import failure is reported clearly
    Tool:     tmux
    Steps:    tmux new-session -d -s issue18_t7_error 'cd /Users/jeffrey/Projects-dev/agentdir && mkdir -p evidence && python3 -S scripts/ci/python_wheel_smoke.py > evidence/task-7-python-wheel-smoke-error.txt 2>&1; printf "%s" "$?" > evidence/task-7-python-wheel-smoke-error.exit'; while tmux has-session -t issue18_t7_error 2>/dev/null; do sleep 1; done; test "$(cat evidence/task-7-python-wheel-smoke-error.exit)" != "0"; grep -q "agentdir import failed" evidence/task-7-python-wheel-smoke-error.txt
    Expected: command exits nonzero and transcript contains `agentdir import failed`
    Evidence: evidence/task-7-python-wheel-smoke-error.txt
  ```

  Commit: YES | Message: `ci(python): smoke built wheels before publish` | Files: [`scripts/ci/python_wheel_smoke.py`, `.github/workflows/release-python.yml`, `tests/issue_18_checks/python_wheel_smoke_artifact.py`]

## Final verification wave (MANDATORY - after all implementation tasks)
> Runs in PARALLEL. ALL must APPROVE. Surface results to the caller and wait for an explicit "okay" before declaring complete.
- [ ] F1. Plan compliance audit - every task done, every acceptance criterion met
- [ ] F2. Code quality review - diagnostics clean, idioms match, no dead code
- [ ] F3. Real manual QA - every QA scenario executed with evidence captured
- [ ] F4. Scope fidelity - nothing extra shipped beyond Must-Have, nothing Must-NOT-Have introduced

## Commit strategy
- One logical change per commit. Conventional Commits (`<type>(<scope>): <subject>` body + footer).
- Atomic: every commit builds and passes tests on its own.
- No "WIP" / "fix typo squash later" commits on the final branch - clean up before merge.
- Reference the plan file path in the final commit footer: `Plan: plans/issue-18-release-qa.md`.

## Success criteria
- All Must-Have shipped; all QA scenarios pass with captured evidence; F1-F4 approved; commit history clean.
