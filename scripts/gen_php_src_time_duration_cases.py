#!/usr/bin/env python3
"""Generates Rust codegen cases from php-src's Time\\Duration PHPT corpus.

The PHPT files stay the source of truth: this script reads them at development time and
writes `tests/codegen/time_duration_php_src_cases.rs` with the PHP source and the exact
`--EXPECT--` output inline. Tests never parse `.phpt` at runtime.

Usage:
    python3 scripts/gen_php_src_time_duration_cases.py \
        --php-src /path/to/php-src

Only 64-bit expectations are used (`_32` variants are skipped): every supported elephc
target has 64-bit integers. Cases that need engine internals elephc does not expose
(`ReflectionProperty::setRawValue`, `newInstanceWithoutConstructor`, readonly
`clone($d, [...])`) are listed in EXCLUDED with the reason they stay out.
"""

from __future__ import annotations

import argparse
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "tests/codegen/time_duration_php_src_cases.rs"

# Top-level PHPTs that exercise engine internals outside the class contract.
EXCLUDED_TOP_LEVEL = {
    "new": "needs ReflectionClass::newInstanceWithoutConstructor and internal-class reflection errors",
    "readonly": "needs ReflectionProperty::setRawValue/setRawValueWithoutLazyInitialization and clone-with",
    "serialize": (
        "covered by tests/codegen/time_duration_serialization.rs with a mixed-typed helper; "
        "the generated bundle requires exact PHPT source and cannot narrow unserialize()"
    ),
    "gh23639": "needs the internal __unserialize property loader and readonly by-reference rejection",
    "oss_fuzz_562721806": "needs the internal __unserialize property loader",
    "z_param_date_time_duration": (
        "passes int where a Time\\Duration parameter is required; elephc rejects that "
        "statically (the dedicated error test covers the boundary instead)"
    ),
}

PREAMBLE = r'''function f(mixed $d, bool $pad = true, bool $plus_sign = true): string {
    if (!$d instanceof Time\Duration) {
        throw new \TypeError("f(): Argument #1 ($d) must be of type Time\\Duration");
    }

    $result = sprintf("%s%d.%09d", ($d->negative ? "-" : ($plus_sign ? "+" : " ")), $d->seconds, $d->nanoseconds);
    if ($pad) {
        $result = str_pad($result, 21, " ", STR_PAD_LEFT);
    }

    return $result;
}

function as_int(Time\Duration $d): int {
    return ($d->negative ? -1 : 1) * ($d->seconds * 1_000_000_000 + $d->nanoseconds);
}

function negate(Time\Duration $d): Time\Duration {
    return $d->negate();
}

function is_negative(Time\Duration $d): bool {
    return $d->negative;
}

function negate_all(array $d): array {
    $negated = array_map(negate(...), $d);

    return array_filter($negated, is_negative(...));
}

function n(int $result) {
    return ($result === 0 ? '=' : ($result < 0 ? '<' : '>'));
}

function fc(Time\Duration $d): string {
    return f(clone($d));
}
'''


def php_single_quote(text: str) -> str:
    """Returns a PHP single-quoted literal for an HTML output chunk."""
    escaped = text.replace("\\", "\\\\").replace("'", "\\'")
    return f"'{escaped}'"


def rust_raw(text: str) -> str:
    """Returns a Rust raw string literal that preserves `text` byte-for-byte."""
    hashes = 1
    while f'"{("#" * hashes)}' in text:
        hashes += 1
    fence = "#" * hashes
    return f'r{fence}"{text}"{fence}'


def parse_phpt(path: pathlib.Path) -> tuple[str, str, str]:
    """Returns `(title, file source, expected stdout)` for one exact-output PHPT."""
    raw = path.read_text(encoding="utf-8")
    sections: dict[str, str] = {}
    current: str | None = None
    lines: list[str] = []
    for line in raw.splitlines(keepends=True):
        match = re.fullmatch(r"--([A-Z0-9_]+)--\n?", line)
        if match:
            if current is not None:
                sections[current] = "".join(lines)
            current = match.group(1)
            lines = []
        elif current is not None:
            lines.append(line)
    if current is not None:
        sections[current] = "".join(lines)

    if "EXPECTF" in sections or "EXPECTREGEX" in sections:
        raise SystemExit(f"{path}: only exact --EXPECT-- output is supported")
    file_source = sections.get("FILE")
    expected = sections.get("EXPECT")
    if file_source is None or expected is None:
        raise SystemExit(f"{path}: missing --FILE-- or --EXPECT--")

    # PHPT files usually end with a closing tag; the bundle concatenates bodies, so a
    # closing tag would switch the shared file back to HTML mode. Inline any trailing
    # output (compare.phpt prints `==DONE==` after `?>`) as an echo instead.
    closing = re.search(r"\?>(.*)\Z", file_source, flags=re.S)
    if closing:
        trailing = closing.group(1)
        inline = "".join(
            f"echo {php_single_quote(chunk)};\n"
            for chunk in re.split(r"(?<=\n)", trailing)
            if chunk
        )
        file_source = file_source[: closing.start()] + "\n" + inline

    # helper.inc is inlined by the generated bundle preamble.
    file_source = re.sub(
        r"require __DIR__ \. '/(?:\.\./)?helper\.inc';\n",
        "",
        file_source,
    )
    # `n()` and `fc()` live in the shared preamble.
    if path.name == "compare.phpt":
        file_source = re.sub(
            r"function n\(int \$result\) \{\n.*?\n\}\n\n",
            "",
            file_source,
            flags=re.S,
        )
    if path.name == "clone.phpt":
        file_source = re.sub(
            r"function fc\(Time\\Duration \$d\): string \{\n.*?\n\}\n\n",
            "",
            file_source,
            flags=re.S,
        )

    # php-src uses `addcslashes($specification, "\0")` only to render the fixture label.
    # elephc does not ship that unrelated builtin; the rewrite is byte-identical for a
    # charlist containing exactly NUL (`\0` -> `\000`).
    file_source = file_source.replace(
        'addcslashes($specification, "\\0")',
        'str_replace("\\0", "\\\\000", $specification)',
    )

    # The static checker cannot prove that a mixed array element is a Duration after a
    # `=== null` check. Add the runtime guard php-src already relies on; it is always true
    # for every non-null element, so stdout is unchanged.
    for variable in ("a", "b", "d"):
        file_source = re.sub(
            r"if \(\$" + variable + r" === null\) \{\n.*?continue;\n\s*\}",
            lambda match, variable=variable: (
                match.group(0)
                + "\n\n        if (!$" + variable + " instanceof Time\\Duration) {\n"
                + "            continue;\n        }"
            ),
            file_source,
            flags=re.S,
        )

    # Bundled cases share one global scope, so a variable left mixed by an earlier case
    # would poison the next one. Each body runs in its own closure; nothing observable
    # changes because the fixtures never declare a global function inside a body.
    body = file_source
    if body.startswith("<?php"):
        body = body[len("<?php"):]
    file_source = "<?php\n(function () {\n" + body + "\n})();\n"

    return sections.get("TEST", path.name).strip(), file_source, expected


