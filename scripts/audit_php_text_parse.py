#!/usr/bin/env python3
"""Ratchet against PHP source text reaching elephc's parser on a production path.

Every PHP surface elephc provides is built with the AST builders (AGENTS.md, "PHP
surfaces are built as AST, never parsed from PHP text"). Text handed to the lexer at
compile time is paid on every invocation of the compiler and is opaque to every static
pass, so the remaining sites are legacy: bounded, listed, and shrinking.

This script is the ratchet. It finds, outside test code, every PHP source literal and
every call into the lexer/parser, and compares the set against the recorded legacy
allowlist. A NEW site fails the audit. A legacy site that has been converted also fails,
because the allowlist must shrink with it -- an entry nobody removes is an entry that
lets the site come back.

    python3 scripts/audit_php_text_parse.py            # report
    python3 scripts/audit_php_text_parse.py --enforce  # exit 1 on any drift
    python3 scripts/audit_php_text_parse.py --write    # re-record the allowlist

Test code is exempt: probes, fixtures and oracle inputs are PHP by nature. "Test code"
means a path under a `tests/` directory, a `#[cfg(test)]` module or function, or a file
whose parsed literal only exists behind that attribute -- which is also what makes a
converted surface self-enforcing, since the constant then does not exist in a release
build.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
ALLOWLIST_PATH = REPO_ROOT / "scripts" / "php_text_parse_allowlist.json"

# Where compiler-owned Rust lives. Bridge crates are included: a prelude can hide
# anywhere, and the 2026-08-14 campaign found one behind an `r##"` literal.
SOURCE_ROOTS = [REPO_ROOT / "src"] + sorted(
    p for p in (REPO_ROOT / "crates").glob("*/src") if p.is_dir()
)

# A call into the compiler's own lexer or parser. The lexer has two entry points, so
# enumerating these is exhaustive rather than indicative.
PARSER_CALL = re.compile(r"\b(tokenize|parse_internal|parse_program)\s*\(")

# PHP source text. Both raw-literal widths are checked on purpose: `r##"` once hid 369
# lines from a `r#"`-only sweep.
PHP_TEXT = re.compile(r"<\?php")

CFG_TEST = re.compile(r"#\[cfg\(test\)\]|#\[cfg\(any\([^)]*test[^)]*\)\)\]")

# A module declared `#[cfg(test)] mod name;` lives in its own file, so the gate is in the
# PARENT. Without following it, a whole test-only module reads as production code --
# src/synthetic_class/transcribe.rs, the PHP-to-AST transcriber, is exactly that shape.
CFG_TEST_MOD = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;")

# The lexer and the parser mention PHP syntax because parsing PHP is their job; they
# provide no PHP surface. Excluded by path so the allowlist keeps naming only real
# surfaces -- if a prelude ever hides here, it belongs somewhere else anyway.
EXEMPT_DIR_PARTS = frozenset({"lexer", "parser"})


@dataclass(frozen=True, order=True)
class Site:
    path: str
    line: int
    kind: str
    excerpt: str

    def key(self) -> str:
        return f"{self.path}:{self.line}:{self.kind}"


def is_test_path(path: Path) -> bool:
    parts = path.parts
    if "tests" in parts or path.name == "tests.rs" or path.name.endswith("_tests.rs"):
        return True
    return bool(EXEMPT_DIR_PARTS.intersection(parts))


def test_only_modules() -> set[Path]:
    """Files whose whole module is gated by a `#[cfg(test)] mod name;` in its parent."""
    gated: set[Path] = set()
    for root in SOURCE_ROOTS:
        for path in root.rglob("*.rs"):
            try:
                lines = path.read_text(encoding="utf-8").splitlines()
            except (UnicodeDecodeError, OSError):
                continue
            for index, line in enumerate(lines):
                match = CFG_TEST_MOD.match(line)
                if not match or index == 0:
                    continue
                # The attribute sits on the line above, or on the ones above it.
                back = index - 1
                while back >= 0 and lines[back].strip().startswith("#["):
                    if CFG_TEST.search(lines[back]):
                        name = match.group(1)
                        parent = path.parent
                        stem = path.stem
                        for candidate in (
                            parent / f"{name}.rs",
                            parent / name / "mod.rs",
                            parent / stem / f"{name}.rs",
                            parent / stem / name / "mod.rs",
                        ):
                            if candidate.exists():
                                gated.add(candidate.resolve())
                        break
                    back -= 1
    return gated


def test_gated_lines(text: str) -> set[int]:
    """Line numbers that sit inside a `#[cfg(test)]` item.

    Brace counting rather than a Rust parse: the attribute is followed by an item whose
    body is balanced, so tracking depth from the item's first `{` to its matching `}`
    covers the module or function exactly. Attributes on a `use` or a `const` (no body)
    gate only their own statement.
    """
    gated: set[int] = set()
    lines = text.splitlines()
    index = 0
    while index < len(lines):
        if not CFG_TEST.search(lines[index]):
            index += 1
            continue
        # Walk forward to the item's opening brace, if it has one.
        cursor = index + 1
        while cursor < len(lines) and "{" not in lines[cursor]:
            if ";" in lines[cursor]:  # attribute on a bodyless item
                break
            cursor += 1
        if cursor >= len(lines) or "{" not in lines[cursor]:
            for gated_line in range(index, min(cursor + 1, len(lines))):
                gated.add(gated_line + 1)
            index = cursor + 1
            continue
        depth = 0
        end = cursor
        for scan in range(cursor, len(lines)):
            depth += lines[scan].count("{") - lines[scan].count("}")
            end = scan
            if depth <= 0:
                break
        for gated_line in range(index, end + 1):
            gated.add(gated_line + 1)
        index = end + 1
    return gated


def scan_file(path: Path) -> list[Site]:
    try:
        text = path.read_text(encoding="utf-8")
    except (UnicodeDecodeError, OSError):
        return []
    if "<?php" not in text and not PARSER_CALL.search(text):
        return []
    gated = test_gated_lines(text)
    rel = path.relative_to(REPO_ROOT).as_posix()
    found: list[Site] = []
    for number, line in enumerate(text.splitlines(), start=1):
        if number in gated:
            continue
        stripped = line.strip()
        if stripped.startswith("//") or stripped.startswith("*"):
            continue  # prose about the rule is not a violation of it
        if PHP_TEXT.search(line):
            found.append(Site(rel, number, "php-text", stripped[:120]))
        elif PARSER_CALL.search(line):
            found.append(Site(rel, number, "parser-call", stripped[:120]))
    return found


def scan_repo() -> list[Site]:
    sites: list[Site] = []
    gated_modules = test_only_modules()
    for root in SOURCE_ROOTS:
        for path in sorted(root.rglob("*.rs")):
            if is_test_path(path) or path.resolve() in gated_modules:
                continue
            sites.extend(scan_file(path))
    return sorted(sites)


def load_allowlist() -> dict[str, str]:
    if not ALLOWLIST_PATH.exists():
        return {}
    return json.loads(ALLOWLIST_PATH.read_text(encoding="utf-8")).get("sites", {})


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--enforce", action="store_true", help="exit 1 on any drift")
    parser.add_argument("--write", action="store_true", help="re-record the allowlist")
    args = parser.parse_args()

    sites = scan_repo()
    current = {site.key(): site.excerpt for site in sites}

    if args.write:
        ALLOWLIST_PATH.write_text(
            json.dumps(
                {
                    "_comment": (
                        "Legacy sites that still hand PHP source text to elephc's parser on a "
                        "production path. See AGENTS.md, 'PHP surfaces are built as AST, never "
                        "parsed from PHP text'. This list may only SHRINK: convert a site, then "
                        "re-record with --write in the same commit."
                    ),
                    "sites": current,
                },
                indent=2,
                sort_keys=True,
            )
            + "\n",
            encoding="utf-8",
        )
        print(f"recorded {len(current)} site(s) to {ALLOWLIST_PATH.relative_to(REPO_ROOT)}")
        return 0

    allowed = load_allowlist()
    added = sorted(set(current) - set(allowed))
    removed = sorted(set(allowed) - set(current))

    print(f"php-text/parser sites outside tests: {len(current)} (allowlisted: {len(allowed)})")
    for key in added:
        print(f"  NEW      {key}\n           {current[key]}")
    for key in removed:
        print(f"  GONE     {key}  (converted -- re-record with --write)")

    if not added and not removed:
        print("no drift")
        return 0
    if added:
        print(
            "\nA new PHP surface is being parsed from text. Build it with the AST builders "
            "instead -- see AGENTS.md, 'PHP surfaces are built as AST, never parsed from PHP "
            "text', and src/web_prelude/build.rs for the reference shape."
        )
    if removed and not added:
        print("\nA legacy site was converted. Re-record the allowlist in the same commit.")
    return 1 if args.enforce else 0


if __name__ == "__main__":
    sys.exit(main())
