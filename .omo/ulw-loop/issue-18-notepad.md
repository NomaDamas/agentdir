# ULW Notepad - Issue 18
Started: 2026-06-03
Goal: Resolve GitHub issue #18: cross-platform release QA coverage for Rust core/CLI, CLI smoke behavior, materialization strategies, release metadata, and Python wheel smoke.

## Skills Survey
- github:github - used to fetch and classify issue #18 from the current repository.
- omo:programming - used because changes will touch Rust, YAML workflows, and scripts/tests.
- omo:debugging - used for CLI/manual runtime evidence and artifact journaling discipline.
- omo:ulw-loop - used because the user explicitly requested `ulw`.
- openai-docs, browser, presentations, spreadsheets, Korean/local search, and document skills are not applicable to this repository CI/CLI task.

## Binding Success Criteria
1. `ci-os-matrix`: PR CI runs `cargo test --workspace` on `ubuntu-latest`, `macos-latest`, and `windows-latest`.
   - Automated test/check: `crates/agentdir/tests/issue_18_release_qa.py::test_ci_runs_rust_workspace_tests_on_linux_macos_windows`.
   - Manual QA channel: tmux.
   - Scenario: `tmux new-session -d -s ulw-qa-ci 'python3 -m pytest -q crates/agentdir/tests/issue_18_release_qa.py -k ci_runs_rust_workspace_tests_on_linux_macos_windows'`; PASS iff pytest exits 0 and transcript includes `1 passed`.
   - Evidence: `.omo/ulw-loop/evidence/issue-18-ci-os-matrix.txt`.
2. `cli-smoke`: a reusable cross-platform smoke script exercises `init`, `map`, `stat`, `cat`, source modify/add/delete + `refresh`, `export-mapping`, and default read-only materialization.
   - Automated test/check: `crates/agentdir/tests/issue_18_release_qa.py::test_cli_smoke_script_exercises_required_commands_and_readonly_check`.
   - Manual QA channel: tmux.
   - Scenario: `tmux new-session -d -s ulw-qa-cli 'python3 scripts/ci/cli_smoke.py --agentdir target/debug/agentdir --keep-temp'`; PASS iff transcript contains `CLI smoke passed`.
   - Evidence: `.omo/ulw-loop/evidence/issue-18-cli-smoke.txt`.
3. `strategy-smoke`: strategy smoke coverage includes `reflink`, `virtual`, and `symlink`, documenting symlink passthrough/unsafe writes.
   - Automated test/check: `crates/agentdir/tests/issue_18_release_qa.py::test_strategy_smoke_script_covers_reflink_virtual_and_symlink_contracts`.
   - Manual QA channel: tmux.
   - Scenario: `tmux new-session -d -s ulw-qa-strategy 'python3 scripts/ci/strategy_smoke.py --agentdir target/debug/agentdir --keep-temp'`; PASS iff transcript contains all three strategy pass lines and symlink source mutation.
   - Evidence: `.omo/ulw-loop/evidence/issue-18-strategy-smoke.txt`.
4. `release-preflight`: release metadata/version synchronization is checked before publish and documented.
   - Automated test/check: `crates/agentdir/tests/issue_18_release_qa.py::test_release_preflight_checks_versions_and_packaging_commands`.
   - Manual QA channel: tmux.
   - Scenario: `tmux new-session -d -s ulw-qa-release 'python3 scripts/ci/release_preflight.py --metadata-only'`; PASS iff transcript contains `Release metadata preflight passed`.
   - Evidence: `.omo/ulw-loop/evidence/issue-18-release-preflight.txt`.
