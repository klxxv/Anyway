#!/usr/bin/env python3
"""Render deterministic SSE/BP and Canvas Diff manifests into two PNGs."""

from __future__ import annotations

import json
import math
import sys
from pathlib import Path
from typing import Any

from PIL import Image, ImageDraw, ImageFont


ROOT = Path(__file__).resolve().parents[1]
MANIFEST = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "target/sse-canvas-artifacts/manifest.json"
OUTPUT_DIR = Path(sys.argv[2]) if len(sys.argv) > 2 else ROOT / "docs"

COLORS = {
    "ink": "#20242A",
    "muted": "#69727D",
    "paper": "#FBFAF6",
    "canvas": "#F2F0E9",
    "line": "#AAB1B8",
    "blue": "#2457D6",
    "blue_soft": "#EAF0FF",
    "green": "#15803D",
    "green_fill": "#E8F6EC",
    "red": "#B42318",
    "red_fill": "#FDECEA",
    "yellow": "#A16207",
    "yellow_fill": "#FFF4C2",
    "white": "#FFFFFF",
}


def font(size: int, bold: bool = False) -> ImageFont.FreeTypeFont:
    candidates = [
        Path("C:/Windows/Fonts/seguisb.ttf" if bold else "C:/Windows/Fonts/segoeui.ttf"),
        Path("C:/Windows/Fonts/arialbd.ttf" if bold else "C:/Windows/Fonts/arial.ttf"),
    ]
    for candidate in candidates:
        if candidate.is_file():
            return ImageFont.truetype(str(candidate), size)
    return ImageFont.load_default()


FONTS = {
    "title": font(36, True),
    "subtitle": font(17),
    "heading": font(19, True),
    "body": font(15),
    "small": font(13),
    "mono": font(13, True),
}


def rounded(draw: ImageDraw.ImageDraw, box: tuple[int, int, int, int], fill: str, outline: str, width: int = 2, radius: int = 14) -> None:
    draw.rounded_rectangle(box, radius=radius, fill=fill, outline=outline, width=width)


def wrap(draw: ImageDraw.ImageDraw, text: str, target_width: int, face: ImageFont.ImageFont, max_lines: int = 2) -> list[str]:
    words = text.split()
    lines: list[str] = []
    current = ""
    for word in words:
        candidate = f"{current} {word}".strip()
        if draw.textbbox((0, 0), candidate, font=face)[2] <= target_width:
            current = candidate
        else:
            if current:
                lines.append(current)
            current = word
            if len(lines) == max_lines - 1:
                break
    if current and len(lines) < max_lines:
        lines.append(current)
    consumed = " ".join(lines)
    if len(consumed) < len(text) and lines:
        while lines[-1] and draw.textbbox((0, 0), lines[-1] + "…", font=face)[2] > target_width:
            lines[-1] = lines[-1][:-1]
        lines[-1] += "…"
    return lines


