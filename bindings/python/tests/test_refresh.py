import os
import time

from agentdir import Workspace


class TestRefresh:
    def test_refresh_returns_summary(self, mapped_workspace):
        ws, _ = mapped_workspace
        result = ws.refresh()
        assert isinstance(result, dict)
        for key in ("added", "refreshed", "removed", "errors"):
            assert key in result

    def test_refresh_detects_modification(self, tmp_dir, source_dir):
        ws = Workspace.init(tmp_dir)
        ws.map(source_dir, "/docs")

        assert ws.read_bytes("/docs/file1.txt") == b"hello"

        time.sleep(0.05)
        with open(os.path.join(source_dir, "file1.txt"), "wb") as f:
            f.write(b"updated")

        ws.refresh()
        assert ws.read_bytes("/docs/file1.txt") == b"updated"

    def test_refresh_detects_new_file(self, tmp_dir, source_dir):
        ws = Workspace.init(tmp_dir)
        ws.map(source_dir, "/docs")

        time.sleep(0.05)
        with open(os.path.join(source_dir, "new_file.txt"), "wb") as f:
            f.write(b"brand new")

        result = ws.refresh()
        assert result["added"] >= 1
        assert ws.exists("/docs/new_file.txt")
        assert ws.read_bytes("/docs/new_file.txt") == b"brand new"

    def test_refresh_detects_deletion(self, tmp_dir, source_dir):
        ws = Workspace.init(tmp_dir)
        ws.map(source_dir, "/docs")
        assert ws.exists("/docs/file2.txt")

        os.remove(os.path.join(source_dir, "file2.txt"))
        result = ws.refresh()
        assert result["removed"] >= 1
        assert not ws.exists("/docs/file2.txt")

    def test_refresh_no_changes(self, mapped_workspace):
        ws, _ = mapped_workspace
        result = ws.refresh()
        assert result["added"] == 0
        assert result["removed"] == 0
