"""Build a deterministic Research Canvas .myc plugin archive."""

from __future__ import annotations

import argparse
import hashlib
import pathlib
import zipfile


ALLOWED_ROOT_FILES = {
    "plugin.yml",
    "agent.yml",
    "theme.json",
    "edge-style.json",
    "workspace-plugin.json",
    "agent-manifest.json",
    "provider.json",
    "plugin.wasm",
    "plugin.wat",
    "README.md",
    "LICENSE",
}
ALLOWED_DIRECTORIES = {
    "locales": {".json"},
    "prompts": {".yaml", ".yml"},
    "schemas": {".json"},
}


def build(source: pathlib.Path, destination: pathlib.Path) -> None:
    source = source.resolve()
    destination = destination.resolve()
    manifest = source / "plugin.yml"
    if not source.is_dir() or not manifest.is_file():
        raise SystemExit("source must be a plugin folder containing plugin.yml")

    files = sorted(path for path in source.rglob("*") if path.is_file())
    for path in files:
        relative = path.relative_to(source)
        root = relative.parts[0]
        if root in ALLOWED_DIRECTORIES:
            if (
                len(relative.parts) != 2
                or relative.suffix.lower() not in ALLOWED_DIRECTORIES[root]
            ):
                raise SystemExit(f"unsupported package entry: {relative.as_posix()}")
            continue
        if root not in ALLOWED_ROOT_FILES:
            raise SystemExit(f"unsupported package entry: {relative.as_posix()}")

    # Every payload file (all but plugin.yml) is sha256-hashed into a top-level
    # `payloads:` block appended to the archived manifest, so a manifest
    # signature covers every payload byte in the package.
    manifest_text = manifest.read_text(encoding="utf-8").rstrip() + "\n"
    if "\npayloads:" in f"\n{manifest_text}":
        raise SystemExit("plugin.yml must not declare payloads; they are build-generated")
    payload_lines = ["payloads:"]
    for path in files:
        relative = path.relative_to(source).as_posix()
        if relative == "plugin.yml":
            continue
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        payload_lines.append(f"  {relative}: {digest}")
    archived_manifest = manifest_text + "\n".join(payload_lines) + "\n"

    destination.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(
        destination, mode="w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
    ) as archive:
        for path in files:
            relative = path.relative_to(source).as_posix()
            info = zipfile.ZipInfo(relative, date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o644 << 16
            if relative == "plugin.yml":
                archive.writestr(info, archived_manifest.encode("utf-8"))
            else:
                archive.writestr(info, path.read_bytes())

    print(destination)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=pathlib.Path)
    parser.add_argument("destination", type=pathlib.Path)
    arguments = parser.parse_args()
    build(arguments.source, arguments.destination)
