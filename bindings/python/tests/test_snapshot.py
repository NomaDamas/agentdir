import pytest


class TestSnapshot:
    def test_list_snapshots_empty(self, workspace):
        snapshots = workspace.list_snapshots()
        assert isinstance(snapshots, list)
        assert len(snapshots) == 0

    def test_destroy_nonexistent_snapshot(self, workspace):
        with pytest.raises(FileNotFoundError):
            workspace.destroy_snapshot("nonexistent")