5. `python-wheel-smoke`: Python wheel release matrix performs post-build import/smoke per OS where practical.
   - Automated test/check: `crates/agentdir/tests/issue_18_release_qa.py::test_python_release_workflow_import_smokes_built_wheels_per_os`.
   - Manual QA channel: tmux.
   - Scenario: `tmux new-session -d -s ulw-qa-python-release 'python3 -m pytest -q crates/agentdir/tests/issue_18_release_qa.py -k python_release_workflow_import_smokes_built_wheels_per_os'`; PASS iff pytest exits 0 and transcript includes `1 passed`.
   - Evidence: `.omo/ulw-loop/evidence/issue-18-python-release.txt`.

## Findings
- Issue #18: https://github.com/NomaDamas/agentdir/issues/18.
- Existing CLI already exposes `init`, `map`, `stat`, `cat`, `refresh`, `export-mapping`, and `--strategy`.
- Existing workflows are Ubuntu-only for main Rust CI; release Python builds wheels but has no post-build smoke job; release Node tests exist on native OSes.

## RED/GREEN Evidence
- RED 2026-06-03: `python3 -m pytest -q tests/test_issue_18_release_qa.py` exited 1 with 5 failures:
  - missing `os: [ubuntu-latest, macos-latest, windows-latest]` in `.github/workflows/ci.yml`
  - missing `scripts/ci/cli_smoke.py`
  - missing `scripts/ci/strategy_smoke.py`
  - missing `scripts/ci/release_preflight.py`
  - missing `smoke:` job in `.github/workflows/release-python.yml`
- GREEN 2026-06-03: `python3 -m pytest -q tests/test_issue_18_release_qa.py` exited 0 with `6 passed in 0.01s`.
- RESTORED GREEN 2026-06-03: `python3 -m pytest -q crates/agentdir/tests/issue_18_release_qa.py` exited 0 with `6 passed in 0.02s`.
- `cargo test --workspace` exited 0 after final restore.
- `cargo fmt --check` exited 0 after final restore.
- `cargo clippy --workspace -- -D warnings` exited 0 after final restore.
- `python3 -m py_compile scripts/ci/cli_smoke.py scripts/ci/strategy_smoke.py scripts/ci/release_preflight.py crates/agentdir/tests/issue_18_release_qa.py` exited 0.
- LSP diagnostics for `crates/agentdir/tests/issue_18_release_qa.py`: clean.

## Manual QA Evidence
- PASS `ci-os-matrix`: `.omo/ulw-loop/evidence/issue-18-ci-os-matrix.txt` contains `1 passed` and `EXIT:0`.
- PASS `cli-smoke`: `.omo/ulw-loop/evidence/issue-18-cli-smoke.txt` contains `CLI smoke passed` and `EXIT:0`.
- PASS `strategy-smoke`: `.omo/ulw-loop/evidence/issue-18-strategy-smoke.txt` contains `reflink strategy passed`, `virtual strategy passed`, `symlink source mutation observed; passthrough strategy is unsafe for read-only use`, `Strategy smoke passed`, and `EXIT:0`.
- PASS `release-preflight`: `.omo/ulw-loop/evidence/issue-18-release-preflight.txt` contains `Release metadata preflight passed: version 0.1.5` and `EXIT:0`.
- PASS `python-wheel-smoke`: `.omo/ulw-loop/evidence/issue-18-python-release.txt` contains `1 passed` and `EXIT:0`.

## Cleanup Receipts
- `tmux kill-session -t ulw-qa-ci`; verified no `ulw-qa-ci` session remains.
- `tmux kill-session -t ulw-qa-cli`; verified no `ulw-qa-cli` session remains.
- `tmux kill-session -t ulw-qa-strategy`; verified no `ulw-qa-strategy` session remains.
- `tmux kill-session -t ulw-qa-release`; verified no `ulw-qa-release` session remains.
- `tmux kill-session -t ulw-qa-python-release`; verified no `ulw-qa-python-release` session remains.
- `find /var/folders /tmp -maxdepth 2 -type d \( -name 'agentdir-cli-smoke-*' -o -name 'agentdir-strategy-smoke-*' -o -name 'agentdir-perm-probe-*' \)` returned no leftovers.
