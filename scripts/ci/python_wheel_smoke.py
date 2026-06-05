from __future__ import annotations

import subprocess
import sys


SMOKE_PROGRAM = "\n".join(
    [
        "import shutil",
        "import sys",
        "import tempfile",
        "import time",
        "from pathlib import Path",
        "",
        "try:",
        "    import agentdir",
        "    from agentdir import Workspace",
        "except ImportError as error:",
        "    print(f'agentdir import failed: {error}', file=sys.stderr)",
        "    raise SystemExit(1)",
        "",
        "temp = Path(tempfile.mkdtemp(prefix='agentdir-python-wheel-smoke-'))",
        "source = temp / 'source'",
        "workspace = temp / 'workspace'",
        "source.mkdir()",
        "source_file = source / 'file.txt'",
        "source_file.write_bytes(b'wheel smoke v1')",
        "try:",
        "    ws = Workspace.init(str(workspace))",
        "    if agentdir.Workspace is not Workspace:",
        "        raise AssertionError('agentdir.Workspace does not match exported Workspace')",
        "    mapped = ws.map(str(source), '/docs')",
        "    if mapped['entries_added'] < 1:",
        "        raise AssertionError(f'map added no entries: {mapped}')",
        "    if ws.read_bytes('/docs/file.txt') != b'wheel smoke v1':",
        "        raise AssertionError('read_bytes did not return initial mapped content')",
        "    mapping = ws.export_mapping(reverse=True)",
        "    if mapping.get('/docs/file.txt') != str(source_file):",
        "        raise AssertionError(f'export_mapping omitted mapped source: {mapping}')",
        "",
        "    time.sleep(0.05)",
        "    source_file.write_bytes(b'wheel smoke v2')",
        "    refreshed = ws.refresh()",
        "    if refreshed['refreshed'] < 1:",
        "        raise AssertionError(f'refresh did not report source update: {refreshed}')",
        "    if ws.read_bytes('/docs/file.txt') != b'wheel smoke v2':",
        "        raise AssertionError('refresh did not expose updated source content')",
        "",
        "    print('Python wheel smoke passed')",
        "finally:",
        "    shutil.rmtree(temp, ignore_errors=True)",
    ]
)


def main() -> int:
    return subprocess.run([sys.executable, "-c", SMOKE_PROGRAM], check=False).returncode


if __name__ == "__main__":
    raise SystemExit(main())
