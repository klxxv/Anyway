"""Pythonic reference contract for Research Canvas plugins.

This SDK is intentionally dependency-free and descriptive. The MVP does not
execute Python plugins; connectors exchange reviewed manifests and RunResult
artifacts through a file protocol.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Literal, Mapping, Protocol


GRAPH_PATCH_API_VERSION = "researchcanvas.dev/graph-patch/v1alpha1"
PluginSettingType = Literal["boolean", "number", "text", "select"]


@dataclass(frozen=True)
class PluginUpdateInfo:
    """Optional SemVer update metadata rendered by the host plugin settings view."""

    latest_version: str | None = None
    url: str | None = None
    release_notes: str | None = None


@dataclass(frozen=True)
class PluginSetting:
    """Declarative setting metadata; the host owns persistence and rendering."""

    setting_id: str
    label: str
    setting_type: PluginSettingType
    secret: bool = False
    required: bool = False
    description: str = ""
    placeholder: str = ""
    group: str = ""
    write_only: bool = False
    default: bool | float | int | str | None = None
    minimum: float | None = None
    maximum: float | None = None
    step: float | None = None
    options: tuple[Mapping[str, str], ...] = ()

    @property
    def is_secret(self) -> bool:
        """Secret settings are host-owned and must never be read by a plugin."""

        return self.secret

    def to_mapping(self) -> Mapping[str, Any]:
        """Return the host manifest shape without exposing any secret value."""

        value: dict[str, Any] = {
            "id": self.setting_id,
            "label": self.label,
            "type": self.setting_type,
            "secret": self.secret,
            "required": self.required,
            "description": self.description,
            "placeholder": self.placeholder,
            "group": self.group,
            "writeOnly": self.write_only or self.is_secret,
            "options": [dict(option) for option in self.options],
        }
        if self.default is not None and not self.is_secret:
            value["default"] = self.default
        if self.minimum is not None:
            value["min"] = self.minimum
        if self.maximum is not None:
            value["max"] = self.maximum
        if self.step is not None:
            value["step"] = self.step
        return value


@dataclass(frozen=True)
class PluginManifest:
    plugin_id: str
    name: str
    version: str
    capabilities: tuple[str, ...]
    developer: str = ""
    developer_id: str | None = None
    permissions: tuple[str, ...] = ()
    update: PluginUpdateInfo | None = None
    settings: tuple[PluginSetting, ...] = ()


class SettingReader(Protocol):
    """Read only host-validated non-secret settings.

    The host never exposes a `secret` value to this interface. API keys are
    consumed by the host model gateway or request proxy instead.
    """

    def get_boolean(self, setting_id: str) -> bool | None: ...

    def get_number(self, setting_id: str) -> float | None: ...

    def get_text(self, setting_id: str) -> str | None: ...

    def has(self, setting_id: str) -> bool: ...


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
