import os

from agentdir import Workspace


class TestMap:
    def test_map_returns_summary(self, workspace, source_dir):
        result = workspace.map(source_dir, "/docs")
        assert isinstance(result, dict)
        assert result["entries_added"] > 0
        assert "reflinked" in result
        assert "copied" in result
        assert "dirs_created" in result
        assert "errors" in result

    def test_map_materializes_files(self, tmp_dir, source_dir):
        ws = Workspace.init(tmp_dir)
        ws.map(source_dir, "/docs")
        assert os.path.exists(os.path.join(tmp_dir, "docs", "file1.txt"))
        assert os.path.exists(os.path.join(tmp_dir, "docs", "file2.txt"))
        assert os.path.exists(os.path.join(tmp_dir, "docs", "subdir", "nested.txt"))

    def test_map_file_content(self, tmp_dir, source_dir):
        ws = Workspace.init(tmp_dir)
        ws.map(source_dir, "/docs")
        with open(os.path.join(tmp_dir, "docs", "file1.txt"), "rb") as f:
            assert f.read() == b"hello"

    def test_map_updates_status(self, workspace, source_dir):
        workspace.map(source_dir, "/docs")
        s = workspace.status()
        assert s["total_entries"] > 0
        assert s["source_roots"] == 1

    def test_map_canonical_paths(self, workspace, source_dir):
        workspace.map(source_dir, "/root-mount")
        assert workspace.exists("/root-mount/file1.txt")
        assert workspace.exists("/root-mount/subdir/nested.txt")


class TestUnmap:
    def test_unmap_returns_summary(self, mapped_workspace):
        ws, _ = mapped_workspace
        result = ws.unmap("/docs")
        assert isinstance(result, dict)
        assert result["entries_removed"] > 0

    def test_unmap_clears_entries(self, mapped_workspace):
        ws, _ = mapped_workspace
        ws.unmap("/docs")
        assert ws.status()["total_entries"] == 0

    def test_unmap_nonexistent_mount_removes_nothing(self, workspace):
        result = workspace.unmap("/nonexistent")
        assert result["entries_removed"] == 0
