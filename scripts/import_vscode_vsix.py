#!/usr/bin/env python3
"""Convert declarative VS Code theme/icon-theme contributions into MYC source data.

This importer never imports or executes a VS Code extension runtime. It reads only
package.json contributions and referenced JSON/SVG/PNG/font assets. The generated
IconThemePlugin is an adapter resource for the future host icon registry; it is
not an installable VS Code extension.
"""

from __future__ import annotations

import argparse
import json
import posixpath
import re
import shutil
import stat
import zipfile
from pathlib import Path
from typing import Any

MAX_ENTRIES = 10_000
MAX_MEMBER_BYTES = 16 * 1024 * 1024
MAX_TOTAL_BYTES = 64 * 1024 * 1024
MAX_COMPRESSION_RATIO = 200
SAFE_EXTENSIONS = {".json", ".svg", ".png", ".woff", ".woff2", ".ttf", ".otf"}
EXECUTABLE_EXTENSIONS = {
    ".js",
    ".cjs",
    ".mjs",
    ".ts",
    ".wasm",
    ".exe",
    ".dll",
    ".so",
    ".dylib",
    ".node",
    ".bin",
    ".bat",
    ".cmd",
    ".ps1",
    ".sh",
}
NATIVE_EXTENSIONS = {".wasm", ".exe", ".dll", ".so", ".dylib", ".node", ".bin"}
COLOR_FALLBACKS = {
    "app": "#eef1f5",
    "panel": "#ffffff",
    "canvas": "#f8f9fb",
    "text": "#172033",
    "muted": "#697386",
    "accent": "#6750d8",
    "border": "#dfe3e9",
}


def fail(message: str) -> "NoReturn":
    raise ValueError(message)


def safe_archive_path(path: str) -> str:
    if not path or path.startswith("/") or "\\" in path or ":" in path:
        fail(f"Unsafe VSIX archive path: {path!r}")
    raw_parts = path.split("/")
    if any(part in {"", ".", ".."} for part in raw_parts):
        fail(f"Unsafe VSIX archive path: {path!r}")
    normalized = posixpath.normpath(path)
    parts = normalized.split("/")
    if normalized == "." or any(part in {"", ".", ".."} for part in parts):
        fail(f"Unsafe VSIX archive path: {path!r}")
    return normalized


def is_symlink(info: zipfile.ZipInfo) -> bool:
    mode = (info.external_attr >> 16) & 0xFFFF
    return stat.S_ISLNK(mode)


def validate_archive(infos: list[zipfile.ZipInfo]) -> tuple[list[str], int]:
    if len(infos) > MAX_ENTRIES:
        fail("VSIX contains too many archive entries")
    total = 0
    names: list[str] = []
    for info in infos:
        if info.is_dir():
            continue
        name = safe_archive_path(info.filename)
        if is_symlink(info):
            fail(f"VSIX contains a symbolic link: {name}")
        if info.file_size > MAX_MEMBER_BYTES:
            fail(f"VSIX member exceeds {MAX_MEMBER_BYTES} bytes: {name}")
        if info.compress_size and info.file_size / info.compress_size > MAX_COMPRESSION_RATIO:
            fail(f"VSIX member compression ratio is unsafe: {name}")
        if Path(name).suffix.lower() in NATIVE_EXTENSIONS:
            fail(f"VSIX contains a native binary: {name}")
        total += info.file_size
        if total > MAX_TOTAL_BYTES:
            fail(f"VSIX uncompressed payload exceeds {MAX_TOTAL_BYTES} bytes")
        names.append(name)
    return names, total


def read_member(archive: zipfile.ZipFile, name: str, limit: int = MAX_MEMBER_BYTES) -> bytes:
    try:
        with archive.open(name, "r") as handle:
            data = handle.read(limit + 1)
    except KeyError:
        fail(f"VSIX contribution references missing file: {name}")
    if len(data) > limit:
        fail(f"VSIX member exceeds {limit} bytes: {name}")
    return data


