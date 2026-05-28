import os
import tempfile

import pytest
from agentdir import Workspace


class TestInit:
    def test_init_creates_manifest(self, tmp_dir):
        Workspace.init(tmp_dir)
        assert os.path.exists(os.path.join(tmp_dir, ".agentdir", "manifest.json"))

    def test_init_returns_workspace(self, tmp_dir):
        ws = Workspace.init(tmp_dir)
        assert isinstance(ws, Workspace)

    def test_init_empty_status(self, workspace):
        s = workspace.status()
        assert s["total_entries"] == 0
        assert s["source_roots"] == 0

    def test_init_with_strategy_reflink(self, tmp_dir):
        ws = Workspace.init(tmp_dir, strategy="reflink")
        assert ws.status()["total_entries"] == 0

    def test_init_with_strategy_symlink(self, tmp_dir):
        ws = Workspace.init(tmp_dir, strategy="symlink")
        assert ws.status()["total_entries"] == 0

    def test_init_with_strategy_hardlink(self, tmp_dir):
        ws = Workspace.init(tmp_dir, strategy="hardlink")
        assert ws.status()["total_entries"] == 0

    def test_init_with_strategy_virtual(self, tmp_dir):
        ws = Workspace.init(tmp_dir, strategy="virtual")
        assert ws.status()["total_entries"] == 0

    def test_init_invalid_strategy(self, tmp_dir):
        with pytest.raises(ValueError, match="unknown strategy"):
            Workspace.init(tmp_dir, strategy="bogus")


class TestOpen:
    def test_open_existing(self, tmp_dir):
        Workspace.init(tmp_dir)
        ws = Workspace.open(tmp_dir)
        assert isinstance(ws, Workspace)

    def test_open_nonexistent(self):
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "nonexistent")
            with pytest.raises(IOError):
                Workspace.open(path)

    def test_open_preserves_entries(self, tmp_dir, source_dir):
        ws = Workspace.init(tmp_dir)
        ws.map(source_dir, "/docs")
        count = ws.status()["total_entries"]

        ws2 = Workspace.open(tmp_dir)
        assert ws2.status()["total_entries"] == count