def arrow(draw: ImageDraw.ImageDraw, start: tuple[int, int], end: tuple[int, int], color: str, label: str = "", dashed: bool = False) -> None:
    x1, y1 = start
    x2, y2 = end
    if dashed:
        length = math.hypot(x2 - x1, y2 - y1)
        steps = max(1, int(length / 16))
        for index in range(0, steps, 2):
            a = index / steps
            b = min(1, (index + 1) / steps)
            draw.line((x1 + (x2 - x1) * a, y1 + (y2 - y1) * a, x1 + (x2 - x1) * b, y1 + (y2 - y1) * b), fill=color, width=3)
    else:
        draw.line((x1, y1, x2, y2), fill=color, width=3)
    angle = math.atan2(y2 - y1, x2 - x1)
    tip = (x2, y2)
    left = (x2 - 13 * math.cos(angle - 0.45), y2 - 13 * math.sin(angle - 0.45))
    right = (x2 - 13 * math.cos(angle + 0.45), y2 - 13 * math.sin(angle + 0.45))
    draw.polygon([tip, left, right], fill=color)
    if label:
        mx, my = (x1 + x2) // 2, (y1 + y2) // 2
        bbox = draw.textbbox((0, 0), label, font=FONTS["small"])
        w, h = bbox[2] - bbox[0] + 12, bbox[3] - bbox[1] + 6
        draw.rounded_rectangle((mx - w // 2, my - h // 2, mx + w // 2, my + h // 2), 5, fill=COLORS["paper"])
        draw.text((mx - w // 2 + 6, my - h // 2 + 2), label, font=FONTS["small"], fill=color)


def node_box(draw: ImageDraw.ImageDraw, node: dict[str, Any], belief: dict[str, Any] | None, box: tuple[int, int, int, int], state: str = "normal", before_belief: dict[str, Any] | None = None) -> None:
    palette = {
        "normal": (COLORS["white"], "#88919B"),
        "added": (COLORS["green_fill"], COLORS["green"]),
        "removed": (COLORS["red_fill"], COLORS["red"]),
        "special": (COLORS["yellow_fill"], COLORS["yellow"]),
    }
    fill, outline = palette[state]
    rounded(draw, box, fill, outline, 3 if state != "normal" else 2)
    x1, y1, x2, y2 = box
    kind = str(node.get("type", "node")).upper()
    draw.text((x1 + 16, y1 + 12), kind, font=FONTS["mono"], fill=outline)
    lines = wrap(draw, str(node.get("title", node.get("id", ""))), x2 - x1 - 32, FONTS["heading"], 2)
    y = y1 + 34
    for line in lines:
        draw.text((x1 + 16, y), line, font=FONTS["heading"], fill=COLORS["ink"])
        y += 24
    if belief:
        net = float(belief.get("netBelief", 0.5))
        support = float(belief.get("support", 0.5))
        refutation = float(belief.get("refutation", 0.5))
        if before_belief:
            old = float(before_belief.get("netBelief", 0.5))
            probability = f"BP P(net) {old:.3f} → {net:.3f}"
        else:
            probability = f"BP P(net) {net:.3f}"
        draw.text((x1 + 16, y2 - 42), probability, font=FONTS["mono"], fill=outline)
        draw.text((x1 + 16, y2 - 22), f"support {support:.3f} · refute {refutation:.3f}", font=FONTS["small"], fill=COLORS["muted"])
    else:
        draw.text((x1 + 16, y2 - 26), "structural node · BP n/a", font=FONTS["small"], fill=COLORS["muted"])


BASE_POS = {
    "q-retention": (660, 118),
    "h-residual": (660, 342),
    "v-residual-gate": (110, 230),
    "v-context-length": (110, 475),
    "exp-primary": (610, 690),
    "r-retrieval": (1180, 155),
    "r-perplexity": (1180, 405),
    "r-latency": (1180, 655),
}

DIFF_POS = {
    "q-retention": (700, 105),
    "h-residual": (700, 315),
    "v-context-length": (70, 150),
    "v-residual-gate": (70, 365),
    "v-head-gate": (70, 580),
    "v-depth-gate": (70, 795),
    "exp-primary": (695, 690),
    "r-retrieval": (1320, 150),
    "r-perplexity": (1320, 365),
    "r-latency": (1320, 580),
    "r-replication": (1320, 795),
}


def edge_points(source: tuple[int, int], target: tuple[int, int], width: int = 310, height: int = 142) -> tuple[tuple[int, int], tuple[int, int]]:
    sx, sy = source
    tx, ty = target
    sc = (sx + width // 2, sy + height // 2)
    tc = (tx + width // 2, ty + height // 2)
    dx, dy = tc[0] - sc[0], tc[1] - sc[1]
    if abs(dx) > abs(dy):
        return ((sx + width if dx > 0 else sx, sc[1]), (tx if dx > 0 else tx + width, tc[1]))
    return ((sc[0], sy + height if dy > 0 else sy), (tc[0], ty if dy > 0 else ty + height))


def edge_label(edge: dict[str, Any]) -> str:
    p_value = edge.get("data", {}).get("pValue")
    confidence = edge.get("confidence")
    pieces = [str(edge.get("type", "edge"))]
    if p_value is not None:
        pieces.append(f"p={float(p_value):.3f}")
    if confidence is not None:
        pieces.append(f"C={float(confidence):.2f}")
    return " · ".join(pieces)


def render_base(manifest: dict[str, Any], output: Path) -> None:
    width, height = 1640, 980
    image = Image.new("RGB", (width, height), COLORS["canvas"])
    draw = ImageDraw.Draw(image)
    draw.text((55, 35), "SSE → Canvas → Dual-channel BP", font=FONTS["title"], fill=COLORS["ink"])
    draw.text((57, 82), "Probabilities are computed by the Rust graph kernel; SSE supplies only entities, p-values and confidence inputs.", font=FONTS["subtitle"], fill=COLORS["muted"])
    base = manifest["base"]
    project = base["project"]
    beliefs = base["beliefsByNode"]
    bp = base["bp"]
    rounded(draw, (1130, 30, 1585, 105), COLORS["blue_soft"], COLORS["blue"], 2)
    draw.text((1150, 44), f"BP {str(bp['status']).replace('_', ' ')} · {bp['iterations']} iterations", font=FONTS["mono"], fill=COLORS["blue"])
    draw.text((1150, 72), f"mean P(net) {bp['meanNetBelief']:.4f} · variables {bp['variableCount']}", font=FONTS["body"], fill=COLORS["ink"])
    for edge in project["edges"]:
        if edge["source"] in BASE_POS and edge["target"] in BASE_POS:
            start, end = edge_points(BASE_POS[edge["source"]], BASE_POS[edge["target"]])
            color = COLORS["red"] if edge.get("polarity") == "negative" else COLORS["blue"]
            arrow(draw, start, end, color, edge_label(edge), dashed=edge.get("polarity") == "negative")
    for node in project["nodes"]:
        x, y = BASE_POS[node["id"]]
        node_box(draw, node, beliefs.get(node["id"]), (x, y, x + 310, y + 142))
    draw.text((55, 942), "Legend: BP P(net)=σ(support logit−refutation logit) · C=edge confidence · p=statistical test probability", font=FONTS["small"], fill=COLORS["muted"])
    output.parent.mkdir(parents=True, exist_ok=True)
    image.save(output, "PNG", optimize=True)


def node_map(project: dict[str, Any]) -> dict[str, dict[str, Any]]:
    return {item["id"]: item for item in project["nodes"]}


def render_diff(manifest: dict[str, Any], output: Path) -> None:
    width, height = 1760, 1080
    image = Image.new("RGB", (width, height), COLORS["canvas"])
    draw = ImageDraw.Draw(image)
    draw.text((55, 32), "Canvas Diff · replication vs baseline", font=FONTS["title"], fill=COLORS["ink"])
    draw.text((57, 79), "Git-style semantic blocks: additions, removals, and special experiment/variable revisions.", font=FONTS["subtitle"], fill=COLORS["muted"])
    diff_manifest = manifest["canvasDiff"]
    diff = diff_manifest["diff"]
    baseline = diff_manifest["baseline"]
    comparison = diff_manifest["comparison"]
    before = node_map(baseline)
    after = node_map(comparison)
    base_beliefs = manifest["base"]["beliefsByNode"]
    next_beliefs = manifest["comparison"]["beliefsByNode"]
    added = set(diff["addedNodes"])
    removed = set(diff["removedNodes"])
    modified = {item["entityId"] for item in diff["modifiedNodes"]}
    yellow = set(diff_manifest["renderPolicy"]["yellowNodeIds"])

    legend = [("ADDED", COLORS["green_fill"], COLORS["green"]), ("REMOVED", COLORS["red_fill"], COLORS["red"]), ("RETEST / VARIABLE FISSION", COLORS["yellow_fill"], COLORS["yellow"])]
    lx = 955
    for text, fill, outline in legend:
        box_width = 130 if text != "RETEST / VARIABLE FISSION" else 250
        rounded(draw, (lx, 38, lx + box_width, 78), fill, outline, 2, 8)
        draw.text((lx + 12, 49), text, font=FONTS["mono"], fill=outline)
        lx += box_width + 14

    summary = f"+{len(added)} nodes  −{len(removed)} nodes  ~{len(modified)} nodes  ·  {len(diff['addedEdges'])} added edges"
    draw.text((57, 106), summary, font=FONTS["mono"], fill=COLORS["ink"])

    union_edges: dict[str, dict[str, Any]] = {edge["id"]: edge for edge in baseline["edges"]}
    union_edges.update({edge["id"]: edge for edge in comparison["edges"]})
    added_edges = set(diff["addedEdges"])
    removed_edges = set(diff["removedEdges"])
    for edge_id, edge in union_edges.items():
        if edge["source"] not in DIFF_POS or edge["target"] not in DIFF_POS:
            continue
        start, end = edge_points(DIFF_POS[edge["source"]], DIFF_POS[edge["target"]], 330, 145)
        if edge_id in removed_edges:
            color, dashed = COLORS["red"], True
        elif edge_id in added_edges:
            color, dashed = COLORS["green"], False
        elif edge["source"] in yellow or edge["target"] in yellow:
            color, dashed = COLORS["yellow"], False
        else:
            color, dashed = COLORS["line"], False
        arrow(draw, start, end, color, "", dashed)

    for node_id, position in DIFF_POS.items():
        node = after.get(node_id) or before.get(node_id)
        if not node:
            continue
        if node_id in yellow:
            state = "special"
        elif node_id in added:
            state = "added"
        elif node_id in removed:
            state = "removed"
        else:
            state = "normal"
        x, y = position
        node_box(draw, node, next_beliefs.get(node_id) or base_beliefs.get(node_id), (x, y, x + 330, y + 145), state, base_beliefs.get(node_id) if node_id in modified else None)
    draw.text((55, 1040), "Yellow overrides green for newly split variables and marks modified existing experiments/variables. Probabilities are recomputed after the diff.", font=FONTS["small"], fill=COLORS["muted"])
    output.parent.mkdir(parents=True, exist_ok=True)
    image.save(output, "PNG", optimize=True)


def main() -> None:
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    render_base(manifest, OUTPUT_DIR / "sse-bp-canvas.png")
    render_diff(manifest, OUTPUT_DIR / "sse-canvas-diff.png")
    print(OUTPUT_DIR / "sse-bp-canvas.png")
    print(OUTPUT_DIR / "sse-canvas-diff.png")


if __name__ == "__main__":
    main()
