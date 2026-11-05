#!/usr/bin/env python3
"""Generate `tests/codegen/tidy_php_src_cases.rs` from the committed php-src corpus.

Usage:
    python3 scripts/gen_php_src_tidy_cases.py

Each generated Rust `#[test]` carries its PHP source, INI/ENV arguments, relative corpus
directory, and EXPECT/EXPECTF/EXPECTREGEX assertion inline. The `.phpt` files under
`tests/php_src/tidy/` remain the provenance copy and the generator input; the test
process never parses a `.phpt` at run time.
"""

from __future__ import annotations

import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CORPUS = ROOT / "tests" / "php_src" / "tidy"
OUTPUT = ROOT / "tests" / "codegen" / "tidy_php_src_cases.rs"
STATUS = CORPUS / "phpt_status.json"
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
    """Rewrites real PHP open/close tags and inline HTML into echo statements.

    Unlike a raw `?>` search this tracks single/double quotes and comments, so a
    closing tag inside a string (023.phpt) is left alone. Leading and trailing
    HTML is emitted as `echo` before/after the merged PHP block.
    """
    out: list[str] = []
    i = 0
    n = len(source)
    in_php = False
    while i < n:
        if not in_php:
            open_at = source.find("<?php", i)
            if open_at == -1:
                literal = source[i:]
                if literal:
                    out.append("echo " + php_single_quote(literal) + ";")
                break
            literal = source[i:open_at]
            if literal:
                out.append("echo " + php_single_quote(literal) + ";")
            i = open_at + 5
            in_php = True
            continue
        start = i
        quote: str | None = None
        while i < n:
            char = source[i]
            if quote is not None:
                if char == "\\" and quote != "'":
                    i += 2
                    continue
                if char == quote:
                    quote = None
                i += 1
                continue
            if char in ("'", '"'):
                quote = char
                i += 1
                continue
            if char == "/" and i + 1 < n and source[i + 1] == "/":
                end = source.find("\n", i + 2)
                i = n if end == -1 else end
                continue
            if char == "#":
                end = source.find("\n", i + 1)
                i = n if end == -1 else end
                continue
            if char == "/" and i + 1 < n and source[i + 1] == "*":
                end = source.find("*/", i + 2)
                i = n if end == -1 else end + 2
                continue
            if char == "?" and i + 1 < n and source[i + 1] == ">":
                out.append(source[start:i])
                i += 2
                if source[i:i + 1] == "\n":
                    i += 1
                elif source[i:i + 2] == "\r\n":
                    i += 2
                in_php = False
                break
            i += 1
        else:
            out.append(source[start:])
            break
    return "<?php\n" + "".join(out)


def adapt_source(name: str, source: str) -> str:
    """Applies the documented test-only source adaptations from the corpus README."""
    if name == "012.phpt":
        source = source.replace("function dump_nodes(tidyNode $node)", "function dump_nodes($node)")
    # Rust source cannot carry invalid UTF-8. PHP heredoc interprets `\xNN` to the
    # same byte, so 020.phpt's Latin-1 heredoc survives the round trip.
    out: list[str] = []
    for char in source:
        code = ord(char)
        if 0xDC80 <= code <= 0xDCFF:
            out.append("\\x%02X" % (code - 0xDC00))
        else:
            out.append(char)
    return "".join(out)


def rust_string(text: str) -> str:
    """Returns an escaped Rust string literal for a valid-UTF-8 PHP source."""
    out = ['"']
    for char in text:
        if char == "\\":
            out.append("\\\\")
        elif char == '"':
            out.append('\\"')
        elif char == "\n":
            out.append("\\n")
        elif char == "\r":
            out.append("\\r")
        elif char == "\t":
            out.append("\\t")
        elif ord(char) < 0x20:
            out.append(f"\\u{{{ord(char):x}}}")
        else:
            out.append(char)
    out.append('"')
    return "".join(out)


def rust_bytes(text: str) -> str:
    """Returns a Rust byte-string literal preserving invalid UTF-8 bytes."""
    data = text.encode("utf-8", "surrogateescape")
    out = ['b"']
    for byte in data:
        if byte == 0x5C:
            out.append("\\\\")
        elif byte == 0x22:
            out.append('\\"')
        elif byte == 0x0A:
            out.append("\\n")
        elif byte == 0x0D:
            out.append("\\r")
        elif byte == 0x09:
            out.append("\\t")
        elif 0x20 <= byte < 0x7F:
            out.append(chr(byte))
        else:
            out.append(f"\\x{byte:02x}")
    out.append('"')
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


