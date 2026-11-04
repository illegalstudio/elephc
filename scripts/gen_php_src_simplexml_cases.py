#!/usr/bin/env python3
"""Generate `tests/codegen/simplexml_php_src_cases.rs` from the committed php-src corpus.

Usage:
    python3 scripts/gen_php_src_simplexml_cases.py

Each generated Rust `#[test]` carries its PHP source and assertion inline, matching
the ordinary Elephc codegen fixture style. The `.phpt` files under
`tests/php_src/simplexml/` remain the provenance copy and the input to this
generator; the test process never parses a `.phpt` file at run time.
"""

from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CORPUS = ROOT / "tests" / "php_src" / "simplexml"
OUTPUT = ROOT / "tests" / "codegen" / "simplexml_php_src_cases.rs"
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


HEREDOC_LABEL = re.compile(r"(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\1")


def strip_php_tags(source: str) -> str:
    """Rewrites PHP open/close tags and inline HTML into the compiler's blockless form.

    The PHP lexer used by Elephc accepts a leading `<?php` block but not a closing `?>` or
    inline HTML. Test fixtures keep PHP's own syntax, so this generator removes the tags
    and turns each inline-HTML segment into an equivalent `echo` statement. Strings,
    heredocs/nowdocs, and comments are scanned so a `?>` inside XML data is not mistaken
    for a closing tag.
    """
    output: list[str] = []
    index = 0
    length = len(source)
    in_php = False
    while index < length:
        if not in_php:
            opening = source.find("<?", index)
            if opening < 0:
                html = source[index:]
                if html:
                    output.append("echo " + php_single_quote(html) + ";")
                break
            html = source[index:opening]
            if html:
                output.append("echo " + php_single_quote(html) + ";")
            if source.startswith("<?php", opening):
                if not output:
                    output.append("<?php")
                index = opening + 5
            elif source.startswith("<?=", opening):
                output.append("echo ")
                index = opening + 3
            else:
                index = opening + 2
            in_php = True
            output.append("")
            continue

        start = index
        cursor = index
        closing = None
        while cursor < length:
            char = source[cursor]
            if char in "'\"`":
                quote = char
                cursor += 1
                while cursor < length:
                    if source[cursor] == "\\":
                        cursor += 2
                        continue
                    if source[cursor] == quote:
                        cursor += 1
                        break
                    cursor += 1
                continue
            if char == "/" and cursor + 1 < length and source[cursor + 1] == "/":
                cursor += 2
                while cursor < length and source[cursor] not in "\r\n":
                    if source.startswith("?>", cursor):
                        break
                    cursor += 1
                continue
            if char == "#":
                cursor += 1
                while cursor < length and source[cursor] not in "\r\n":
                    if source.startswith("?>", cursor):
                        break
                    cursor += 1
                continue
            if char == "/" and cursor + 1 < length and source[cursor + 1] == "*":
                end = source.find("*/", cursor + 2)
                cursor = length if end < 0 else end + 2
                continue
            if source.startswith("<<<", cursor):
                line_end = source.find("\n", cursor)
                if line_end < 0:
                    cursor = length
                    continue
                match = HEREDOC_LABEL.match(source[cursor + 3:line_end].strip())
                if not match:
                    cursor = line_end + 1
                    continue
                label = match.group(2)
                probe = line_end + 1
                while probe < length:
                    probe_end = source.find("\n", probe)
                    if probe_end < 0:
                        probe_end = length
                    if re.match(
                        r"^[ \t]*" + re.escape(label) + r"\s*[;,\)\]]*\s*$",
                        source[probe:probe_end],
                    ):
                        cursor = probe_end + 1 if probe_end < length else length
                        break
                    probe = probe_end + 1
                else:
                    cursor = length
                continue
            if source.startswith("?>", cursor):
                closing = cursor
                break
            cursor += 1

        code = source[start:closing if closing is not None else length]
        if closing is not None:
            # A closing tag terminates the preceding statement like `;`, so restore the
            # semicolon the compiler's tagless form needs.
            trimmed = code.rstrip()
            if trimmed and trimmed[-1] not in ";}{:":
                code = code.rstrip() + ";"
        output[-1] = output[-1] + code
        if closing is None:
            break
        after = closing + 2
        if source.startswith("\r\n", after):
            after += 2
        elif after < length and source[after] == "\n":
            after += 1
        index = after
        in_php = False
    return "".join(output)


