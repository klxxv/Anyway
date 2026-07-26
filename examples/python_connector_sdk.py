"""Minimal file-protocol SDK for a Research Canvas scenario run.

Usage:
    python examples/python_connector_sdk.py scenario-run-manifest.json run-result.json

The example intentionally does not execute arbitrary shell text from a manifest.
It validates stable IDs, reads reviewed parameters, and emits one structured
RunResult that can be imported from the Research Canvas navigator.
"""

from __future__ import annotations

import argparse
import json
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


PROTOCOL = "research-canvas-run/1"


def read_manifest(path: Path) -> dict[str, Any]:
    manifest = json.loads(path.read_text(encoding="utf-8"))
    if manifest.get("protocolVersion") != PROTOCOL:
        raise ValueError(f"Expected protocolVersion={PROTOCOL!r}")
    if not isinstance(manifest.get("projectId"), str):
        raise ValueError("projectId must be a stable string")
    scenario = manifest.get("scenario")
    if scenario is not None and not isinstance(scenario.get("id"), str):
        raise ValueError("scenario.id must be a stable string")
    return manifest


def create_result(manifest: dict[str, Any]) -> dict[str, Any]:
    scenario = manifest.get("scenario") or {}
    parameters = scenario.get("parameters") or {}
    # A connector replaces this deterministic demonstration value with the
    # metric produced by reviewed Python code.
    value = float(parameters.get("exampleMetricValue", 0.941))
    return {
        "protocolVersion": "research-canvas-result/1",
        "projectId": manifest["projectId"],
        "projectRevision": manifest.get("projectRevision"),
        "scenarioId": scenario.get("id"),
        "metric": "example_accuracy",
        "value": value,
        "summary": "Structured result written by the example Python connector SDK.",
        "artifact": {
            "kind": "json",
            "path": "run-result.json",
        },
        "completedAt": datetime.now(timezone.utc).isoformat(),
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("manifest", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    manifest = read_manifest(args.manifest)
    result = create_result(manifest)
    args.output.write_text(json.dumps(result, indent=2), encoding="utf-8")
    print(args.output)


if __name__ == "__main__":
    main()
