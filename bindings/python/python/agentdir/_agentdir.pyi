"""Type stubs for the native agentdir extension module."""

from __future__ import annotations

class Workspace:
    """Virtual filesystem workspace for agent-optimized file exploration."""

    @staticmethod
    def init(path: str, strategy: str = "reflink") -> Workspace:
        """Initialize a new workspace at the given path.

        Args:
            path: Root directory for the workspace.
            strategy: Materialization strategy — one of "reflink", "symlink",
                "hardlink", or "virtual".
        """
        ...

    @staticmethod
    def open(path: str) -> Workspace:
        """Open an existing workspace.

        Args:
            path: Root directory of an existing workspace.

        Raises:
            FileNotFoundError: If the workspace does not exist.
        """
        ...

    def map(self, source: str, mount: str) -> dict[str, int]:
        """Map a source directory to a virtual mount point.

        Returns:
            Dict with keys: entries_added, reflinked, copied, symlinked,
            hardlinked, dirs_created, errors.
        """
        ...

    def unmap(self, mount: str) -> dict[str, int]:
        """Remove a mapping at the given mount point.

        Returns:
            Dict with key: entries_removed.
        """
        ...

    def mv(self, from_path: str, to_path: str) -> None:
        """Move a virtual entry from one path to another."""
        ...

    def cp(self, from_path: str, to_path: str) -> None:
        """Copy a virtual entry from one path to another."""
        ...

    def mkdir(self, path: str) -> None:
        """Create a virtual directory."""
        ...

    def rmdir(self, path: str, recursive: bool) -> None:
        """Remove a virtual directory.

        Args:
            path: Virtual path to remove.
            recursive: If True, remove all children recursively.
        """
        ...

    def rename(self, path: str, new_name: str) -> None:
        """Rename a virtual entry (last path component only)."""
        ...

    def exists(self, path: str) -> bool:
        """Check whether a virtual path exists."""
        ...

    def stat(self, path: str) -> dict[str, object]:
        """Get metadata for a virtual path.

        Returns:
            Dict with keys: virtual_path, source_path, size_bytes, mtime_ns,
            entry_type, materialized.
        """
        ...

    def read_bytes(self, path: str) -> bytes:
        """Read the raw bytes of a file at the given virtual path."""
        ...

    def refresh(self) -> dict[str, int]:
        """Refresh the workspace by detecting source changes.

        Returns:
            Dict with keys: added, refreshed, removed, errors.
        """
        ...

    def status(self) -> dict[str, object]:
        """Get workspace status summary.

        Returns:
            Dict with keys: total_entries, source_roots, materialized_root,
            last_updated_epoch_secs.
        """
        ...

    def export_mapping(
        self,
        reverse: bool = False,
        relative_to: str | None = None,
    ) -> dict[str, str]:
        """Export the source-to-virtual (or reverse) path mapping.

        Args:
            reverse: If True, export virtual-to-source mapping.
            relative_to: Base path for relativizing source paths.
        """
        ...

    def map_batch(self, mappings: list[tuple[str, str]]) -> dict[str, object]:
        """Map multiple source directories in a single batch.

        Args:
            mappings: List of (source_path, mount_point) tuples.

        Returns:
            Dict with keys: entries_added, reflinked, copied, symlinked,
            hardlinked, dirs_created.
        """
        ...

    def rglob(self, pattern: str) -> list[str]:
        """Match virtual paths against a glob pattern.

        Args:
            pattern: Glob pattern (e.g. "/docs/*.txt", "/docs/**/*.md").

        Returns:
            List of matching virtual path strings.
        """
        ...

    def list_snapshots(self) -> list[str]:
        """List all snapshot names."""
        ...

    def destroy_snapshot(self, name: str) -> None:
        """Destroy a named snapshot."""
        ...
