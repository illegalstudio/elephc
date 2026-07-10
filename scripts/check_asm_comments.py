#!/usr/bin/env python3
"""Check inline assembly-comment alignment in elephc codegen files.

Every `emitter.instruction(...)` call must carry an inline `//` comment that
starts at column 81 (1-indexed). See "Assembly comment alignment" in
CONTRIBUTING.md for the full policy.

Usage:
    scripts/check_asm_comments.py FILE.rs [FILE.rs ...]

Reports every `emitter.instruction(...)` whose `//` comment is misaligned, as
`path:line: // at col N (expected 81)`. Exits non-zero if any problem is found,
so it can be used in pre-commit hooks or CI.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

# 1-indexed column where the `//` of an inline comment must start.
COMMENT_COLUMN = 81


def check_file(path: Path) -> list[str]:
    """Return a list of human-readable problems for a single file."""
    try:
        text = path.read_text()
    except OSError as exc:
        return [f"{path}: cannot read file ({exc})"]

    problems: list[str] = []
    for lineno, line in enumerate(text.splitlines(), 1):
        stripped = line.rstrip()
        if "emitter.instruction" not in stripped or "//" not in stripped:
            continue
        pos = stripped.index("//")  # 0-indexed position of the comment
        # The `//` must sit at column 81 (index 80). Lines whose code already
        # reaches 80 characters may use a single space before `//` instead, so
        # they are exempt from the column check.
        if pos != COMMENT_COLUMN - 1 and len(line[:pos].rstrip()) < 80:
            problems.append(f"{path}:{lineno}: // at col {pos + 1} (expected {COMMENT_COLUMN})")
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("files", nargs="+", type=Path, help="Rust codegen files to check")
    args = parser.parse_args()

    problems: list[str] = []
    for path in args.files:
        problems.extend(check_file(path))

    for problem in problems:
        print(problem)

    if problems:
        print(f"\n{len(problems)} misaligned comment(s) found.", file=sys.stderr)
        return 1

    print("All assembly comments aligned.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
