from __future__ import annotations

import argparse
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile
from pathlib import Path


def run(args: list[str], *, check: bool = True) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(args, text=True, capture_output=True, check=False)
    if check and result.returncode != 0:
        raise AssertionError(
            f"command failed ({result.returncode}): {' '.join(args)}\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    return result


def permissions_are_enforced() -> bool:
    if os.name != "posix":
        return True
    probe_dir = Path(tempfile.mkdtemp(prefix="agentdir-perm-probe-"))
    probe = probe_dir / "probe.txt"
    try:
        probe.write_text("x", encoding="utf-8")
        probe.chmod(0)
        return not probe.read_text(encoding="utf-8")
    except PermissionError:
        return True
    finally:
        try:
            probe.chmod(0o600)
            shutil.rmtree(probe_dir)
        except OSError:
            pass


def assert_materialized_readonly(path: Path) -> None:
    if not path.exists():
        raise AssertionError(f"materialized file does not exist: {path}")
    if os.name == "nt":
        if not path.stat().st_file_attributes & stat.FILE_ATTRIBUTE_READONLY:
            raise AssertionError(f"materialized file is not read-only: {path}")
        return
    mode = path.stat().st_mode
    if mode & 0o222:
        raise AssertionError(f"materialized file has writable bits: {oct(mode)}")
    if permissions_are_enforced():
        result = run(
            [
                sys.executable,
                "-c",
                "from pathlib import Path; Path(__import__('sys').argv[1]).write_text('bad')",
                str(path),
            ],
            check=False,
        )
        if result.returncode == 0:
            raise AssertionError(f"materialized file accepted a direct write: {path}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--agentdir", required=True)
    parser.add_argument("--keep-temp", action="store_true")
    args = parser.parse_args()
    agentdir = Path(args.agentdir)
    if not agentdir.exists():
        print(f"agentdir binary not found: {agentdir}", file=sys.stderr)
        return 1

    temp = Path(tempfile.mkdtemp(prefix="agentdir-cli-smoke-"))
    source = temp / "source"
    workspace = temp / "workspace"
    source.mkdir()
    (source / "nested").mkdir()
    (source / "root.txt").write_text("root v1", encoding="utf-8")
    (source / "nested" / "nested.txt").write_text("nested v1", encoding="utf-8")
    try:
        run([str(agentdir), "init", str(workspace)])
        run([str(agentdir), "-w", str(workspace), "map", str(source), "/docs"])
        stat_result = run([str(agentdir), "-w", str(workspace), "stat", "/docs/root.txt"])
        if (
            "Path: /docs/root.txt" not in stat_result.stdout
            or "Size: 7 bytes" not in stat_result.stdout
        ):
            raise AssertionError(f"unexpected stat output:\n{stat_result.stdout}")
        cat_result = run([str(agentdir), "-w", str(workspace), "cat", "/docs/root.txt"])
        if cat_result.stdout != "root v1":
            raise AssertionError(f"unexpected cat output: {cat_result.stdout!r}")
        assert_materialized_readonly(workspace / "docs" / "root.txt")

        (source / "root.txt").write_text("root v2 changed", encoding="utf-8")
        (source / "added.txt").write_text("added", encoding="utf-8")
        (source / "nested" / "nested.txt").unlink()
        refresh_result = run([str(agentdir), "-w", str(workspace), "refresh"])
        if "Synced:" not in refresh_result.stdout:
            raise AssertionError(f"unexpected refresh output:\n{refresh_result.stdout}")
        updated = run([str(agentdir), "-w", str(workspace), "cat", "/docs/root.txt"])
        if updated.stdout != "root v2 changed":
            raise AssertionError(f"refresh did not expose updated source: {updated.stdout!r}")
        run([str(agentdir), "-w", str(workspace), "stat", "/docs/added.txt"])
        deleted = run(
            [str(agentdir), "-w", str(workspace), "stat", "/docs/nested/nested.txt"], check=False
        )
        if deleted.returncode == 0:
            raise AssertionError("deleted source file still has a virtual stat entry")

        mapping = json.loads(run([str(agentdir), "-w", str(workspace), "export-mapping"]).stdout)
        if "/docs/root.txt" not in mapping.values():
            raise AssertionError("export-mapping omitted root.txt")
        if "/docs/added.txt" not in mapping.values():
            raise AssertionError("export-mapping omitted added.txt")
        print("CLI smoke passed")
        return 0
    finally:
        if args.keep_temp:
            print(f"keeping temp directory: {temp}")
        else:
            shutil.rmtree(temp, ignore_errors=True)


if __name__ == "__main__":
    raise SystemExit(main())
