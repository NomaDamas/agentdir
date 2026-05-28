import os
import tempfile

import pytest
from agentdir import Workspace


@pytest.fixture
def tmp_dir():
    with tempfile.TemporaryDirectory() as d:
        yield d


@pytest.fixture
def source_dir():
    with tempfile.TemporaryDirectory() as d:
        os.makedirs(os.path.join(d, "subdir"))
        with open(os.path.join(d, "file1.txt"), "wb") as f:
            f.write(b"hello")
        with open(os.path.join(d, "file2.txt"), "wb") as f:
            f.write(b"world")
        with open(os.path.join(d, "subdir", "nested.txt"), "wb") as f:
            f.write(b"nested content")
        yield d


@pytest.fixture
def workspace(tmp_dir):
    return Workspace.init(tmp_dir)


@pytest.fixture
def mapped_workspace(workspace, source_dir):
    workspace.map(source_dir, "/docs")
    return workspace, source_dir
