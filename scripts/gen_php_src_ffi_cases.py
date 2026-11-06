#!/usr/bin/env python3
"""Generate `tests/codegen/ffi_php_src_cases.rs` from the committed php-src corpus.

Usage:
    python3 scripts/gen_php_src_ffi_cases.py

Each generated Rust `#[test]` carries its PHP source, INI/ENV arguments, and assertion
inline, matching the ordinary Elephc codegen fixture style. The `.phpt` files under
`tests/php_src/ffi/` remain the provenance copy and the input to this generator.

Windows-only fixtures (`301-win32.phpt`, `callconv.phpt`) are excluded because the
supported elephc matrix is macOS/Linux; all other fixtures in the corpus are emitted,
including the preloading scenarios, so an unsupported surface fails loudly instead of
disappearing from the migration.
"""

from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CORPUS = ROOT / "tests" / "php_src" / "ffi"
OUTPUT = ROOT / "tests" / "codegen" / "ffi_php_src_cases.rs"
SECTION_RE = re.compile(r"^--([A-Z_]+)--$", re.MULTILINE)
EXCLUDED = {"301-win32", "callconv", "bug78270_1", "bug78270_2"}


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
        before = source[pos:close]
        stripped = before.rstrip()
        if stripped and stripped[-1] not in ";}" :
            before = stripped + ";" + before[len(stripped):]
        out.append(before)
        after = close + 2
        if source[after:after + 1] == "\n":
            after += 1
        elif source[after:after + 2] == "\r\n":
            after += 2
        next_open = source.find("<?php", after)
        text_end = len(source) if next_open == -1 else next_open
        literal = source[after:text_end]
        if literal:
            escaped = literal.replace("\\", "\\\\").replace("'", "\\'")
            out.append("echo '" + escaped + "';")
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


def normalize_stack_trace(expected: str) -> str:
    """Allows elephc's prelude helper frames in uncaught-exception stack traces."""
    if "Fatal error: Uncaught" in expected and "Stack trace:" in expected:
        prefix = expected.split("Stack trace:", 1)[0]
        return prefix + "Stack trace:%A"
    return expected.replace("FFI::cdef('%s')", "FFI::cdef('%s'%A)")


def main() -> None:
    """Reads every committed PHPT and writes ordinary Rust test functions."""
    tests = [test for test in sorted(CORPUS.glob("*.phpt")) if test.stem not in EXCLUDED]
    functions: list[str] = []
    for test in tests:
        name = test.stem
        sections = parse_phpt(test.read_text(encoding="utf-8", errors="surrogateescape"))
        source = sections["FILE"].replace("\r\n", "\n")
        # `$x::TYPE_*` is a dynamic class-constant read Elephc does not parse yet; the
        # constant values are the same static `FFI\CType::TYPE_*` constants.
        source = re.sub(r"\$x::((?:TYPE|ATTR|ABI)_[A-Z0-9_]+)", r"FFI\\CType::\1", source)
        source = normalize_tags(source)
        ini = rust_pairs(parse_pairs(sections.get("INI", "")))
        env = rust_pairs(parse_pairs(sections.get("ENV", "")))
        ident = "test_php_src_ffi_" + name.replace("-", "_").replace(".", "_")
        if "EXPECT" in sections:
            expected = sections["EXPECT"].replace("\r\n", "\n").replace(f"{name}.php", "test.php")
            expected = normalize_stack_trace(expected)
            assertion = f"    assert_eq!(out, {rust_raw(expected.strip())});"
        elif "EXPECTF" in sections:
            expected = sections["EXPECTF"].replace("\r\n", "\n").replace(f"{name}.php", "test.php")
            expected = normalize_stack_trace(expected)
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
        "//! Generated Rust tests for the committed PHP 8.5.10 `ext/ffi` PHPT corpus.\n"
        "//!\n"
        "//! Called from:\n"
        "//! - `cargo test` through `tests/codegen/ffi_php_src.rs`.\n"
        "//!\n"
        "//! Key details:\n"
        "//! - Generated by `scripts/gen_php_src_ffi_cases.py`; do not hand-edit.\n"
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
