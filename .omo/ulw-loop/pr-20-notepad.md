# ULW Notepad - PR 20 Readiness

Goal: strengthen draft PR #20 cross-platform QA coverage so release QA is defensible and enforced.

## Skills Survey
- github:github - used for PR #20 metadata and check status.
- github:gh-fix-ci - consulted because PR readiness involved GitHub Actions checks.
- omo:programming - used for Python scripts/tests and workflow-adjacent code.
- omo:review-work - used for post-implementation review.

## Binding Success Criteria
1. Python release wheel smoke installs a host-compatible wheel with pip resolver and runs a reusable script that maps, reads, exports, refreshes, and reports import failures.
2. CLI and strategy smoke scripts support `--keep-temp` for manual QA artifacts while preserving default cleanup.
3. Release preflight supports `--expected-version` and fails clearly on version mismatch.
4. CI enforces the release-QA artifact checks through a zero-dependency self-running script.
5. All changed files have clean diagnostics, automated tests pass, manual tmux QA passes, and retained QA state is cleaned up.

## RED Evidence
- `.omo/ulw-loop/evidence/pr-20-readiness-red.txt`: four intended failures for missing `python_wheel_smoke.py`, incompatible direct wheel install, missing `--keep-temp`, and missing `--expected-version`.
- `.omo/ulw-loop/evidence/pr-20-ci-enforcement-red.txt`: CI did not run the release-QA artifact checks.
- `.omo/ulw-loop/evidence/pr-20-python-wheel-entrypoint-red.txt`: reusable Python wheel smoke script lacked executable entrypoint.

## GREEN Evidence
- `python3 -m pytest -q crates/agentdir/tests/issue_18_release_qa.py`: `10 passed`.
- `python3 crates/agentdir/tests/issue_18_release_qa.py`: all 10 checks printed `PASS`.
- `python3 -m py_compile scripts/ci/cli_smoke.py scripts/ci/strategy_smoke.py scripts/ci/release_preflight.py scripts/ci/python_wheel_smoke.py crates/agentdir/tests/issue_18_release_qa.py`: passed.
- `cargo fmt --check`: passed.
- `cargo clippy --workspace -- -D warnings`: passed.
- `cargo test --workspace`: passed.
- LSP diagnostics: clean for changed Python files.

## Manual QA Evidence
- `.omo/ulw-loop/evidence/pr-20-release-qa-self-runner.txt`: self-runner exits 0 and prints release-QA PASS lines.
- `.omo/ulw-loop/evidence/pr-20-cli-keep-temp.txt`: CLI smoke exits 0, prints `CLI smoke passed`, and prints retained temp directory.
- `.omo/ulw-loop/evidence/pr-20-strategy-keep-temp.txt`: strategy smoke exits 0, prints all strategy pass markers, symlink passthrough warning, and retained temp directory.
- `.omo/ulw-loop/evidence/pr-20-release-preflight-version-mismatch.txt`: mismatched expected version exits 1 and prints `version mismatch`.
- `.omo/ulw-loop/evidence/pr-20-release-preflight-version-ok.txt`: expected version `0.1.5` exits 0.
- `.omo/ulw-loop/evidence/pr-20-python-wheel-smoke.txt`: developed Python binding interpreter runs `scripts/ci/python_wheel_smoke.py`, exits 0, and prints `Python wheel smoke passed`.
- `.omo/ulw-loop/evidence/pr-20-python-wheel-import-negative.txt`: `python3 -S scripts/ci/python_wheel_smoke.py` exits 1 and prints `agentdir import failed`.

## Cleanup
- `.omo/ulw-loop/evidence/pr-20-cleanup.txt`: retained CLI/strategy temp directories removed and no `ulw-qa-pr20` tmux sessions remain.

## Review
- Delegated reviewer attempt `reviewer`: inconclusive after two waits plus targeted follow-up; agent closed while still running.
- Delegated reviewer attempt `reviewer_small`: inconclusive after two waits plus targeted follow-up; agent closed while still running.
- Root-side blocking review: no blocking issue found in the scoped CI/release QA diff after diagnostics, automated verification, manual tmux QA, and cleanup checks.
- Delegated reviewer attempt `review_quick`: PASS. Reviewer found no blocking issues and accepted the CI-enforced release QA, resolver-based wheel install, reusable wheel smoke, keep-temp behavior, expected-version mismatch check, and captured verification.
