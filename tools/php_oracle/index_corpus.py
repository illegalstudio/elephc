#!/usr/bin/env python3
"""Generate the checked-in Gate 0 stream oracle case-source index."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import tempfile
from pathlib import Path
from typing import Any

SCHEMA_VERSION = 1
ROOT = Path(__file__).resolve().parents[2]
CASE_ROOT = ROOT / "tests" / "php_oracle" / "cases" / "streams" / "gate0"
DEFAULT_OUTPUT = ROOT / "tests" / "php_oracle" / "corpora" / "streams" / "gate0.json"


def parse_args() -> argparse.Namespace:
    """Parse corpus-index generation or byte-check arguments."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    action = parser.add_mutually_exclusive_group(required=True)
    action.add_argument("--write", action="store_true")
    action.add_argument("--check", action="store_true")
    return parser.parse_args()


def canonical_bytes(value: Any) -> bytes:
    """Serialize deterministic UTF-8 JSON with sorted keys and a final LF."""
    return (
        json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + "\n"
    ).encode()


def sha256_bytes(content: bytes) -> str:
    """Return the lowercase SHA-256 digest for arbitrary bytes."""
    return hashlib.sha256(content).hexdigest()


def case_source_digest(case_dir: Path) -> str:
    """Hash every case input in relative-path/content order."""
    digest = hashlib.sha256()
    for path in sorted(item for item in case_dir.rglob("*") if item.is_file()):
        relative = path.relative_to(case_dir).as_posix().encode()
        content = path.read_bytes()
        digest.update(len(relative).to_bytes(8, "big"))
        digest.update(relative)
        digest.update(len(content).to_bytes(8, "big"))
        digest.update(content)
    return digest.hexdigest()


def build_index() -> dict[str, Any]:
    """Inventory every Gate 0 case and the observable channels it exercises."""
    cases = []
    for case_file in sorted(CASE_ROOT.glob("*/case.json")):
        case_dir = case_file.parent
        case = json.loads(case_file.read_bytes())
        if case.get("schema_version") != SCHEMA_VERSION:
            raise SystemExit(f"unsupported case schema: {case_file}")
        cases.append(
            {
                "id": case["id"],
                "source": case_dir.relative_to(ROOT).as_posix(),
                "source_sha256": case_source_digest(case_dir),
                "description": case.get("description", ""),
                "dependencies": case.get("dependencies", []),
                "normalization": case.get("normalization", []),
            }
        )
    if not cases:
        raise SystemExit(f"no cases found under {CASE_ROOT}")
    return {
        "schema_version": SCHEMA_VERSION,
        "kind": "php-src-stream-oracle-corpus-index",
        "gate": {"number": 0, "status": "candidate"},
        "capture_contract": {
            "runs": ["raw-unchanged", "instrumented-fresh-sandbox"],
            "channels": [
                "stdout-bytes",
                "stderr-bytes",
                "exit-code",
                "signal",
                "timeout",
                "ordered-diagnostics",
                "exception",
                "return-value",
                "reference-observations",
                "filesystem-diff",
            ],
            "binary_encoding": ["base64", "length", "sha256"],
        },
        "cases": cases,
        "generator": {
            "script": Path(__file__).relative_to(ROOT).as_posix(),
            "script_sha256": sha256_bytes(Path(__file__).read_bytes()),
        },
    }


def atomic_write(path: Path, content: bytes) -> None:
    """Replace one generated corpus index atomically."""
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(content)
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def main() -> int:
    """Generate or byte-check the Gate 0 corpus index."""
    args = parse_args()
    content = canonical_bytes(build_index())
    if args.check:
        if not args.output.exists():
            print(f"missing corpus index: {args.output}", file=sys.stderr)
            return 1
        if args.output.read_bytes() != content:
            print(f"corpus index drift: regenerate {args.output}", file=sys.stderr)
            return 1
        return 0
    atomic_write(args.output, content)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
