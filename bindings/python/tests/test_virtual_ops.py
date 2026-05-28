import pytest
from agentdir import Workspace


class TestMv:
    def test_mv_file(self, tmp_dir, mapped_workspace):
        ws, _ = mapped_workspace
        ws.mv("/docs/file1.txt", "/docs/moved.txt")
        assert not ws.exists("/docs/file1.txt")
        assert ws.exists("/docs/moved.txt")

    def test_mv_preserves_content(self, tmp_dir, source_dir):
        ws = Workspace.init(tmp_dir)
        ws.map(source_dir, "/docs")
        ws.mv("/docs/file1.txt", "/docs/moved.txt")
        data = ws.read_bytes("/docs/moved.txt")
        assert data == b"hello"

    def test_mv_nonexistent(self, mapped_workspace):
        ws, _ = mapped_workspace
        with pytest.raises(FileNotFoundError):
            ws.mv("/docs/nope.txt", "/docs/dest.txt")


class TestCp:
    def test_cp_file(self, mapped_workspace):
        ws, _ = mapped_workspace
        ws.cp("/docs/file1.txt", "/docs/copy.txt")
        assert ws.exists("/docs/file1.txt")
        assert ws.exists("/docs/copy.txt")

    def test_cp_preserves_content(self, mapped_workspace):
        ws, _ = mapped_workspace
        ws.cp("/docs/file1.txt", "/docs/copy.txt")
        assert ws.read_bytes("/docs/copy.txt") == b"hello"

    def test_cp_nonexistent(self, mapped_workspace):
        ws, _ = mapped_workspace
        with pytest.raises(FileNotFoundError):
            ws.cp("/docs/nope.txt", "/docs/copy.txt")


class TestMkdir:
    def test_mkdir(self, mapped_workspace):
        ws, _ = mapped_workspace
        ws.mkdir("/docs/newdir")
        assert ws.exists("/docs/newdir")

    def test_mkdir_nested(self, mapped_workspace):
        ws, _ = mapped_workspace
        ws.mkdir("/mydir")
        assert ws.exists("/mydir")


class TestRmdir:
    def test_rmdir_empty(self, mapped_workspace):
        ws, _ = mapped_workspace
        ws.mkdir("/docs/empty")
        ws.rmdir("/docs/empty", False)
        assert not ws.exists("/docs/empty")

    def test_rmdir_recursive(self, mapped_workspace):
        ws, _ = mapped_workspace
        ws.rmdir("/docs/subdir", True)
        assert not ws.exists("/docs/subdir")
        assert not ws.exists("/docs/subdir/nested.txt")

    def test_rmdir_nonempty_nonrecursive(self, mapped_workspace):
        ws, _ = mapped_workspace
        with pytest.raises(ValueError):
            ws.rmdir("/docs/subdir", False)


class TestRename:
    def test_rename_file(self, mapped_workspace):
        ws, _ = mapped_workspace
        ws.rename("/docs/file1.txt", "renamed.txt")
        assert not ws.exists("/docs/file1.txt")
        assert ws.exists("/docs/renamed.txt")

    def test_rename_preserves_content(self, mapped_workspace):
        ws, _ = mapped_workspace
        ws.rename("/docs/file1.txt", "renamed.txt")
        assert ws.read_bytes("/docs/renamed.txt") == b"hello"

    def test_rename_invalid_name(self, mapped_workspace):
        ws, _ = mapped_workspace
        with pytest.raises(ValueError):
            ws.rename("/docs/file1.txt", "bad/name")

    def test_rename_empty_name(self, mapped_workspace):
        ws, _ = mapped_workspace
        with pytest.raises(ValueError):
            ws.rename("/docs/file1.txt", "")