def rust_raw(text: str) -> str:
    """Returns a Rust raw string literal with a delimiter absent from `text`."""
    hashes = 1
    while ('"' + "#" * hashes) in text:
        hashes += 1
    marker = "#" * hashes
    return f'r{marker}"{text}"{marker}'


BLOCKERS = {
}

def skip_reason(sections: dict[str, str]) -> str | None:
    """Returns the reason a fixture cannot run on the supported Elephc targets."""
    extensions = {
        line.strip().lower()
        for line in sections.get("EXTENSIONS", "").splitlines()
        if line.strip()
    }
    if "xsl" in extensions:
        return "requires the unsupported ext/xsl extension"
    if "zend_test" in extensions:
        return "requires the test-only zend_test extension"
    skipif = sections.get("SKIPIF", "")
    if "zend_test_override_libxml_global_state" in skipif:
        return "requires the test-only zend_test extension"
    return None


def expected_assertion(stem: str, sections: dict[str, str]) -> str:
    """Builds the Rust assertion matching the fixture's EXPECT/EXPECTF section."""
    if "EXPECT" in sections:
        expected = sections["EXPECT"].replace("\r\n", "\n")
        expected = expected.replace(f"{stem}.php", "test.php").strip()
        return f"    assert_exact(&out, {rust_raw(expected)});"
    if "EXPECTF" in sections:
        expected = sections["EXPECTF"].replace("\r\n", "\n")
        expected = expected.replace(f"{stem}.php", "test.php").strip()
        assertion = "assert_fatal_format" if expected.startswith("Fatal error:") else "assert_format"
        return f"    {assertion}(&out, {rust_raw(expected)});"
    if "EXPECTREGEX" in sections:
        # php-src applies EXPECTREGEX as a raw PCRE pattern; the migrated runner has no
        # equivalent helper yet, so fail generation instead of emitting a non-compiling test.
        raise SystemExit(
            f"{stem}: EXPECTREGEX is not supported by the SimpleXML corpus generator"
        )
    raise SystemExit(f"{stem}: missing EXPECT/EXPECTF/EXPECTREGEX section")


def main() -> None:
    """Reads every committed PHPT and writes ordinary Rust test functions."""
    tests = sorted(CORPUS.glob("*.phpt"))
    functions: list[str] = []
    for test in tests:
        name = test.stem
        sections = parse_phpt(test.read_text(encoding="utf-8", errors="surrogateescape"))
        source = strip_php_tags(sections["FILE"].replace("\r\n", "\n"))
        # PHP stops parsing at the first `__HALT_COMPILER()`, so the migration drops the
        # terminator and every following byte; the fixture never executes that tail.
        source = re.split(r"__HALT_COMPILER\s*\(\s*\)\s*;?", source, maxsplit=1)[0]
        # The compiler parses `exit`/`die` as calls; a bare terminator is exactly `exit(0)`.
        source = re.sub(r"(?<![A-Za-z0-9_$])(?:exit|die)\s*;", "exit(0);", source)
        ident = "test_php_src_simplexml_" + re.sub(r"[^a-z0-9]+", "_", name.lower()).strip("_")
        reason = skip_reason(sections) or BLOCKERS.get(name)
        ignore = f'    #[ignore = {rust_raw(reason)}]\n' if reason else ""
        functions.append(
            f"/// Replays php-src `{name}.phpt`.\n"
            "#[test]\n"
            f"{ignore}"
            f"fn {ident}() {{\n"
            f"    if skip_without_xml_native(\"{ident}\") {{ return; }}\n"
            f"    let out = run_case({rust_raw(source)});\n"
            f"{expected_assertion(name, sections)}\n"
            "}"
        )
    output = (
        "//! Purpose:\n"
        "//! Generated Rust tests for the committed PHP 8.5.8 `ext/simplexml` PHPT corpus.\n"
        "//!\n"
        "//! Called from:\n"
        "//! - `cargo test` through `tests/codegen/simplexml_php_src.rs`.\n"
        "//!\n"
        "//! Key details:\n"
        "//! - Generated by `scripts/gen_php_src_simplexml_cases.py`; do not hand-edit.\n"
        "//! - Each test carries its PHP source and assertion inline, and the test process\n"
        "//!   never parses a `.phpt` file at run time.\n"
        "\n"
        "use super::{assert_exact, assert_fatal_format, assert_format, run_case, skip_without_xml_native};\n\n"
        + "\n\n".join(functions)
        + "\n"
    )
    OUTPUT.write_text(output, encoding="utf-8")
    print(f"wrote {len(tests)} inline tests to {OUTPUT}")


if __name__ == "__main__":
    main()
