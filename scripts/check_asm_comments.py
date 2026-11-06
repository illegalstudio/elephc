#!/usr/bin/env python3
"""Check inline assembly-comment alignment in elephc codegen files.

Every `emitter.instruction(...)` call must carry an inline `//` comment that
starts at column 81 (1-indexed). See "Assembly comment alignment" in
CONTRIBUTING.md for the full policy.

Usage:
    scripts/check_asm_comments.py FILE.rs [FILE.rs ...]

For a multiline call, the comment may appear on the call-closing line.
Reports every `emitter.instruction(...)` whose `//` comment is missing or
misaligned. Exits non-zero if any problem is found, so it can be used in
pre-commit hooks or CI.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

# 1-indexed column where the `//` of an inline comment must start.
COMMENT_COLUMN = 81


def instruction_comment_line(lines: list[str], start: int) -> int:
    """Return the line index that must carry the instruction comment."""
    if "//" in lines[start]:
        return start

    marker = "emitter.instruction"
    marker_offset = lines[start].index(marker) + len(marker)
    depth = 0
    started = False
    in_string = False
    in_char = False
    escaped = False
    block_comment_depth = 0

    for index in range(start, len(lines)):
        line = lines[index]
        offset = marker_offset if index == start else 0
        while offset < len(line):
            char = line[offset]
            next_char = line[offset + 1] if offset + 1 < len(line) else ""

            if block_comment_depth:
                if char == "/" and next_char == "*":
                    block_comment_depth += 1
                    offset += 2
                    continue
                if char == "*" and next_char == "/":
                    block_comment_depth -= 1
                    offset += 2
                    continue
                offset += 1
                continue

            if in_string or in_char:
                if escaped:
                    escaped = False
                elif char == "\\":
                    escaped = True
                elif (in_string and char == '"') or (in_char and char == "'"):
                    in_string = False
                    in_char = False
                offset += 1
                continue

            if char == "/" and next_char == "/":
                break
            if char == "/" and next_char == "*":
                block_comment_depth = 1
                offset += 2
                continue
            if char == '"':
                in_string = True
            elif char == "'":
                in_char = True
            elif char == "(":
                depth += 1
                started = True
            elif char == ")":
                depth -= 1
                if started and depth == 0:
                    return index
            offset += 1

    return start


def check_file(path: Path) -> list[str]:
    """Return a list of human-readable problems for a single file."""
    try:
        text = path.read_text()
    except OSError as exc:
        return [f"{path}: cannot read file ({exc})"]

    lines = text.splitlines()
    problems: list[str] = []
    for index, line in enumerate(lines):
        stripped = line.rstrip()
        if "emitter.instruction" not in stripped:
            continue
        comment_index = instruction_comment_line(lines, index)
        comment_line = lines[comment_index].rstrip()
        lineno = comment_index + 1
        if "//" not in comment_line:
            problems.append(f"{path}:{lineno}: missing inline // comment")
            continue
        pos = comment_line.index("//")  # 0-indexed position of the comment
        # The `//` must sit at column 81 (index 80). Lines whose code already
        # reaches 80 characters may use a single space before `//` instead, so
        # they are exempt from the column check.
        if pos != COMMENT_COLUMN - 1 and len(comment_line[:pos].rstrip()) < 80:
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
