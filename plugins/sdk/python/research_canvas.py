"""Pythonic reference contract for Research Canvas plugins.

This SDK is intentionally dependency-free and descriptive. The MVP does not
execute Python plugins; connectors exchange reviewed manifests and RunResult
artifacts through a file protocol.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Mapping, Protocol


GRAPH_PATCH_API_VERSION = "researchcanvas.dev/graph-patch/v1alpha1"


@dataclass(frozen=True)
class PluginManifest:
    plugin_id: str
    name: str
    version: str
    capabilities: tuple[str, ...]
    permissions: tuple[str, ...] = ()


@dataclass(frozen=True)
class GraphNodeProposal:
    """Reviewable semantic node / 可审阅的语义节点提案。"""

    node_id: str
    node_type: str
    title: str
    body: str = ""
    tags: tuple[str, ...] = ()
    data: Mapping[str, Any] | None = None

    def to_operation(self) -> Mapping[str, Any]:
        return {
            "op": "add-node",
            "node": {
                "id": self.node_id,
                "type": self.node_type,
                "title": self.title,
                "body": self.body,
                "tags": list(self.tags),
                "data": dict(self.data or {}),
            },
        }


@dataclass(frozen=True)
class GraphEdgeProposal:
    """Reviewable semantic relation / 可审阅的语义关系提案。"""

    edge_id: str
    source: str
    target: str
    edge_type: str
    note: str = ""

    def to_operation(self) -> Mapping[str, Any]:
        return {
            "op": "add-edge",
            "edge": {
                "id": self.edge_id,
                "source": self.source,
                "target": self.target,
                "type": self.edge_type,
                "note": self.note,
            },
        }


@dataclass(frozen=True)
class GraphPatch:
    """Portable host-reviewed patch / 由宿主审阅后应用的可移植图谱补丁。"""

    plugin_id: str
    operation: str
    title: str
    summary: str
    operations: tuple[Mapping[str, Any], ...]
    external_id: str | None = None

    def to_mapping(self) -> Mapping[str, Any]:
        source: dict[str, str] = {
            "pluginId": self.plugin_id,
            "operation": self.operation,
        }
        if self.external_id:
            source["externalId"] = self.external_id
        return {
            "apiVersion": GRAPH_PATCH_API_VERSION,
            "source": source,
            "title": self.title,
            "summary": self.summary,
            "reviewRequired": True,
            "operations": list(self.operations),
        }


class NetworkBlockExtractor(Protocol):
    """Torch/ONNX adapters implement this without importing application stores."""

    def extract(self, model: Any, *, external_id: str | None = None) -> GraphPatch: ...