def build_cases(directory: pathlib.Path, prefix: str = "duration_") -> list[tuple[str, str, str, str]]:
    """Returns `(name, phpt file, php source, expected)` for one directory."""
    cases = []
    for path in sorted(directory.glob("*.phpt")):
        stem = path.stem
        if stem.endswith("_32"):
            continue
        if path.parent.name == "duration" and stem in EXCLUDED_TOP_LEVEL:
            continue
        title, source, expected = parse_phpt(path)
        name = prefix + stem.replace("-", "_")
        cases.append((name, path.name, source, expected))
    return cases


def emit(cases: list[tuple[str, str, str, str]]) -> str:
    helpers_line = "pub(super) const HELPERS: &str = " + rust_raw("<?php\n" + PREAMBLE) + ";"
    out = [
        "//! Purpose:",
        "//! Generated Rust fixtures migrated from php-src PHP 8.6's `Time\\Duration` PHPT corpus.",
        "//!",
        "//! Called from:",
        "//! - `cargo test --test codegen_tests time_duration_php_src` through the module in",
        "//!   `tests/codegen/time_duration_php_src.rs`.",
        "//!",
        "//! Key details:",
        "//! - Every `source`/`expected` pair is transcribed from the vendored PHPT file named in",
        "//!   `phpt`; regenerate with `scripts/gen_php_src_time_duration_cases.py`.",
        "//! - Only 64-bit expectations are represented: every supported elephc target has 64-bit",
        "//!   integers, so php-src's `_32` variants do not apply.",
        "//! - These cases print stdout only, so they compile once as a bundle.",
        "",
        "/// One transcribed PHPT fixture.",
        "pub(super) struct DurationPhptCase {",
        "    /// Former test name and bundle case name.",
        "    pub name: &'static str,",
        "    /// Vendored PHPT file name, kept for provenance.",
        "    pub phpt: &'static str,",
        "    /// Complete PHP source, minus the shared `helper.inc` require.",
        "    pub source: &'static str,",
        "    /// Byte-exact stdout from the PHPT's `--EXPECT--` section.",
        "    pub expected: &'static str,",
        "}",
        "",
        "/// Shared `helper.inc` declarations, compiled once before every case.",
        helpers_line,
        "",
        "/// Every supported php-src `ext/date/tests/time/duration` fixture.",
        "pub(super) const CASES: &[DurationPhptCase] = &[",
    ]
    for name, phpt, source, expected in cases:
        out.extend(
            [
                "    DurationPhptCase {",
                f'        name: "{name}",',
                f'        phpt: "{phpt}",',
                f"        source: {rust_raw(source)},",
                f"        expected: {rust_raw(expected)},",
                "    },",
            ]
        )
    out.append("];")
    out.append("")
    return "\n".join(out)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--php-src",
        required=True,
        help="php-src checkout containing ext/date/tests/time/duration",
    )
    parser.add_argument("--output", default=str(OUTPUT))
    args = parser.parse_args()

    corpus = pathlib.Path(args.php_src) / "ext/date/tests/time/duration"
    if not corpus.is_dir():
        corpus = pathlib.Path(args.php_src)
    if not (corpus / "helper.inc").is_file():
        raise SystemExit(f"missing corpus directory: {corpus}")

    top = build_cases(corpus, "duration_top_")
    methods = build_cases(corpus / "methods")
    generated = emit(top + methods)
    output = pathlib.Path(args.output)
    output.write_text(generated, encoding="utf-8")
    print(f"wrote {output} ({len(top) + len(methods)} cases)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
