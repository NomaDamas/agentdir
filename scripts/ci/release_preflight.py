from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


def load_toml(path: str) -> dict[str, object]:
    with (ROOT / path).open("rb") as handle:
        return tomllib.load(handle)


def package_version(path: str) -> str:
    package = load_toml(path)["package"]
    if not isinstance(package, dict) or not isinstance(package.get("version"), str):
        raise AssertionError(f"missing package.version in {path}")
    return package["version"]


def project_version(path: str) -> str:
    project = load_toml(path)["project"]
    if not isinstance(project, dict) or not isinstance(project.get("version"), str):
        raise AssertionError(f"missing project.version in {path}")
    return project["version"]


def assert_all_versions_match() -> str:
    versions = {
        "crates/agentdir/Cargo.toml": package_version("crates/agentdir/Cargo.toml"),
        "crates/agentdir-cli/Cargo.toml": package_version("crates/agentdir-cli/Cargo.toml"),
        "bindings/python/Cargo.toml": package_version("bindings/python/Cargo.toml"),
        "bindings/node/Cargo.toml": package_version("bindings/node/Cargo.toml"),
        "bindings/python/pyproject.toml": project_version("bindings/python/pyproject.toml"),
    }
    node_package = json.loads((ROOT / "bindings/node/package.json").read_text(encoding="utf-8"))
    node_lock = json.loads((ROOT / "bindings/node/package-lock.json").read_text(encoding="utf-8"))
    versions["bindings/node/package.json"] = str(node_package["version"])
    versions["bindings/node/package-lock.json"] = str(node_lock["version"])
    versions["bindings/node/package-lock.json packages[\"\"]"] = str(node_lock["packages"][""]["version"])
    uv_lock = (ROOT / "bindings/python/uv.lock").read_text(encoding="utf-8")
    uv_match = re.search(r'name = "agentdir"\nversion = "([^"]+)"', uv_lock)
    if uv_match is None:
        raise AssertionError("bindings/python/uv.lock does not contain agentdir version")
    versions["bindings/python/uv.lock"] = uv_match.group(1)
    loader = (ROOT / "bindings/node/index.js").read_text(encoding="utf-8")
    loader_versions = set(re.findall(r"expected ([0-9]+\.[0-9]+\.[0-9]+) but got", loader))
    if len(loader_versions) != 1:
        raise AssertionError(f"bindings/node/index.js has inconsistent generated versions: {loader_versions}")
    versions["bindings/node/index.js"] = next(iter(loader_versions))
    if len(set(versions.values())) != 1:
        details = "\n".join(f"{path}: {version}" for path, version in sorted(versions.items()))
        raise AssertionError(f"release versions are not synchronized:\n{details}")
    return next(iter(versions.values()))


def run_packaging_checks() -> None:
    commands = [
        "cargo publish -p agentdir --dry-run",
        "cargo publish -p agentdir-cli --dry-run",
        "npm pack --dry-run",
        "maturin sdist",
    ]
    for command in commands:
        cwd = ROOT
        if command.startswith("npm "):
            cwd = ROOT / "bindings/node"
        if command.startswith("maturin "):
            cwd = ROOT / "bindings/python"
        result = subprocess.run(command.split(), cwd=cwd, text=True, check=False)
        if result.returncode != 0:
            raise AssertionError(f"packaging command failed: {command}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--metadata-only", action="store_true")
    parser.add_argument("--expected-version")
    args = parser.parse_args()
    version = assert_all_versions_match()
    if args.expected_version is not None and version != args.expected_version:
        raise AssertionError(f"version mismatch: expected {args.expected_version}, found {version}")
    if not args.metadata_only:
        run_packaging_checks()
    print(f"Release metadata preflight passed: version {version}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except AssertionError as error:
        print(f"Release metadata preflight failed: {error}", file=sys.stderr)
        raise SystemExit(1)
