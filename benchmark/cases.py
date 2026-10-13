"""Discover native, Magician, and buffer/array fixtures for the common runner."""

from __future__ import annotations

import json
from pathlib import Path

from common import ROOT, SUITES


def magician_metadata(directory: Path) -> dict:
    """Validate eval fragment descriptions and retain their interpretation fields."""
    data = json.loads((directory / "metadata.json").read_text())
    fragments = data.get("eval_fragments", [])
    if not fragments:
        raise ValueError(f"{directory.name}: metadata must describe eval fragments")
    kinds = set()
    for fragment in fragments:
        if not isinstance(fragment.get("source"), str):
            raise ValueError(f"{directory.name}: fragment source must be a string")
        count = fragment.get("invocations")
        if type(count) is not int or count < 0:
            raise ValueError(f"{directory.name}: fragment invocations must be non-negative")
        if fragment.get("source_kind") not in {"literal", "dynamic"}:
            raise ValueError(f"{directory.name}: fragment source kind is invalid")
        if type(fragment.get("parse_cache_should_hit")) is not bool:
            raise ValueError(f"{directory.name}: fragment cache expectation must be boolean")
        fragment["size_bytes"] = len(fragment["source"].encode())
        if count:
            kinds.add(fragment["source_kind"])
    return {**data, "eval_invocations": sum(f["invocations"] for f in fragments),
            "max_fragment_size_bytes": max(f["size_bytes"] for f in fragments),
            "literal_or_dynamic": next(iter(kinds)) if len(kinds) == 1 else "mixed" if kinds else "n/a",
            "parse_cache": "hit expected" if any(f["parse_cache_should_hit"] for f in fragments) else "no hit expected"}


def discover(suites: list[str], selected: list[str]) -> dict[str, list[dict]]:
    """Resolve filters before building so typos never launch an expensive build."""
    results = {}
    matched = set()
    for suite in suites:
        if suite not in SUITES:
            raise ValueError(f"unknown suite: {suite}")
        cases = []
        for directory in sorted((ROOT / "cases" / suite).iterdir()):
            if not directory.is_dir():
                continue
            names = {directory.name, f"{suite}/{directory.name}"}
            if selected and not names.intersection(selected):
                continue
            matched.update(names.intersection(selected))
            metadata = {}
            if suite == "native":
                variants = [("elephc", "elephc", "main.php"), ("php", "php", "main.php"),
                            ("c", "c", "main.c")]
            elif suite == "magician":
                metadata = magician_metadata(directory)
                variants = [(f"elephc-{v}", "elephc", f"{v}.php") for v in ("native", "eval")]
                variants += [(f"php-{v}", "php", f"{v}.php") for v in ("native", "eval")]
            else:
                variants = [(v, "elephc", f"{v}.php") for v in ("buffer", "indexed", "assoc")]
                metadata = {"description": "Equivalent position updates with three storage representations"}
            for _, _, source in variants:
                if not (directory / source).is_file():
                    raise ValueError(f"missing fixture: {directory / source}")
            if not (directory / "expected.txt").is_file():
                raise ValueError(f"missing expected output: {directory}")
            cases.append({"name": directory.name, "directory": directory,
                          "variants": variants, "metadata": metadata})
        if cases:
            results[suite] = cases
    missing = set(selected) - matched
    if missing:
        raise ValueError("unknown benchmark case(s): " + ", ".join(sorted(missing)))
    return results
