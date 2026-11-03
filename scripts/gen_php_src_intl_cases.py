#!/usr/bin/env python3
"""Generate `tests/codegen/intl_php_src_cases.rs` from the committed php-src corpus.

Usage:
    python3 scripts/gen_php_src_intl_cases.py

Each generated Rust `#[test]` carries its PHP source, INI/ENV arguments, relative corpus
directory, and assertion inline. The `.phpt` files under `tests/php_src/intl/` remain the
provenance copy and the input to this generator; the test process never parses a `.phpt`.
"""

from __future__ import annotations

import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CORPUS = ROOT / "tests" / "php_src" / "intl"
OUTPUT = ROOT / "tests" / "codegen" / "intl_php_src_cases.rs"
STATUS = CORPUS / "phpt_status.json"
SECTION_RE = re.compile(r"^--([A-Z_]+)--$", re.MULTILINE)
ANSI_RE = re.compile(r"\x1b\[[0-9;]*m")


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


def rust_string(text: str) -> str:
    """Returns an escaped Rust string literal.

    Raw literals keep fixture trailing whitespace at the end of a Rust source line, which
    `git diff --check` rejects. Escaped literals keep the spaces mid-line and preserve the
    exact bytes.
    """
    out = ['"']
    for ch in text:
        if ch == "\\":
            out.append("\\\\")
        elif ch == '"':
            out.append('\\"')
        elif ch == "\n":
            out.append("\\n")
        elif ch == "\r":
            out.append("\\r")
        elif ch == "\t":
            out.append("\\t")
        elif ord(ch) < 0x20:
            out.append(f"\\u{{{ord(ch):x}}}")
        else:
            out.append(ch)
    out.append('"')
    return "".join(out)


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
    """Reads the committed PHP-run status/reason ledger."""
    status: dict[str, tuple[str, str]] = {}
    if STATUS.is_file():
        for path, value in json.loads(STATUS.read_text()).items():
            status[path] = (value["status"], value.get("reason", ""))
    return status


def identifier(relative: str) -> str:
    """Builds one unique Rust test identifier from a corpus-relative path."""
    stem = str(Path(relative).with_suffix(""))
    clean = re.sub(r"[^A-Za-z0-9_]", "_", stem)
    return "test_php_src_intl_" + clean


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
        source = normalize_tags(source.replace("\r\n", "\n"))
        # `ini_set`/`ini_restore` for the one Intl directive are the same operation as the
        # public default-locale API; modelling them directly keeps the fixture independent of
        # the OPcache INI prelude that an ordinary compiled program does not inject.
        source = re.sub(
            r"ini_set\(\s*['\"]intl\.default_locale['\"]\s*,\s*([^)]+)\)",
            r"locale_set_default(\1)",
            source,
        )
        source = re.sub(
            r"ini_restore\(\s*['\"]intl\.default_locale['\"]\s*\)",
            "locale_set_default('')",
            source,
        )
        source = re.sub(
            r"ini_get\(\s*['\"]intl\.default_locale['\"]\s*\)",
            "locale_get_default()",
            source,
        )
        # php-src's lookup/filter helpers pass `false` as an empty language range.
        # Elephc does not weakly coerce bool to a user function's string parameter.
        rewritten = []
        for line in source.splitlines(keepends=True):
            if "locale_lookup(" in line:
                line = line.replace(", false,", ", '',", 1)
            rewritten.append(line)
        source = "".join(rewritten)
        ini = rust_pairs(parse_pairs(sections.get("INI", "")))
        env = rust_pairs(parse_pairs(sections.get("ENV", "")))
        relative_dir = str(Path(relative).parent)
        if relative_dir == ".":
            relative_dir = ""
        ident = identifier(relative)
        if "EXPECT" in sections:
            expected = sections["EXPECT"].replace("\r\n", "\n").replace(test.name, "test.php")
            assertion = f"    assert_eq!(out, {rust_string(expected.strip())});"
        elif "EXPECTF" in sections:
            expected = sections["EXPECTF"].replace("\r\n", "\n").replace(test.name, "test.php")
            assertion = f"    assert_format(&out, {rust_string(expected.strip())});"
        elif "EXPECTREGEX" in sections:
            expected = sections["EXPECTREGEX"].replace("\r\n", "\n").replace(test.name, "test.php")
            assertion = f"    assert_regex(&out, {rust_string(expected.strip())});"
        else:
            raise SystemExit(f"{relative}: missing EXPECT/EXPECTF/EXPECTREGEX section")
        status_path = relative
        state, reason = status.get(status_path, ("", ""))
        ignore = ""
        if state == "SKIP":
            counts["skip"] += 1
            suffix = f": {reason}" if reason else ""
            ignore = f'    #[ignore = {rust_string("php-src SKIP on this platform" + suffix)}]\n'
        elif state in ("XFAIL", "FAIL", "BORK"):
            counts["xfail"] += 1
            suffix = f": {reason}" if reason else ""
            ignore = f'    #[ignore = {rust_string("php-src " + state + " on this platform" + suffix)}]\n'
        elif state == "PASS":
            counts["pass"] += 1
        else:
            counts["other"] += 1
        functions.append(
            f"/// Replays php-src `{relative}`.\n"
            "#[test]\n"
            f"{ignore}"
            f"fn {ident}() {{\n"
            f"    let out = run_case({rust_string(source)}, {rust_string(relative_dir)}, {ini}, {env});\n"
            f"{assertion}\n"
            "}"
        )
    output = (
        "//! Purpose:\n"
        "//! Generated Rust tests for the committed PHP 8.5.10 `ext/intl` PHPT corpus.\n"
        "//!\n"
        "//! Called from:\n"
        "//! - `cargo test` through `tests/codegen/intl_php_src.rs`.\n"
        "//!\n"
        "//! Key details:\n"
        "//! - Generated by `scripts/gen_php_src_intl_cases.py`; do not hand-edit.\n"
        "//! - Each test carries its PHP source and assertion inline, and the test process\n"
        "//!   never parses a `.phpt` file at run time.\n"
        "//! - Tests php-src skips on this platform are preserved as ignored cases with the\n"
        "//!   original skip reason, so the provenance stays complete.\n"
        "\n"
        "#![allow(non_snake_case)]\n\n"
        "use super::{assert_format, assert_regex, run_case};\n\n"
        + "\n\n".join(functions)
        + "\n"
    )
    OUTPUT.write_text(output, encoding="utf-8")
    print(f"wrote {len(tests)} inline tests to {OUTPUT} ({counts})")


if __name__ == "__main__":
    main()
