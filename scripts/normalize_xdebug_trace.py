#!/usr/bin/env python3
"""Normalize an Xdebug format-4 function trace into stable execution ledgers.

The source program is deliberately opaque to this tool. It records the PHP
call graph observed by an Xdebug trace without identifying a framework,
package manager, or project layout, so compiler compatibility work can begin
from evidence rather than a hand-maintained list of APIs.
"""

from __future__ import annotations

import argparse
import csv
import gzip
import json
from collections import Counter
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import TextIO


@dataclass(frozen=True)
class CallEvent:
    """One normalized function-entry record with its immediate caller."""

    order: int
    depth: int
    caller: str
    callee: str
    kind: str
    source_file: str
    source_line: str
    include_target: str


def open_trace(path: Path) -> TextIO:
    """Open a plain or gzip-compressed Xdebug trace as UTF-8 replacement text."""

    if path.suffix == ".gz":
        return gzip.open(path, "rt", encoding="utf-8", errors="replace")
    return path.open("r", encoding="utf-8", errors="replace")


def classify(callee: str, is_user_defined: str) -> str:
    """Classify a trace target using only Xdebug's PHP-level entry metadata."""

    if callee in {"include", "include_once", "require", "require_once"}:
        return "include"
    if callee.startswith("Reflection"):
        return "reflection"
    if callee.endswith("->__construct"):
        return "constructor"
    if "::" in callee:
        return "static_method"
    if "->" in callee:
        return "instance_method"
    if is_user_defined == "0":
        return "builtin"
    return "function"


def parse_trace(path: Path) -> list[CallEvent]:
    """Parse Xdebug format-4 entry records while reconstructing parent edges."""

    stack: dict[int, str] = {}
    events: list[CallEvent] = []

    with open_trace(path) as trace:
        for raw in trace:
            fields = raw.rstrip("\n").split("\t")
            if len(fields) < 10 or fields[2] != "0":
                continue
            try:
                depth = int(fields[0])
            except ValueError:
                continue

            callee = fields[5]
            kind = classify(callee, fields[6])
            parent = stack.get(depth - 1, "<root>")
            event = CallEvent(
                order=len(events) + 1,
                depth=depth,
                caller=parent,
                callee=callee,
                kind=kind,
                source_file=fields[8],
                source_line=fields[9],
                include_target=fields[7] if kind == "include" else "",
            )
            events.append(event)
            stack[depth] = callee
            for child_depth in [known_depth for known_depth in stack if known_depth > depth]:
                del stack[child_depth]

    if not events:
        raise ValueError(f"{path}: no Xdebug format-4 function-entry records found")
    return events


def write_tsv(path: Path, fieldnames: list[str], rows: list[dict[str, object]]) -> None:
    """Write deterministic tab-separated rows with a header into ``path``."""

    with path.open("w", encoding="utf-8", newline="") as output:
        writer = csv.DictWriter(output, fieldnames=fieldnames, delimiter="\t", lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)


def write_ledgers(events: list[CallEvent], output_dir: Path) -> None:
    """Emit event, aggregate-edge, symbol, inclusion, and summary ledger files."""

    output_dir.mkdir(parents=True, exist_ok=True)
    write_tsv(
        output_dir / "events.tsv",
        list(CallEvent.__dataclass_fields__),
        [asdict(event) for event in events],
    )

    edge_counts: Counter[tuple[str, str, str, str, str]] = Counter(
        (event.caller, event.callee, event.kind, event.source_file, event.source_line) for event in events
    )
    edge_rows = [
        {
            "count": count,
            "caller": caller,
            "callee": callee,
            "kind": kind,
            "source_file": source_file,
            "source_line": source_line,
        }
        for (caller, callee, kind, source_file, source_line), count in edge_counts.items()
    ]
    edge_rows.sort(
        key=lambda row: (-int(row["count"]), str(row["caller"]), str(row["callee"]), str(row["source_file"]))
    )
    write_tsv(
        output_dir / "edges.tsv",
        ["count", "caller", "callee", "kind", "source_file", "source_line"],
        edge_rows,
    )

    symbol_counts = Counter(event.callee for event in events)
    symbol_kinds = {event.callee: event.kind for event in events}
    symbol_rows = [
        {"count": count, "kind": symbol_kinds[symbol], "symbol": symbol}
        for symbol, count in symbol_counts.items()
    ]
    symbol_rows.sort(key=lambda row: (-int(row["count"]), str(row["symbol"])))
    write_tsv(output_dir / "symbols.tsv", ["count", "kind", "symbol"], symbol_rows)

    include_counts = Counter(event.include_target for event in events if event.include_target)
    include_rows = [
        {"count": count, "target": target} for target, count in sorted(include_counts.items())
    ]
    include_rows.sort(key=lambda row: (-int(row["count"]), str(row["target"])))
    write_tsv(output_dir / "includes.tsv", ["count", "target"], include_rows)

    kind_counts = Counter(event.kind for event in events)
    summary = {
        "entry_events": len(events),
        "distinct_symbols": len(symbol_counts),
        "distinct_edges": len(edge_counts),
        "distinct_include_targets": len(include_counts),
        "events_by_kind": dict(sorted(kind_counts.items())),
    }
    (output_dir / "summary.json").write_text(
        json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )


def main() -> int:
    """Parse command-line inputs and write normalized trace ledgers."""

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("trace", type=Path, help="Xdebug format-4 trace, optionally gzip-compressed")
    parser.add_argument("--output-dir", required=True, type=Path, help="directory receiving TSV and JSON ledgers")
    args = parser.parse_args()

    try:
        events = parse_trace(args.trace)
    except (OSError, ValueError) as exc:
        parser.error(str(exc))
    write_ledgers(events, args.output_dir)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
