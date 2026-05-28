import pytest


class TestExists:
    def test_exists_true(self, mapped_workspace):
        ws, _ = mapped_workspace
        assert ws.exists("/docs/file1.txt") is True

    def test_exists_false(self, mapped_workspace):
        ws, _ = mapped_workspace
        assert ws.exists("/docs/nonexistent.txt") is False

    def test_exists_directory(self, mapped_workspace):
        ws, _ = mapped_workspace
        assert ws.exists("/docs/subdir") is True


class TestStat:
    def test_stat_file(self, mapped_workspace):
        ws, _ = mapped_workspace
        s = ws.stat("/docs/file1.txt")
        assert isinstance(s, dict)
        assert s["virtual_path"] == "/docs/file1.txt"
        assert s["size_bytes"] == 5
        assert "mtime_ns" in s
        assert "entry_type" in s
        assert "materialized" in s

    def test_stat_source_path(self, mapped_workspace):
        ws, _src = mapped_workspace
        s = ws.stat("/docs/file1.txt")
        assert isinstance(s["source_path"], str)
        assert len(s["source_path"]) > 0

    def test_stat_nonexistent(self, mapped_workspace):
        ws, _ = mapped_workspace
        with pytest.raises(FileNotFoundError):
            ws.stat("/docs/nope.txt")


class TestReadBytes:
    def test_read_bytes_content(self, mapped_workspace):
        ws, _ = mapped_workspace
        data = ws.read_bytes("/docs/file1.txt")
        assert data == b"hello"

    def test_read_bytes_nested(self, mapped_workspace):
        ws, _ = mapped_workspace
        data = ws.read_bytes("/docs/subdir/nested.txt")
        assert data == b"nested content"

    def test_read_bytes_nonexistent(self, mapped_workspace):
        ws, _ = mapped_workspace
        with pytest.raises(FileNotFoundError):
            ws.read_bytes("/docs/nope.txt")

    def test_read_bytes_returns_bytes(self, mapped_workspace):
        ws, _ = mapped_workspace
        data = ws.read_bytes("/docs/file1.txt")
        assert isinstance(data, bytes)


class TestRglob:
    def test_rglob_matches_txt(self, mapped_workspace):
        ws, _ = mapped_workspace
        results = ws.rglob("/docs/*.txt")
        assert isinstance(results, list)
        assert "/docs/file1.txt" in results
        assert "/docs/file2.txt" in results

    def test_rglob_recursive(self, mapped_workspace):
        ws, _ = mapped_workspace
        results = ws.rglob("/docs/**/*.txt")
        assert "/docs/subdir/nested.txt" in results

    def test_rglob_no_match(self, mapped_workspace):
        ws, _ = mapped_workspace
        results = ws.rglob("/docs/*.xyz")
        assert results == []

    def test_rglob_invalid_pattern(self, mapped_workspace):
        ws, _ = mapped_workspace
        with pytest.raises(ValueError):
            ws.rglob("[invalid")
