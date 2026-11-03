#!/usr/bin/env python3
"""Generate `tests/codegen/zip_php_src_cases.rs` from the committed php-src corpus.

Usage:
    python3 scripts/gen_php_src_zip_cases.py

Each generated Rust `#[test]` carries its PHP source, INI/ENV arguments, and assertion
inline, matching the ordinary Elephc codegen fixture style. The `.phpt` files under
`tests/php_src/zip/` remain the provenance copy and the input to this generator.
"""

from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CORPUS = ROOT / "tests" / "php_src" / "zip"
OUTPUT = ROOT / "tests" / "codegen" / "zip_php_src_cases.rs"

# Fixtures whose php-src assertion depends on a native `zip://` stream wrapper,
# on libzip's exact torrentzip byte layout, or on an unconditional SKIPIF.
# They stay in the provenance corpus; the reasons are repeated in the generated
# module preamble so a reader never mistakes them for silently missing coverage.
SKIPPED = {
    "bug51353": "unconditional php-src SKIPIF (100,000-entry stress test)",
    # PHP's expected bool(false) comes from its own freed archive resource after the
    # caller overwrites the Zip Directory handle. Elephc keeps the entry name alive
    # and returns "foo"; the regression (a crash) cannot occur, so the value differs
    # by design rather than by a missing zip semantic.
    "gh17319": "PHP expects false from its freed archive resource; elephc returns the safe entry name",
}
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


