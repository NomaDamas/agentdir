import pytest
from agentdir import SnapshotWorkspace


class TestSnapshot:
    def test_list_snapshots_empty(self, workspace):
        snapshots = workspace.list_snapshots()
        assert isinstance(snapshots, list)
        assert len(snapshots) == 0

    def test_destroy_nonexistent_snapshot(self, workspace):
        with pytest.raises(FileNotFoundError):
            workspace.destroy_snapshot("nonexistent")

    def test_create_snapshot(self, mapped_workspace):
        ws, _ = mapped_workspace
        snap = ws.snapshot("snap1")
        assert isinstance(snap, SnapshotWorkspace)

    def test_snapshot_appears_in_list(self, mapped_workspace):
        ws, _ = mapped_workspace
        ws.snapshot("snap1")
        assert "snap1" in ws.list_snapshots()

    def test_snapshot_read_files(self, mapped_workspace):
        ws, _ = mapped_workspace
        snap = ws.snapshot("snap1")
        assert snap.exists("/docs/file1.txt")
        assert snap.read_bytes("/docs/file1.txt") == b"hello"

    def test_snapshot_stat(self, mapped_workspace):
        ws, _ = mapped_workspace
        snap = ws.snapshot("snap1")
        s = snap.stat("/docs/file1.txt")
        assert s["virtual_path"] == "/docs/file1.txt"
        assert s["size_bytes"] == 5
        assert s["entry_type"] == "File"
        assert isinstance(s["materialized"], bool)

    def test_snapshot_write_isolates(self, mapped_workspace):
        ws, _ = mapped_workspace
        snap = ws.snapshot("snap1")
        snap.write("/docs/file1.txt", b"snapshot-only")
        assert snap.read_bytes("/docs/file1.txt") == b"snapshot-only"
        assert ws.read_bytes("/docs/file1.txt") == b"hello"

    def test_snapshot_write_new_file(self, mapped_workspace):
        ws, _ = mapped_workspace
        snap = ws.snapshot("snap1")
        snap.write("/docs/new.txt", b"new content")
        assert snap.read_bytes("/docs/new.txt") == b"new content"
        assert not ws.exists("/docs/new.txt")

    def test_snapshot_export_mapping(self, mapped_workspace):
        ws, _ = mapped_workspace
        snap = ws.snapshot("snap1")
        mapping = snap.export_mapping()
        assert isinstance(mapping, dict)
        assert len(mapping) > 0

    def test_snapshot_export_mapping_reverse(self, mapped_workspace):
        ws, _ = mapped_workspace
        snap = ws.snapshot("snap1")
        mapping = snap.export_mapping(reverse=True)
        assert "/docs/file1.txt" in mapping

    def test_duplicate_snapshot_throws(self, mapped_workspace):
        ws, _ = mapped_workspace
        ws.snapshot("snap1")
        with pytest.raises(ValueError):
            ws.snapshot("snap1")

    def test_open_snapshot(self, mapped_workspace):
        ws, _ = mapped_workspace
        ws.snapshot("snap1")
        opened = ws.open_snapshot("snap1")
        assert isinstance(opened, SnapshotWorkspace)
        assert opened.read_bytes("/docs/file1.txt") == b"hello"

    def test_open_nonexistent_snapshot(self, mapped_workspace):
        ws, _ = mapped_workspace
        with pytest.raises(FileNotFoundError):
            ws.open_snapshot("nonexistent")

    def test_snapshot_destroy(self, mapped_workspace):
        ws, _ = mapped_workspace
        snap = ws.snapshot("snap1")
        assert "snap1" in ws.list_snapshots()
        snap.destroy()
        assert "snap1" not in ws.list_snapshots()

    def test_destroy_snapshot_by_name(self, mapped_workspace):
        ws, _ = mapped_workspace
        ws.snapshot("snap1")
        ws.destroy_snapshot("snap1")
        assert "snap1" not in ws.list_snapshots()
