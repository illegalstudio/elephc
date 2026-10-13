#!/usr/bin/env python3
"""Render deterministic, dependency-free SVG summaries from saved CI histories."""

from __future__ import annotations

import argparse
import json
from datetime import timedelta
from html import escape
from pathlib import Path

from common import ROOT, SUITES, effective_ms, format_ms
from history import load_history, timestamp

COLORS = ("#d85a26", "#168a81", "#8161c7", "#3677bc")
TITLES = {"native": "Native execution", "magician": "Magician eval", "hot-path": "Buffer and array storage"}
PRIMARY = {"native": "elephc", "magician": "elephc-eval", "hot-path": "buffer"}


def case_at(record: dict, name: str) -> dict | None:
    return next((case for case in record["cases"] if case["case"] == name), None)


def epoch(record: dict, case: dict) -> str:
    """Never join lines across changes in environment or measured fixture."""
    return json.dumps([record["environment"], case.get("fixture_sha256")], sort_keys=True)


def ratio_text(case: dict, numerator: str, denominator: str, label: str) -> str:
    variants = case["variants"]
    a = effective_ms(variants.get(numerator, {}))
    b = effective_ms(variants.get(denominator, {}))
    return f"{label}: n/a" if a is None or not b else f"{label}: {a / b:.2f}x"


def card_summary(suite: str, case: dict) -> str:
    variants = case["variants"]
    if suite == "native":
        php = effective_ms(variants['php'])
        return " | ".join([f"elephc {format_ms(variants['elephc']['median_ms'])} ms",
                           f"C {format_ms(variants['c']['median_ms'])} ms",
                           "PHP absent" if php is None else f"PHP {format_ms(php)} ms",
                           ratio_text(case, "elephc", "c", "elephc/C")])
    if suite == "magician":
        return " | ".join([f"eval {format_ms(variants['elephc-eval']['median_ms'])} ms",
                           f"native {format_ms(variants['elephc-native']['median_ms'])} ms",
                           ratio_text(case, "elephc-eval", "elephc-native", "eval/native")])
    return " | ".join(f"{label} {format_ms(value['median_ms'])} ms" for label, value in variants.items())


def render_svg(history: dict, dark: bool = False, days: int = 90) -> str:
    """Show all cases, their latest values, and recent execution trends."""
    suite = history["suite"]
    records = history["series"]
    names = sorted({case["case"] for record in records for case in record["cases"]})
    theme = {"bg": "#0b1220", "card": "#131d2d", "fg": "#eef3fa", "muted": "#a8b7cb", "line": "#344158"} if dark else {
        "bg": "#ffffff", "card": "#f6f8fa", "fg": "#172033", "muted": "#53647a", "line": "#d0d7de"}
    height = 150 + max(len(names), 1) * 150
    svg = [f'<svg xmlns="http://www.w3.org/2000/svg" width="1000" height="{height}" viewBox="0 0 1000 {height}" role="img" aria-labelledby="title desc">',
           f'<title id="title">{escape(TITLES[suite])} benchmark history</title>',
           '<desc id="desc">CI process execution medians in milliseconds. Lower is better. Lines break when environment or fixture changes.</desc>',
           f'<rect width="1000" height="{height}" rx="16" fill="{theme["bg"]}"/>']

    def text(x: float, y: float, content: str, size: int = 14, bold: bool = False, muted: bool = False) -> None:
        svg.append(f'<text x="{x:.1f}" y="{y:.1f}" fill="{theme["muted"] if muted else theme["fg"]}" '
                   f'font-family="ui-monospace,monospace" font-size="{size}" font-weight="{700 if bold else 400}">{escape(content)}</text>')

    text(28, 40, f"ELEPHC / {TITLES[suite]}", 22, True)
    latest = records[-1] if records else None
    date = latest["date"] if latest else "no CI samples yet"
    text(28, 67, f"Latest: {date} | {len(records)} saved samples | recent {days} days", 13, muted=True)
    if not records:
        text(28, 128, "The next successful main-branch benchmark run will populate this suite.", 14)
    else:
        cutoff = timestamp(latest["timestamp"]) - timedelta(days=days)
        window = [record for record in records if timestamp(record["timestamp"]) >= cutoff]
        lo = timestamp(window[0]["timestamp"]).timestamp()
        hi = timestamp(window[-1]["timestamp"]).timestamp()
        for index, name in enumerate(names):
            top = 85 + index * 150
            color = COLORS[index % len(COLORS)]
            svg.append(f'<rect x="20" y="{top}" width="960" height="140" rx="10" fill="{theme["card"]}" stroke="{theme["line"]}"/>')
            pairs = [(record, case_at(record, name)) for record in window]
            available = [(record, case) for record, case in pairs if case]
            all_available = [(record, case_at(record, name)) for record in records if case_at(record, name)]
            measured, case = all_available[-1]
            text(36, top + 25, name, 16, True)
            text(36, top + 49, card_summary(suite, case), 12)
            text(36, top + 70, f"{measured['date']} | {measured.get('commit', 'unknown')[:8]} | trend: {PRIMARY[suite]} ms", 11, muted=True)
            values = [case["variants"][PRIMARY[suite]]["median_ms"] for _, case in available]
            maximum = max(values, default=1) or 1
            segments = []
            points = []
            previous = None
            for record, case in pairs:
                key = epoch(record, case) if case else None
                if key != previous and points:
                    segments.append(points)
                    points = []
                previous = key
                if case:
                    x = 565 + (timestamp(record["timestamp"]).timestamp() - lo) / max(hi - lo, 1) * 390
                    y = top + 108 - case["variants"][PRIMARY[suite]]["median_ms"] / maximum * 65
                    points.append((x, y))
            if points:
                segments.append(points)
            svg.append(f'<path d="M565 {top + 108} H955" stroke="{theme["line"]}" fill="none"/>')
            for segment in segments:
                coordinates = " ".join(f"{x:.2f},{y:.2f}" for x, y in segment)
                svg.append(f'<polyline points="{coordinates}" stroke="{color}" stroke-width="2" fill="none"/>')
                for x, y in segment:
                    svg.append(f'<circle cx="{x:.2f}" cy="{y:.2f}" r="1.5" fill="{color}"/>')
            text(565, top + 127, window[0]["date"], 10, muted=True)
            text(875, top + 127, window[-1]["date"], 10, muted=True)
            text(490, top + 48, f"{maximum:.1f}", 10, muted=True)
            text(530, top + 109, "0", 10, muted=True)
    text(28, height - 35, "Process execution only; compile/link excluded. Shared CI runners have timing noise.", 11, muted=True)
    text(28, height - 16, "Legacy runner/workload metadata is unknown. Environment/fixture changes start a new line.", 11, muted=True)
    svg.append("</svg>\n")
    return "\n".join(svg)


def generate(data_dir: Path, output_dir: Path) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    for suite in SUITES:
        history = load_history(data_dir / f"{suite}.json", suite)
        for theme in ("light", "dark"):
            (output_dir / f"{suite}-{theme}.svg").write_text(render_svg(history, theme == "dark"))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data-dir", type=Path, default=ROOT / "data")
    parser.add_argument("--out-dir", type=Path, default=ROOT / "assets")
    args = parser.parse_args()
    generate(args.data_dir, args.out_dir)


if __name__ == "__main__":
    main()
