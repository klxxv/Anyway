"""Pythonic reference contract for Research Canvas plugins.

This SDK is intentionally dependency-free and descriptive. The MVP does not
execute Python plugins; connectors exchange reviewed manifests and RunResult
artifacts through a file protocol.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Mapping, Protocol, runtime_checkable


@dataclass(frozen=True)
class PluginManifest:
    plugin_id: str
    name: str
    version: str
    capabilities: tuple[str, ...]
    permissions: tuple[str, ...] = ()


@runtime_checkable
class PluginContext(Protocol):
    """The only interface a plugin receives from the host."""

    @property
    def project_id(self) -> str: ...

    @property
    def locale(self) -> str: ...

    def has_capability(self, capability: str) -> bool: ...

    def propose_graph_patch(self, patch: Mapping[str, Any]) -> str: ...

    def notify(self, message: str) -> None: ...


class ResearchCanvasPlugin(Protocol):
    """One object with explicit setup and optional teardown."""

    manifest: PluginManifest

    def setup(
        self, context: PluginContext, config: Mapping[str, Any] | None = None
    ) -> None: ...

    def teardown(self) -> None: ...