def read_json(archive: zipfile.ZipFile, name: str) -> dict[str, Any]:
    try:
        value = json.loads(read_member(archive, name, 2 * 1024 * 1024).decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        fail(f"Invalid JSON in VSIX member {name}: {error}")
    if not isinstance(value, dict):
        fail(f"VSIX JSON contribution must be an object: {name}")
    return value


def package_root(package_path: str) -> str:
    parent = posixpath.dirname(package_path)
    return "" if parent == "." else parent


def resolve_member(root: str, requested: str) -> str:
    if not isinstance(requested, str) or not requested.strip():
        fail("VSIX contribution path must be a non-empty string")
    if requested.startswith(("/", "\\")) or "\\" in requested:
        fail(f"Unsafe VSIX contribution path: {requested!r}")
    if any(part in {"", ".", ".."} for part in requested.split("/")):
        fail(f"Unsafe VSIX contribution path: {requested!r}")
    resolved = posixpath.normpath(posixpath.join(root, requested))
    if resolved == "." or resolved == ".." or resolved.startswith("../"):
        fail(f"VSIX contribution escapes the package root: {requested!r}")
    return safe_archive_path(resolved)


def safe_id(value: str) -> str:
    normalized = re.sub(r"[^a-zA-Z0-9]+", "-", value.strip()).strip("-").lower()
    if not normalized:
        fail("VSIX package has no usable identifier")
    return normalized[:80]


def package_identity(package: dict[str, Any]) -> tuple[str, str, str, str]:
    name = package.get("name")
    version = package.get("version")
    publisher = package.get("publisher") or "vscode"
    if not isinstance(name, str) or not isinstance(version, str):
        fail("VSIX package.json requires string name and version")
    display_name = package.get("displayName")
    label = display_name if isinstance(display_name, str) and display_name.strip() else name
    return name, version, publisher, label


def assert_declarative(package: dict[str, Any]) -> None:
    if package.get("main") or package.get("browser"):
        fail("VSIX declares main/browser code and is not a declarative theme package")
    activation = package.get("activationEvents")
    if isinstance(activation, list) and activation:
        fail("VSIX declares activation events and cannot be imported as data")
    contributes = package.get("contributes")
    if not isinstance(contributes, dict):
        fail("VSIX does not declare theme contributions")
    commands = contributes.get("commands")
    if isinstance(commands, list) and commands:
        fail("VSIX commands are not imported or executed")
    if not isinstance(contributes.get("themes"), list) and not isinstance(contributes.get("iconThemes"), list):
        fail("VSIX does not contribute a theme or icon theme")


def value_color(colors: dict[str, Any], *keys: str, fallback: str) -> str:
    for key in keys:
        value = colors.get(key)
        if isinstance(value, str) and value.strip() and len(value) <= 128:
            return value.strip()
    return fallback


def convert_theme(package: dict[str, Any], contribution: dict[str, Any], theme: dict[str, Any], package_name: str, version: str, publisher: str, label: str) -> dict[str, Any]:
    colors = theme.get("colors") if isinstance(theme.get("colors"), dict) else {}
    plugin_id = f"vsix-{safe_id(publisher)}-{safe_id(package_name)}-{safe_id(str(contribution.get('label') or label))}"
    semantic = {
        "app": value_color(colors, "activityBar.background", "titleBar.activeBackground", fallback=COLOR_FALLBACKS["app"]),
        "panel": value_color(colors, "sideBar.background", "panel.background", fallback=COLOR_FALLBACKS["panel"]),
        "canvas": value_color(colors, "editor.background", "editorGroup.emptyBackground", fallback=COLOR_FALLBACKS["canvas"]),
        "text": value_color(colors, "foreground", "editor.foreground", "sideBar.foreground", fallback=COLOR_FALLBACKS["text"]),
        "muted": value_color(colors, "descriptionForeground", "disabledForeground", fallback=COLOR_FALLBACKS["muted"]),
        "accent": value_color(colors, "focusBorder", "button.background", "textLink.foreground", fallback=COLOR_FALLBACKS["accent"]),
        "border": value_color(colors, "editorGroup.border", "panel.border", "contrastBorder", fallback=COLOR_FALLBACKS["border"]),
    }
    return {
        "pluginId": plugin_id,
        "kind": "ThemePlugin",
        "version": version,
        "pluginYml": plugin_yml(plugin_id, label, version, publisher, "theme.json", "theme.register", "ThemePlugin"),
        "theme": {
            "id": plugin_id,
            "name": str(contribution.get("label") or label),
            "publisher": publisher,
            "version": version,
            "description": package.get("description") or "Imported declarative VS Code theme",
            "developer": publisher,
            "source": "vsix",
            "colors": semantic,
            "vscode": {
                "uiTheme": contribution.get("uiTheme"),
                "tokenColors": theme.get("tokenColors", []),
                "semanticTokenColors": theme.get("semanticTokenColors", {}),
            },
        },
        "copiedAssets": [],
    }


def icon_path(value: Any, root: str, archive_names: set[str]) -> str | None:
    if not isinstance(value, str) or not value.strip():
        return None
    resolved = resolve_member(root, value)
    if resolved not in archive_names:
        fail(f"Icon theme references missing asset: {resolved}")
    if Path(resolved).suffix.lower() not in SAFE_EXTENSIONS:
        fail(f"Icon theme references a non-declarative asset: {resolved}")
    return resolved


def convert_icon_theme(package: dict[str, Any], contribution: dict[str, Any], icon_theme: dict[str, Any], package_name: str, version: str, publisher: str, label: str, icon_root: str, archive_names: set[str], archive: zipfile.ZipFile, output_root: Path) -> dict[str, Any]:
    icon_label = str(contribution.get("label") or label)
    plugin_id = f"vsix-{safe_id(publisher)}-{safe_id(package_name)}-{safe_id(str(contribution.get('id') or icon_label))}-icons"
    copied_assets: list[str] = []

    def copy_asset(value: Any) -> str | None:
        resolved = icon_path(value, icon_root, archive_names)
        if not resolved:
            return None
        target = Path("assets") / Path(resolved).name
        destination = output_root / target
        destination.parent.mkdir(parents=True, exist_ok=True)
        if str(target.as_posix()) not in copied_assets:
            destination.write_bytes(read_member(archive, resolved))
            copied_assets.append(target.as_posix())
        return target.as_posix()

    definitions: dict[str, Any] = {}
    raw_definitions = icon_theme.get("iconDefinitions")
    if isinstance(raw_definitions, dict):
        for definition_id, raw in raw_definitions.items():
            if not isinstance(definition_id, str) or not isinstance(raw, dict):
                continue
            definition: dict[str, Any] = {}
            asset = copy_asset(raw.get("iconPath"))
            if asset:
                definition["iconPath"] = asset
            if isinstance(raw.get("fontCharacter"), str):
                definition["fontCharacter"] = raw["fontCharacter"]
            if isinstance(raw.get("fontId"), str):
                definition["fontId"] = raw["fontId"]
            definitions[definition_id] = definition

    fonts: list[dict[str, Any]] = []
    raw_fonts = icon_theme.get("fonts")
    if isinstance(raw_fonts, list):
        for raw_font in raw_fonts:
            if not isinstance(raw_font, dict) or not isinstance(raw_font.get("id"), str):
                continue
            sources: list[str] = []
            raw_sources = raw_font.get("src")
            if isinstance(raw_sources, list):
                for source in raw_sources:
                    source_path = source.get("path") if isinstance(source, dict) else source
                    asset = copy_asset(source_path)
                    if asset:
                        sources.append(asset)
            fonts.append({
                "id": raw_font["id"],
                "src": sources,
                "weight": raw_font.get("weight"),
                "style": raw_font.get("style"),
            })

    def mapping(key: str) -> dict[str, str]:
        raw = icon_theme.get(key)
        return {str(name): str(value) for name, value in raw.items() if isinstance(name, str) and isinstance(value, str)} if isinstance(raw, dict) else {}

    resource = {
        "pluginId": plugin_id,
        "kind": "IconThemePlugin",
        "version": version,
        "pluginYml": plugin_yml(plugin_id, icon_label, version, publisher, "icon-theme.json", "icon-theme.register", "IconThemePlugin"),
        "iconTheme": {
            "schemaVersion": 1,
            "id": plugin_id,
            "name": icon_label,
            "publisher": publisher,
            "version": version,
            "description": package.get("description") or "Imported declarative VS Code icon theme",
            "source": "vsix",
            "fileExtensions": mapping("fileExtensions"),
            "fileNames": mapping("fileNames"),
            "folderNames": mapping("folderNames"),
            "folderNamesExpanded": mapping("folderNamesExpanded"),
            "iconDefinitions": definitions,
            "fonts": fonts,
        },
        "copiedAssets": copied_assets,
    }
    return resource


def plugin_yml(plugin_id: str, name: str, version: str, publisher: str, entry: str, capability: str, kind: str) -> str:
    def quote(value: str) -> str:
        return json.dumps(value, ensure_ascii=False)

    return "\n".join([
        "apiVersion: researchcanvas.dev/v1alpha1",
        f"kind: {kind}",
        "metadata:",
        f"  id: {quote(plugin_id)}",
        f"  name: {quote(name)}",
        f"  version: {quote(version)}",
        f"  publisher: {quote(publisher)}",
        f"  developer: {quote(publisher)}",
        f"  description: {quote('Imported declarative VS Code contribution')}",
        "spec:",
        "  engine: declarative",
        f"  entry: {entry}",
        "  capabilities:",
        f"    - {capability}",
        "  permissions: []",
        "",
    ])


def write_resource(resource: dict[str, Any], output: Path) -> None:
    output.mkdir(parents=True, exist_ok=True)
    (output / "plugin.yml").write_text(resource["pluginYml"], encoding="utf-8")
    entry = "theme.json" if resource["kind"] == "ThemePlugin" else "icon-theme.json"
    payload = resource["theme"] if resource["kind"] == "ThemePlugin" else resource["iconTheme"]
    (output / entry).write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def find_package_json(names: list[str]) -> str:
    candidates = [name for name in names if name == "package.json" or name.endswith("/package.json")]
    if not candidates:
        fail("VSIX package.json is missing")
    return min(candidates, key=lambda name: (name != "extension/package.json", len(name)))


def convert(vsix: Path, output: Path) -> dict[str, Any]:
    if vsix.suffix.lower() != ".vsix":
        fail("Input must be a .vsix archive")
    if output.exists():
        if output.is_file():
            fail("Output must be a directory")
        shutil.rmtree(output)
    output.mkdir(parents=True)
    with zipfile.ZipFile(vsix, "r") as archive:
        names, _ = validate_archive(archive.infolist())
        name_set = set(names)
        package_path = find_package_json(names)
        package = read_json(archive, package_path)
        assert_declarative(package)
        package_name, version, publisher, label = package_identity(package)
        root = package_root(package_path)
        contributes = package["contributes"]
        report: dict[str, Any] = {
            "apiVersion": "researchcanvas.dev/vsix-import/v1alpha1",
            "source": str(vsix),
            "package": {"name": package_name, "publisher": publisher, "version": version},
            "themes": [],
            "iconThemes": [],
            "ignoredExecutableAssets": [name for name in names if Path(name).suffix.lower() in EXECUTABLE_EXTENSIONS],
            "rejectedAssets": [],
        }
        for index, contribution in enumerate(contributes.get("themes", [])):
            if not isinstance(contribution, dict):
                fail("VSIX theme contribution must be an object")
            theme_path = resolve_member(root, contribution.get("path"))
            if Path(theme_path).suffix.lower() != ".json":
                fail("VSIX theme contribution must reference JSON")
            theme = read_json(archive, theme_path)
            resource = convert_theme(package, contribution, theme, package_name, version, publisher, label)
            resource_dir = output / f"{resource['pluginId']}-{index}"
            write_resource(resource, resource_dir)
            report["themes"].append({**resource, "output": str(resource_dir)})
        for index, contribution in enumerate(contributes.get("iconThemes", [])):
            if not isinstance(contribution, dict):
                fail("VSIX icon theme contribution must be an object")
            icon_path_value = contribution.get("path")
            icon_path_resolved = resolve_member(root, icon_path_value)
            if Path(icon_path_resolved).suffix.lower() != ".json":
                fail("VSIX icon theme contribution must reference JSON")
            icon_theme = read_json(archive, icon_path_resolved)
            resource_dir = output / f"vsix-{safe_id(publisher)}-{safe_id(package_name)}-icons-{index}"
            resource = convert_icon_theme(package, contribution, icon_theme, package_name, version, publisher, label, posixpath.dirname(icon_path_resolved), name_set, archive, resource_dir)
            write_resource(resource, resource_dir)
            report["iconThemes"].append({**resource, "output": str(resource_dir)})
        report_path = output / "import-report.json"
        report_path.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("vsix", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    try:
        report = convert(args.vsix, args.output)
    except (OSError, ValueError, zipfile.BadZipFile) as error:
        parser.error(str(error))
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
