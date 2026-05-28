import os
import tempfile

from agentdir import Workspace


class TestSymlinkStrategy:
    def test_symlink_creates_links(self):
        with tempfile.TemporaryDirectory() as ws_dir, tempfile.TemporaryDirectory() as src:
            with open(os.path.join(src, "file.txt"), "wb") as f:
                f.write(b"symlinked")

            ws = Workspace.init(ws_dir, strategy="symlink")
            ws.map(src, "/sym")
            mat_path = os.path.join(ws_dir, "sym", "file.txt")
            assert os.path.exists(mat_path)
            assert os.path.islink(mat_path)

    def test_symlink_content_readable(self):
        with tempfile.TemporaryDirectory() as ws_dir, tempfile.TemporaryDirectory() as src:
            with open(os.path.join(src, "file.txt"), "wb") as f:
                f.write(b"readable")

            ws = Workspace.init(ws_dir, strategy="symlink")
            ws.map(src, "/sym")
            assert ws.read_bytes("/sym/file.txt") == b"readable"


class TestHardlinkStrategy:
    def test_hardlink_creates_file(self):
        with tempfile.TemporaryDirectory() as ws_dir, tempfile.TemporaryDirectory() as src:
            with open(os.path.join(src, "file.txt"), "wb") as f:
                f.write(b"hardlinked")

            ws = Workspace.init(ws_dir, strategy="hardlink")
            ws.map(src, "/hard")
            mat_path = os.path.join(ws_dir, "hard", "file.txt")
            assert os.path.exists(mat_path)
            assert not os.path.islink(mat_path)

    def test_hardlink_content_readable(self):
        with tempfile.TemporaryDirectory() as ws_dir, tempfile.TemporaryDirectory() as src:
            with open(os.path.join(src, "file.txt"), "wb") as f:
                f.write(b"harddata")

            ws = Workspace.init(ws_dir, strategy="hardlink")
            ws.map(src, "/hard")
            assert ws.read_bytes("/hard/file.txt") == b"harddata"


class TestVirtualStrategy:
    def test_virtual_no_files_on_disk(self):
        with tempfile.TemporaryDirectory() as ws_dir, tempfile.TemporaryDirectory() as src:
            with open(os.path.join(src, "file.txt"), "wb") as f:
                f.write(b"virtual")

            ws = Workspace.init(ws_dir, strategy="virtual")
            ws.map(src, "/virt")
            mat_path = os.path.join(ws_dir, "virt", "file.txt")
            assert not os.path.exists(mat_path)

    def test_virtual_read_bytes_works(self):
        with tempfile.TemporaryDirectory() as ws_dir, tempfile.TemporaryDirectory() as src:
            with open(os.path.join(src, "file.txt"), "wb") as f:
                f.write(b"virtual content")

            ws = Workspace.init(ws_dir, strategy="virtual")
            ws.map(src, "/virt")
            assert ws.read_bytes("/virt/file.txt") == b"virtual content"

    def test_virtual_exists(self):
        with tempfile.TemporaryDirectory() as ws_dir, tempfile.TemporaryDirectory() as src:
            with open(os.path.join(src, "file.txt"), "wb") as f:
                f.write(b"exists")

            ws = Workspace.init(ws_dir, strategy="virtual")
            ws.map(src, "/virt")
            assert ws.exists("/virt/file.txt")
