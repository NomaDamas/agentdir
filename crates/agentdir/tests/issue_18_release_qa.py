from __future__ import annotations

import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def test_ci_runs_rust_workspace_tests_on_linux_macos_windows() -> None:
    workflow = read(".github/workflows/ci.yml")
    assert "os: [ubuntu-latest, macos-latest, windows-latest]" in workflow
    assert re.search(r"runs-on:\s*\$\{\{\s*matrix\.os\s*\}\}", workflow)
    assert "cargo test --workspace" in workflow
    assert "CLI smoke (${{ matrix.os }})" in workflow
    assert "Strategy smoke (${{ matrix.os }})" in workflow


def test_cli_smoke_script_exercises_required_commands_and_readonly_check() -> None:
    script = read("scripts/ci/cli_smoke.py")
    for command in ("init", "map", "stat", "cat", "refresh", "export-mapping"):
        assert f'"{command}"' in script
    assert "assert_materialized_readonly" in script
    assert "CLI smoke passed" in script
    assert "agentdir binary not found" in script
    assert "scripts/ci/cli_smoke.py --agentdir" in read(".github/workflows/ci.yml")


def test_strategy_smoke_script_covers_reflink_virtual_and_symlink_contracts() -> None:
    script = read("scripts/ci/strategy_smoke.py")
    for strategy in ("reflink", "virtual", "symlink"):
        assert f'"{strategy}"' in script
    assert "symlink source mutation observed" in script
    assert "passthrough" in script
    assert "Strategy smoke passed" in script
    assert "scripts/ci/strategy_smoke.py --agentdir" in read(".github/workflows/ci.yml")


def test_release_preflight_checks_versions_and_packaging_commands() -> None:
    script = read("scripts/ci/release_preflight.py")
    for path in (
        "crates/agentdir/Cargo.toml",
        "crates/agentdir-cli/Cargo.toml",
        "bindings/python/pyproject.toml",
        "bindings/node/package.json",
        "bindings/node/package-lock.json",
        "bindings/node/index.js",
        "bindings/python/uv.lock",
    ):
        assert path in script
    for command in (
        "cargo publish -p agentdir --dry-run",
        "cargo publish -p agentdir-cli --dry-run",
        "npm pack --dry-run",
        "maturin sdist",
    ):
        assert command in script
    assert "Release metadata preflight passed" in script
    assert "scripts/ci/release_preflight.py --metadata-only" in read(".github/workflows/release-rust.yml")
    assert "../../scripts/ci/release_preflight.py --metadata-only" in read(".github/workflows/release-node.yml")
    assert "../../scripts/ci/release_preflight.py --metadata-only" in read(".github/workflows/release-python.yml")


def test_python_release_workflow_import_smokes_built_wheels_per_os() -> None:
    workflow = read(".github/workflows/release-python.yml")
    assert "smoke:" in workflow
    assert "needs: build" in workflow
    assert "runs-on: ${{ matrix.os }}" in workflow
    assert "pip install dist/*.whl" in workflow
    assert "import agentdir" in workflow
    assert "Workspace.init" in workflow
    assert "target: aarch64" in workflow


def test_node_package_lock_root_version_matches_package_json() -> None:
    package = json.loads(read("bindings/node/package.json"))
    lock = json.loads(read("bindings/node/package-lock.json"))
    assert lock["version"] == package["version"]
    assert lock["packages"][""]["version"] == package["version"]
