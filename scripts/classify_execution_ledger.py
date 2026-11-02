#!/usr/bin/env python3
"""Join a native PHP execution ledger to generic static compiler evidence.

The tool deliberately knows neither an application nor a package layout. It compares
the symbols observed in an Xdebug-derived ledger with the shared builtin contract and
the symbol labels emitted by a previously compiled executable. A matching label proves
only static retention, not runtime parity; callers must still reduce divergent edges to
PHP-oracle tests.
"""

from __future__ import annotations

import argparse
import csv
import json
import re
from collections import Counter
from pathlib import Path


METHOD_LABEL_PREFIXES = ("_method___", "_static___")


def normalize_symbol(symbol: str) -> str:
    """Canonicalize PHP trace spelling for case-insensitive symbol comparison."""

    return symbol.replace("->", "::").lstrip("\\").lower()


def decode_label_fragment(fragment: str) -> str:
    """Decode the stable namespace and underscore escapes used in assembly labels."""

    return fragment.replace("_N_", "\\").replace("_u_", "_")


def static_symbols_from_assembly(path: Path) -> set[str]:
    """Read emitted user-function labels without loading the assembly file into memory."""

    symbols: set[str] = set()
    with path.open("r", encoding="utf-8", errors="replace") as assembly:
        for raw_line in assembly:
            label = raw_line.strip().removesuffix(":")
            if raw_line == "" or not raw_line.endswith(":\n"):
                continue
            if label.startswith(METHOD_LABEL_PREFIXES) and not label.endswith("_epilogue"):
                encoded = label.split("___", 1)[1]
                class_name, method = encoded.split("___", 1)
                symbols.add(normalize_symbol(f"{decode_label_fragment(class_name)}::{decode_label_fragment(method)}"))
            elif label.startswith("_fn_") and not label.endswith("_epilogue"):
                symbols.add(normalize_symbol(decode_label_fragment(label.removeprefix("_fn_"))))
    return symbols


def builtin_contracts(path: Path) -> dict[str, tuple[bool, bool]]:
    """Load AOT and dynamic-evaluation availability from the shared builtin registry."""

    entries = json.loads(path.read_text(encoding="utf-8"))
    return {
        normalize_symbol(entry["canonical_name"]): (
            bool(entry["aot"]["supported"]),
            bool(entry["eval"]["supported"]),
        )
        for entry in entries
    }


def symbols_from_prelude_sources(paths: list[Path]) -> set[str]:
    """Extract plain PHP function declarations from compiler-mode prelude sources."""

    pattern = re.compile(r"^function\s+&?\s*([A-Za-z_][A-Za-z0-9_]*)\s*\(", re.MULTILINE)
    symbols: set[str] = set()
    for path in paths:
        symbols.update(normalize_symbol(match.group(1)) for match in pattern.finditer(path.read_text(encoding="utf-8")))
    return symbols


def classify_symbol(
    kind: str,
    symbol: str,
    static_symbols: set[str],
    contracts: dict[str, tuple[bool, bool]],
    mode_prelude_symbols: set[str],
) -> tuple[str, bool, bool, bool]:
    """Classify one observed symbol using its trace kind and static compiler evidence."""

    if kind == "builtin":
        aot, dynamic = contracts.get(normalize_symbol(symbol), (False, False))
        mode_prelude = normalize_symbol(symbol) in mode_prelude_symbols
        if mode_prelude and aot and dynamic:
            return "mode_prelude_and_contract", aot, dynamic, mode_prelude
        if mode_prelude:
            return "mode_prelude_aot_only", aot, dynamic, mode_prelude
        if aot and dynamic:
            return "contract_aot_and_dynamic", aot, dynamic, mode_prelude
        if aot:
            return "contract_aot_only", aot, dynamic, mode_prelude
        if dynamic:
            return "contract_dynamic_only", aot, dynamic, mode_prelude
        return "missing_shared_contract", aot, dynamic, mode_prelude
    if kind == "include":
        return "resolver_provenance_required", False, False, False
    if kind == "reflection":
        return "reflection_runtime_provenance_required", False, False, False
    if normalize_symbol(symbol) in static_symbols:
        return "static_symbol_retained", False, False, False
    return "static_symbol_not_retained", False, False, False


