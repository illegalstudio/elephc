#!/usr/bin/env python3
"""Generate `tests/codegen/fileinfo_php_src_cases.rs` from the committed php-src corpus.

Usage:
    python3 scripts/gen_php_src_fileinfo_cases.py

Each generated Rust `#[test]` carries its PHP source, INI/ENV arguments, and assertion
inline, matching the ordinary Elephc codegen fixture style. The `.phpt` files under
`tests/php_src/fileinfo/` remain the provenance copy and the input to this generator.
"""

from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CORPUS = ROOT / "tests" / "php_src" / "fileinfo"
OUTPUT = ROOT / "tests" / "codegen" / "fileinfo_php_src_cases.rs"
SECTION_RE = re.compile(r"^--([A-Z_]+)--$", re.MULTILINE)


def parse_phpt(text: str) -> dict[str, str]:
    """Splits a `.phpt` file into its named sections."""
    matches = list(SECTION_RE.finditer(text))
    sections: dict[str, str] = {}
    for index, match in enumerate(matches):
        start = match.end()
        if text[start:start + 1] == "\n":
            start += 1
        end = matches[index + 1].start() if index + 1 < len(matches) else len(text)
        sections[match.group(1)] = text[start:end]
    return sections


def php_single_quote(text: str) -> str:
    """Encodes literal inline output as a PHP single-quoted string."""
    return "'" + text.replace("\\", "\\\\").replace("'", "\\'") + "'"


def normalize_tags(source: str) -> str:
    """Rewrites closing tags and inline HTML into equivalent `echo` statements."""
    if "?>" not in source:
        return source
    out: list[str] = []
    pos = 0
    while True:
        close = source.find("?>", pos)
        if close == -1:
            out.append(source[pos:])
            break
        out.append(source[pos:close])
        after = close + 2
        if source[after:after + 1] == "\n":
            after += 1
        elif source[after:after + 2] == "\r\n":
            after += 2
        next_open = source.find("<?php", after)
        text_end = len(source) if next_open == -1 else next_open
        literal = source[after:text_end]
        if literal:
            out.append("echo " + php_single_quote(literal) + ";")
        if next_open == -1:
            break
        pos = next_open
    return "".join(out)


def rust_raw(text: str) -> str:
    """Returns a Rust raw string literal with a delimiter absent from `text`."""
    hashes = 1
    while ('"' + "#" * hashes) in text:
        hashes += 1
    marker = "#" * hashes
    return f'r{marker}"{text}"{marker}'


def parse_pairs(section: str) -> list[tuple[str, str]]:
    """Parses INI/ENV assignments into key/value pairs."""
    pairs = []
    for line in section.splitlines():
        line = line.strip()
        if not line or line.startswith(";") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        pairs.append((key.strip(), value.strip()))
    return pairs


def rust_pairs(pairs: list[tuple[str, str]]) -> str:
    """Renders an inline array of string pairs."""
    if not pairs:
        return "&[]"
    entries = ", ".join(f"({rust_raw(key)}, {rust_raw(value)})" for key, value in pairs)
    return f"&[{entries}]"


def main() -> None:
    """Reads every committed PHPT and writes ordinary Rust test functions."""
    tests = sorted(CORPUS.glob("*.phpt"))
    functions: list[str] = []
    for test in tests:
        name = test.stem
        sections = parse_phpt(test.read_text(encoding="utf-8", errors="surrogateescape"))
        source = sections["FILE"].replace("\r\n", "\n")
        source = normalize_tags(source)
        if name == "finfo_phpinfo_basic":
            # The fixture only checks a literal substring. Avoid exercising the dynamic
            # regex runtime here; the phpinfo table is the subject under test.
            source = source.replace(
                'var_dump(preg_match("/fileinfo support => enabled/", $output));',
                'var_dump(str_contains($output, "fileinfo support => enabled") ? 1 : 0);',
            )
        ini = rust_pairs(parse_pairs(sections.get("INI", "")))
        env = rust_pairs(parse_pairs(sections.get("ENV", "")))
        ident = "test_php_src_fileinfo_" + name.replace("-", "_").replace(".", "_")
        if "EXPECT" in sections:
            expected = sections["EXPECT"].replace("\r\n", "\n").replace(f"{name}.php", "test.php")
            assertion = f"    assert_eq!(out, {rust_raw(expected.strip())});"
        elif "EXPECTF" in sections:
            expected = sections["EXPECTF"].replace("\r\n", "\n").replace(f"{name}.php", "test.php")
            assertion = f"    assert_format(&out, {rust_raw(expected.strip())});"
        else:
            raise SystemExit(f"{name}: missing EXPECT/EXPECTF section")
        functions.append(
            f"/// Replays php-src `{name}.phpt`.\n"
            "#[test]\n"
            f"fn {ident}() {{\n"
            f"    let out = run_case({rust_raw(source)}, {ini}, {env});\n"
            f"{assertion}\n"
            "}"
        )
    output = (
        "//! Purpose:\n"
        "//! Generated Rust tests for the committed PHP 8.5.10 `ext/fileinfo` PHPT corpus.\n"
        "//!\n"
        "//! Called from:\n"
        "//! - `cargo test` through `tests/codegen/fileinfo_php_src.rs`.\n"
        "//!\n"
        "//! Key details:\n"
        "//! - Generated by `scripts/gen_php_src_fileinfo_cases.py`; do not hand-edit.\n"
        "//! - Each test carries its PHP source and assertion inline, and the test process\n"
        "//!   never parses a `.phpt` file at run time.\n"
        "\n"
        "use super::{assert_format, run_case};\n\n"
        + "\n\n".join(functions)
        + "\n"
    )
    OUTPUT.write_text(output, encoding="utf-8")
    print(f"wrote {len(tests)} inline tests to {OUTPUT}")


if __name__ == "__main__":
    main()
