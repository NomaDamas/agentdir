import pytest
from agentdir import Workspace


class TestPythonExceptions:
    def test_stat_not_found_raises_file_not_found(self, mapped_workspace):
        ws, _ = mapped_workspace
        with pytest.raises(FileNotFoundError):
            ws.stat("/docs/nonexistent.txt")

    def test_invalid_strategy_raises_value_error(self, tmp_dir):
        with pytest.raises(ValueError):
            Workspace.init(tmp_dir, strategy="invalid")

    def test_rename_slash_raises_value_error(self, mapped_workspace):
        ws, _ = mapped_workspace
        with pytest.raises(ValueError):
            ws.rename("/docs/file1.txt", "a/b")

    def test_rename_empty_raises_value_error(self, mapped_workspace):
        ws, _ = mapped_workspace
        with pytest.raises(ValueError):
            ws.rename("/docs/file1.txt", "")

    def test_mv_nonexistent_raises(self, mapped_workspace):
        ws, _ = mapped_workspace
        with pytest.raises(FileNotFoundError):
            ws.mv("/docs/ghost.txt", "/docs/dest.txt")

    def test_cp_nonexistent_raises(self, mapped_workspace):
        ws, _ = mapped_workspace
        with pytest.raises(FileNotFoundError):
            ws.cp("/docs/ghost.txt", "/docs/dest.txt")

    def test_rmdir_nonempty_nonrecursive_raises(self, mapped_workspace):
        ws, _ = mapped_workspace
        with pytest.raises(ValueError):
            ws.rmdir("/docs/subdir", False)

    def test_unmap_nonexistent_returns_zero(self, workspace):
        result = workspace.unmap("/nonexistent")
        assert result["entries_removed"] == 0