def read_tsv(path: Path) -> list[dict[str, str]]:
    """Read a tab-separated ledger while preserving its deterministic row order."""

    with path.open("r", encoding="utf-8", newline="") as source:
        return list(csv.DictReader(source, delimiter="\t"))


def write_tsv(path: Path, fieldnames: list[str], rows: list[dict[str, object]]) -> None:
    """Write deterministic tab-separated analysis output with one header row."""

    with path.open("w", encoding="utf-8", newline="") as destination:
        writer = csv.DictWriter(destination, fieldnames=fieldnames, delimiter="\t", lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)


def build_ledger(
    symbols_path: Path,
    edges_path: Path,
    assembly_path: Path,
    builtin_registry_path: Path,
    output_dir: Path,
    prelude_sources: list[Path] | None = None,
) -> dict[str, int]:
    """Classify observed symbols and edges, then return the status-count summary."""

    output_dir.mkdir(parents=True, exist_ok=True)
    static_symbols = static_symbols_from_assembly(assembly_path)
    contracts = builtin_contracts(builtin_registry_path)
    mode_prelude_symbols = symbols_from_prelude_sources(prelude_sources or [])
    symbol_rows = read_tsv(symbols_path)
    classifications: dict[str, tuple[str, bool, bool, bool]] = {}
    classified_symbols: list[dict[str, object]] = []
    for row in symbol_rows:
        kind = row["kind"]
        symbol = row["symbol"]
        status, aot, dynamic, mode_prelude = classify_symbol(
            kind,
            symbol,
            static_symbols,
            contracts,
            mode_prelude_symbols,
        )
        classifications[symbol] = (status, aot, dynamic, mode_prelude)
        classified_symbols.append(
            {
                **row,
                "status": status,
                "aot_contract": str(aot).lower(),
                "dynamic_contract": str(dynamic).lower(),
                "mode_prelude": str(mode_prelude).lower(),
            }
        )
    write_tsv(
        output_dir / "symbols.tsv",
        ["count", "kind", "symbol", "status", "aot_contract", "dynamic_contract", "mode_prelude"],
        classified_symbols,
    )

    classified_edges: list[dict[str, object]] = []
    for row in read_tsv(edges_path):
        status, aot, dynamic, mode_prelude = classifications[row["callee"]]
        classified_edges.append(
            {
                **row,
                "status": status,
                "aot_contract": str(aot).lower(),
                "dynamic_contract": str(dynamic).lower(),
                "mode_prelude": str(mode_prelude).lower(),
            }
        )
    write_tsv(
        output_dir / "edges.tsv",
        [
            "count",
            "caller",
            "callee",
            "kind",
            "source_file",
            "source_line",
            "status",
            "aot_contract",
            "dynamic_contract",
            "mode_prelude",
        ],
        classified_edges,
    )
    summary = dict(sorted(Counter(row["status"] for row in classified_symbols).items()))
    (output_dir / "summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return summary


def main() -> int:
    """Parse command-line paths and materialize the static-evidence ledger."""

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--symbols", required=True, type=Path, help="native symbols.tsv from the execution ledger")
    parser.add_argument("--edges", required=True, type=Path, help="native edges.tsv from the execution ledger")
    parser.add_argument("--assembly", required=True, type=Path, help="emitted assembly from the matching compiler build")
    parser.add_argument("--builtin-registry", required=True, type=Path, help="shared builtin_registry.json")
    parser.add_argument(
        "--prelude-source",
        action="append",
        type=Path,
        default=[],
        help="compiler-mode PHP prelude source to classify as mode-provided (repeatable)",
    )
    parser.add_argument("--output-dir", required=True, type=Path, help="directory for classified ledger TSV and JSON")
    args = parser.parse_args()
    build_ledger(
        args.symbols,
        args.edges,
        args.assembly,
        args.builtin_registry,
        args.output_dir,
        args.prelude_source,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