def read_status() -> dict[str, tuple[str, str]]:
    """Reads the committed reference status/reason ledger."""
    status: dict[str, tuple[str, str]] = {}
    if STATUS.is_file():
        for path, value in json.loads(STATUS.read_text()).items():
            status[path] = (value["status"], value.get("reason", ""))
    return status


def identifier(relative: str) -> str:
    """Builds one unique Rust test identifier from a corpus-relative path."""
    stem = str(Path(relative).with_suffix(""))
    clean = re.sub(r"[^A-Za-z0-9_]", "_", stem)
    return "test_php_src_tidy_" + clean


def main() -> None:
    """Reads every committed PHPT and writes ordinary Rust test functions."""
    status = read_status()
    tests = sorted(CORPUS.rglob("*.phpt"))
    functions: list[str] = []
    counts = {"pass": 0, "skip": 0, "xfail": 0, "other": 0}
    for test in tests:
        relative = test.relative_to(CORPUS).as_posix()
        sections = parse_phpt(test.read_text(encoding="utf-8", errors="surrogateescape"))
        source = sections.get("FILE")
        if source is None:
            source = sections.get("FILEEOF")
        if source is None:
            raise SystemExit(f"{relative}: missing FILE/FILEEOF section")
        source = adapt_source(relative, normalize_tags(source.replace("\r\n", "\n")))
        ini = rust_pairs(parse_pairs(sections.get("INI", "")))
        env = rust_pairs(parse_pairs(sections.get("ENV", "")))
        relative_dir = str(Path(relative).parent)
        if relative_dir == ".":
            relative_dir = ""
        expected_fatal = "Fatal error:" in sections.get("EXPECT", "") or "Fatal error:" in sections.get("EXPECTF", "")
        name = identifier(relative)
        if "EXPECT" in sections:
            expected = sections["EXPECT"].replace("\r\n", "\n").strip("\n")
            assertion = f"    assert_bytes(&out, {rust_bytes(expected)});"
        elif "EXPECTF" in sections:
            expected = sections["EXPECTF"].replace("\r\n", "\n").strip("\n")
            assertion = f"    assert_format(&out, {rust_bytes(expected)});"
        elif "EXPECTREGEX" in sections:
            expected = sections["EXPECTREGEX"].replace("\r\n", "\n").strip("\n")
            assertion = f"    assert_regex(&out, {rust_bytes(expected)});"
        else:
            raise SystemExit(f"{relative}: missing EXPECT/EXPECTF/EXPECTREGEX section")
        state, reason = status.get(relative, ("", ""))
        ignore = ""
        if state == "SKIP":
            ignore = f'    #[ignore = {rust_string("php-src SKIP on this platform: " + reason)}]\n'
            counts["skip"] += 1
        elif state in ("XFAIL", "FAIL", "BORK"):
            ignore = f'    #[ignore = {rust_string("known elephc divergence: " + reason)}]\n'
            counts["xfail"] += 1
        else:
            counts["pass"] += 1
        functions.append(
            f"/// Replays php-src `{relative}`.\n"
            f"#[test]\n"
            f"{ignore}"
            f"fn {name}() {{\n"
            f'    if skip_without_tidy_native("{name}") {{ return; }}\n'
            f"    let out = run_case({rust_string(source)}, {rust_string(relative_dir)}, {ini}, {env}, {str(expected_fatal).lower()});\n"
            f"{assertion}\n"
            f"}}\n"
        )

    header = """//! Purpose:
//! Generated Rust tests for the committed PHP 8.5.10 `ext/tidy` PHPT corpus.
//!
//! Called from:
//! - `cargo test` through `tests/codegen/tidy_php_src.rs`.
//!
//! Key details:
//! - Generated by `scripts/gen_php_src_tidy_cases.py`; do not hand-edit.
//! - Each test carries its PHP source and assertion inline, and the test process
//!   never parses a `.phpt` file at run time.
//! - Fixtures php-src skips on this platform, and the one known elephc divergence,
//!   are preserved as ignored cases with the original reason.

#![allow(non_snake_case)]

#[allow(unused_imports)]
use super::{assert_bytes, assert_format, assert_regex, run_case, skip_without_tidy_native};

"""
    OUTPUT.write_text(header + "\n".join(functions))
    print(
        f"wrote {OUTPUT.relative_to(ROOT)}: {len(functions)} cases "
        f"({counts['pass']} pass, {counts['skip']} skip, {counts['xfail']} known divergence)"
    )


if __name__ == "__main__":
    main()
