"""Build a deterministic Research Canvas .myc plugin archive."""

from __future__ import annotations

import argparse
import pathlib
import zipfile


ALLOWED_ROOT_FILES = {"plugin.yml", "theme.json", "README.md", "LICENSE"}


def build(source: pathlib.Path, destination: pathlib.Path) -> None:
    source = source.resolve()
    destination = destination.resolve()
    manifest = source / "plugin.yml"
    if not source.is_dir() or not manifest.is_file():
        raise SystemExit("source must be a plugin folder containing plugin.yml")

    files = sorted(path for path in source.rglob("*") if path.is_file())
    for path in files:
        relative = path.relative_to(source)
        if relative.parts[0] not in ALLOWED_ROOT_FILES:
            raise SystemExit(f"unsupported package entry: {relative.as_posix()}")

    destination.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(
        destination, mode="w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
    ) as archive:
        for path in files:
            relative = path.relative_to(source).as_posix()
            info = zipfile.ZipInfo(relative, date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o644 << 16
            archive.writestr(info, path.read_bytes())

    print(destination)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=pathlib.Path)
    parser.add_argument("destination", type=pathlib.Path)
    arguments = parser.parse_args()
    build(arguments.source, arguments.destination)

