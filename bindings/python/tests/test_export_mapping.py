import os

from agentdir import Workspace


class TestExportMapping:
    def test_forward_mapping(self, mapped_workspace):
        ws, _src = mapped_workspace
        m = ws.export_mapping()
        assert isinstance(m, dict)
        assert len(m) > 0

    def test_reverse_mapping(self, mapped_workspace):
        ws, _ = mapped_workspace
        m = ws.export_mapping(reverse=True)
        assert isinstance(m, dict)
        assert len(m) > 0

    def test_forward_values_are_virtual(self, mapped_workspace):
        ws, _ = mapped_workspace
        m = ws.export_mapping()
        for v in m.values():
            assert v.startswith("/")

    def test_reverse_values_are_source(self, mapped_workspace):
        ws, _src = mapped_workspace
        m = ws.export_mapping(reverse=True)
        for v in m.values():
            assert os.path.isabs(v)

    def test_relative_to(self, tmp_dir, source_dir):
        ws = Workspace.init(tmp_dir)
        canonical_src = os.path.realpath(source_dir)
        ws.map(canonical_src, "/docs")
        m = ws.export_mapping(relative_to=canonical_src)
        assert isinstance(m, dict)
        for k in m:
            assert not os.path.isabs(k)

    def test_empty_workspace(self, workspace):
        m = workspace.export_mapping()
        assert m == {}