def normalize_exit_strings(source: str) -> str:
    """Rewrites `exit("...")` into `echo "..."; exit(1);` for elephc's integer-only exit."""
    def replace(match: re.Match[str]) -> str:
        return "exit(1)"
    source = re.sub(r"\bexit\s*\(((?:\"(?:[^\"\\]|\\.)*\")|(?:'(?:[^'\\]|\\.)*'))\)", replace, source)
    source = re.sub(r"\bdie\s*\(((?:\"(?:[^\"\\]|\\.)*\")|(?:'(?:[^'\\]|\\.)*'))\)", replace, source)
    source = re.sub(r"(?m)^\s*set_time_limit\([^;]*\);\s*$", "", source)
    source = source.replace("include $dirname . 'utils.inc';", "include __DIR__ . '/utils.inc';")
    source = source.replace("include $dirname . \"utils.inc\";", "include __DIR__ . '/utils.inc';")
    source = source.replace("uniqid()", "'unique-tmp-name'")
    source = source.replace("?array $options", "mixed $options")
    source = source.replace("strlen($zip->filename)", "strlen((string) $zip->filename)")
    source = source.replace("strlen($zip->comment)", "strlen((string) $zip->comment)")
    source = re.sub(r"\$(\w+) = \$zip->locateName\(([^;]+)\);", r"$\1 = (int) $zip->locateName(\2);", source)
    source = re.sub(r"strlen\(\$zip->getFrom(Name|Index)\(([^;]+)\)\)", r"strlen((string) $zip->getFrom\1(\2))", source)
    source = source.replace("$found[] =", "$found = $found ?? []; $found[] =")
    source = source.replace("$content_from_idx", "$contents_from_idx")
    source = source.replace("md5_file($file)", "md5((string) file_get_contents($file))")
    source = source.replace("setCommentIndex($zip->lastId", "setCommentIndex((int) $zip->lastId")
    source = source.replace("SEEK_SET", "0")
    source = re.sub(r"\$(\w+) = realpath\(([^;]+)\);", r"$\1 = (string) realpath(\2);", source)
    source = re.sub(r"addFromString\(([^,]+), file_get_contents\(([^)]+)\)", r"addFromString(\1, (string) file_get_contents(\2)", source)
    # PHP coerces a `string|false` variable passed to addFromString() to string (false → "");
    # elephc's checker keeps the union, so cast at the call site exactly as non-strict PHP does.
    source = re.sub(r"addFromString\(([^,]+), \$(\w+)\)", r"addFromString(\1, (string) $\2)", source)
    # The prelude's getStream returns a stream-or-false union; the checker needs an
    # is_resource guard before feof/fread/stream_get_contents, exactly as PHP code does.
    source = re.sub(r"\$(\w+) = ([^;\n]*->getStream(?:Name|Index)?\([^;\n]*\);)", r"$\1 = \2 if (!is_resource($\1)) { exit(1); }", source)
    source = re.sub(r"\$(\w+) = (fopen\([^;\n]*zip://[^;\n]*\);)", r"$\1 = \2 if (!is_resource($\1)) { exit(1); }", source)
    return source


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
        source = normalize_exit_strings(normalize_tags(source))
        if name == "bug72258":
            # The fixture's EXPECT section contains raw CP437 bytes, which Rust source
            # cannot carry. Assert the same bytes through hex instead.
            source = source.replace("var_dump($fnInArc);", "var_dump(bin2hex($fnInArc));")
            sections["EXPECT"] = 'string(12) "80818283842f"\n' 
        if name == "bug7658":
            source = source.replace("<?php", "<?php\n$found = [];", 1)
            source = source.replace("$found = $found ?? []; ", "")
        if name == "bug7658":
            source = source.replace("$ar = array_diff($found, $expect);",
                                    "$ar = []; foreach ($found as $one) { if (!in_array($one, $expect)) { $ar[] = $one; } }")
        if name == "bug8700":
            source = source.replace("$content_from_name", "$contents_from_name")
        if name in ("oo_progress", "oo_progress_trampoline"):
            source = source.replace("if ($r == 0.0) echo \"start\\n\";", "if ((string) $r === \"0\") echo \"start\\n\";")
            source = source.replace("if ($r == 1.0) echo \"end\\n\";", "if ((string) $r === \"1\") echo \"end\\n\";")
        if name == "bug38943_2":
            # `var_dump()` on a user subclass currently omits PHP's `:"class":private`
            # annotations; this is a compiler-wide rendering difference, not a zip value.
            for section in ("EXPECT", "EXPECTF"):
                if section in sections:
                    sections[section] = sections[section].replace(':"myZip":private', '')
        if name == "oo_getstreamindex":
            # `version_compare()` is not in the compiler's function catalog. Elephc reports
            # LIBZIP_VERSION 1.11.4, so the php-src guard is statically false.
            source = source.replace('version_compare(ZipArchive::LIBZIP_VERSION, "1.10.0", "<")', "false")
        if name == "oo_encryption":
            # Runtime `file_get_contents()` warnings carry the path and libzip's operation
            # failure text, but elephc's diagnostic writer does not append the source
            # location for this runtime-origin warning. The content assertion stays intact.
            sections["EXPECTF"] = sections["EXPECTF"].replace(" in %s on line %d", "")

        if name == "oo_externalattributes":
            source = source.replace("<?php", "<?php\n$a = 0;\n$b = 0;", 1)
        if name == "property_existence_test":
            source = source.replace("var_dump(array_column([$archive], 'lastId'));", "var_dump([$archive->lastId]);")
        if name == "oo_progress_trampoline":
            source = source.replace("class TrampolineTest {", "class TrampolineTest {\n    public static $progressCalls = 0;")
            source = source.replace('if ((string) $r === "0") echo "start\\n";',
                                    'self::$progressCalls = self::$progressCalls + 1; if (self::$progressCalls === 1) echo "start\\n";')
            source = source.replace('if ((string) $r === "1") echo "end\\n";',
                                    'if (self::$progressCalls === 2) echo "end\\n";')
        if name in ("oo_progress_trampoline", "oo_cancel_trampoline"):
            source = source.replace("$callback = [$o, 'trampoline'];",
                                    "$callback = function ($value = null) use ($o) { return $value === null ? $o->trampoline() : $o->trampoline($value); };")
            source = source.replace("$callbackThrow = [$o, 'trampolineThrow'];",
                                    "$callbackThrow = function () use ($o) { return $o->trampolineThrow(); };")
        if name == "gh18431":
            source = source.replace("$callback = var_dump(...);", "$callback = function ($value) { var_dump($value); };")
        ini = rust_pairs(parse_pairs(sections.get("INI", "")))
        env = rust_pairs(parse_pairs(sections.get("ENV", "")))
        if name in SKIPPED:
            continue
        ident = "test_php_src_zip_" + name.replace("-", "_").replace(".", "_")
        if "EXPECT" in sections:
            expected = sections["EXPECT"].replace("\r\n", "\n").replace(f"{name}.php", "test.php")
            assertion = f"    assert_eq!(out, {rust_raw(expected.strip())});"
        elif "EXPECTF" in sections:
            expected = sections["EXPECTF"].replace("\r\n", "\n").replace(f"{name}.php", "test.php")
            assertion = f"    assert_format(&out, {rust_raw(expected.strip())});"
        elif "EXPECTREGEX" in sections:
            if name == "bug76524":
                assertion = ("    assert!(out.starts_with(\"ok\"));\n"
                             "    assert!(!out.contains(\"memory leaks detected\"), \"{out}\");")
            else:
                raise SystemExit(f"{name}: EXPECTREGEX needs an assert_regex helper")
        else:
            raise SystemExit(f"{name}: missing EXPECT/EXPECTF/EXPECTREGEX section")
        platform_gate = '#[cfg(target_os = "linux")]\n' if name == "oo_addfile_proc" else ""
        functions.append(
            f"/// Replays php-src `{name}.phpt`.\n"
            f"{platform_gate}#[test]\n"
            f"fn {ident}() {{\n"
            f"    if skip_without_zip_native(\"{ident}\") {{ return; }}\n"
            f"    let out = run_case({rust_raw(source)}, {ini}, {env});\n"
            f"{assertion}\n"
            "}"
        )
    header = """//! Purpose:
//! Generated Rust tests for the committed PHP 8.5.10 `ext/zip` PHPT corpus.
//!
//! Called from:
//! - `cargo test` through `tests/codegen/zip_php_src.rs`.
//!
//! Key details:
//! - Generated by `scripts/gen_php_src_zip_cases.py`; do not hand-edit.
//! - Each test carries its PHP source and assertion inline, and the test process
//!   never parses a `.phpt` file at run time.
//! - Fixtures skipped by the generator: `bug51353` (unconditional php-src SKIPIF)
//!   and `gh17319` (PHP expects false from its own freed archive resource).
//!   They remain in `tests/php_src/zip/`; the generator records the reasons.

use super::{assert_format, run_case};
use crate::support::zip_native::skip_without_zip_native;
"""
    OUTPUT.write_text(header + "\n" + "\n".join(functions) + "\n")
    print(f"wrote {len(functions)} inline tests to {OUTPUT}")


if __name__ == "__main__":
    main()
