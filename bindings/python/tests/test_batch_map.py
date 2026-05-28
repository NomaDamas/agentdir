import os
import tempfile

import pytest


class TestBatchMap:
    def test_single_file(self, workspace, source_dir):
        file_path = os.path.join(source_dir, "file1.txt")
        result = workspace.map_batch([(file_path, "/batch/file1.txt")])
        assert isinstance(result, dict)
        assert result["entries_added"] == 1
        assert workspace.exists("/batch/file1.txt")

    def test_multiple_files(self, workspace):
        with tempfile.TemporaryDirectory() as src:
            path_a = os.path.join(src, "a.txt")
            path_b = os.path.join(src, "b.txt")
            with open(path_a, "wb") as f:
                f.write(b"aaa")
            with open(path_b, "wb") as f:
                f.write(b"bbb")

            result = workspace.map_batch([(path_a, "/m/a.txt"), (path_b, "/m/b.txt")])
            assert result["entries_added"] == 2
            assert workspace.exists("/m/a.txt")
            assert workspace.exists("/m/b.txt")

    def test_batch_content(self, workspace):
        with tempfile.TemporaryDirectory() as src:
            path = os.path.join(src, "data.bin")
            with open(path, "wb") as f:
                f.write(b"\x00\x01\x02\x03")
            workspace.map_batch([(path, "/bin/data.bin")])
            assert workspace.read_bytes("/bin/data.bin") == b"\x00\x01\x02\x03"

    def test_batch_empty_list(self, workspace):
        result = workspace.map_batch([])
        assert result["entries_added"] == 0

    def test_batch_summary_keys(self, workspace, source_dir):
        file_path = os.path.join(source_dir, "file1.txt")
        result = workspace.map_batch([(file_path, "/x/file1.txt")])
        for key in (
            "entries_added",
            "reflinked",
            "copied",
            "symlinked",
            "hardlinked",
            "dirs_created",
        ):
            assert key in result

    def test_batch_rejects_directory(self, workspace, source_dir):
        with pytest.raises(ValueError, match="batch map only accepts files"):
            workspace.map_batch([(source_dir, "/shouldfail")])
