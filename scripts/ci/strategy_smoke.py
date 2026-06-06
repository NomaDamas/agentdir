from __future__ import annotations

import argparse
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


def is_readonly(path: Path) -> bool:
    if os.name == "nt":
        return bool(path.stat().st_file_attributes & stat.FILE_ATTRIBUTE_READONLY)
    return not bool(path.stat().st_mode & 0o222)


def smoke_reflink(agentdir: Path, temp: Path) -> None:
    source = temp / "reflink-src"
    workspace = temp / "reflink-ws"
    source.mkdir()
    source_file = source / "file.txt"
    source_file.write_text("reflink content", encoding="utf-8")
    run([str(agentdir), "init", "--strategy", "reflink", str(workspace)])
    run([str(agentdir), "-w", str(workspace), "map", str(source), "/docs"])
    materialized = workspace / "docs" / "file.txt"
    if not materialized.exists() or materialized.is_symlink():
        raise AssertionError("reflink strategy did not create a physical non-symlink file")
    if not is_readonly(materialized):
        raise AssertionError("reflink materialized file is not read-only")
    source_file.write_text("source still writable", encoding="utf-8")
    print("reflink strategy passed")


def smoke_virtual(agentdir: Path, temp: Path) -> None:
    source = temp / "virtual-src"
    workspace = temp / "virtual-ws"
    source.mkdir()
    (source / "file.txt").write_text("virtual content", encoding="utf-8")
    run([str(agentdir), "init", "--strategy", "virtual", str(workspace)])
    run([str(agentdir), "-w", str(workspace), "map", str(source), "/docs"])
    materialized = workspace / "docs" / "file.txt"
    if materialized.exists() or materialized.is_symlink():
        raise AssertionError("virtual strategy created a materialized file")
    if run([str(agentdir), "-w", str(workspace), "cat", "/docs/file.txt"]).stdout != "virtual content":
        raise AssertionError("virtual strategy did not read from source")
    print("virtual strategy passed")


def smoke_symlink(agentdir: Path, temp: Path) -> None:
    source = temp / "symlink-src"
    workspace = temp / "symlink-ws"
    source.mkdir()
    source_file = source / "file.txt"
    source_file.write_text("symlink content", encoding="utf-8")
    run([str(agentdir), "init", "--strategy", "symlink", str(workspace)])
    run([str(agentdir), "-w", str(workspace), "map", str(source), "/docs"])
    materialized = workspace / "docs" / "file.txt"
    if not materialized.is_symlink():
        raise AssertionError("symlink strategy did not create a symlink")
    materialized.write_text("mutated through passthrough", encoding="utf-8")
    if source_file.read_text(encoding="utf-8") != "mutated through passthrough":
        raise AssertionError("symlink passthrough write did not modify source")
    print("symlink source mutation observed; passthrough strategy is unsafe for read-only use")
    print("symlink strategy passed")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--agentdir", required=True)
    parser.add_argument("--keep-temp", action="store_true")
    args = parser.parse_args()
    agentdir = Path(args.agentdir)
    if not agentdir.exists():
        print(f"agentdir binary not found: {agentdir}", file=sys.stderr)
        return 1
    temp = Path(tempfile.mkdtemp(prefix="agentdir-strategy-smoke-"))
    try:
        smoke_reflink(agentdir, temp)
        smoke_virtual(agentdir, temp)
        smoke_symlink(agentdir, temp)
        print("Strategy smoke passed")
        return 0
    finally:
        if args.keep_temp:
            print(f"keeping temp directory: {temp}")
        else:
            shutil.rmtree(temp, ignore_errors=True)


if __name__ == "__main__":
    raise SystemExit(main())
