#!/usr/bin/env python3
"""Regenerates crates/elephc-iconv/src/case/tables.rs from php-src and a php binary.

Usage:
    python3 scripts/mbstring/gen_case_tables.py <php-src checkout> [php binary]

Two oracles feed the generated file:
- php-src's ext/mbstring/unicode_data.h provides the Cased and Case_Ignorable property
  ranges mbstring's final-sigma rule consults (they are mbstring's own tables, which is
  why they are not taken from the Unicode data Rust ships).
- The php binary (via probe_encodings.php) provides every encoding name mbstring accepts,
  including the name prefixes its perfect-hash lookup lets through, and the exact byte <->
  code point tables of the single-byte encodings.

The php binary must carry the same mbstring as the php-src checkout; the script records
the php version in the generated header.
"""

import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
OUTPUT = os.path.join(ROOT, "crates", "elephc-iconv", "src", "case", "tables.rs")
PROBE = os.path.join(ROOT, "scripts", "mbstring", "probe_encodings.php")

# Property indices from php-src's ext/mbstring/php_unicode.h.
UC_CASED = 35
UC_CASE_IGNORABLE = 36


def parse_array(source, name):
    """Returns the integer contents of one C array initializer in unicode_data.h."""
    match = re.search(r"%s\[\]\s*=\s*\{(.*?)\};" % re.escape(name), source, re.S)
    if not match:
        sys.exit("unicode_data.h has no array named %s" % name)
    return [int(value, 16) for value in re.findall(r"0x[0-9a-fA-F]+", match.group(1))]


def property_ranges(source, index):
    """Returns the inclusive code point ranges of one mbstring property."""
    offsets = parse_array(source, "_ucprop_offsets")
    ranges = parse_array(source, "_ucprop_ranges")
    flat = ranges[offsets[index]:offsets[index + 1]]
    return [(flat[i], flat[i + 1]) for i in range(0, len(flat), 2)]


def rust_ident(encoding):
    """Returns the Rust static name for one single-byte encoding table."""
    return "SB_" + re.sub(r"[^A-Za-z0-9]", "_", encoding).upper()


def emit_ranges(out, name, doc, ranges):
    """Writes one sorted inclusive range table."""
    out.append("/// %s" % doc)
    out.append("pub(super) static %s: &[(u32, u32)] = &[" % name)
    for low, high in ranges:
        out.append("    (0x%04X, 0x%04X)," % (low, high))
    out.append("];")
    out.append("")


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    php_src = sys.argv[1]
    php = sys.argv[2] if len(sys.argv) > 2 else "php"
    with open(os.path.join(php_src, "ext", "mbstring", "unicode_data.h")) as handle:
        unicode_data = handle.read()
    probe = json.loads(subprocess.check_output([php, PROBE]))

    out = [
        "//! Purpose:",
        "//! Generated mbstring data for PHP's case-mapping builtins: the Cased and",
        "//! Case_Ignorable property ranges, the accepted encoding names, and the single-byte",
        "//! encoding tables.",
        "//!",
        "//! Called from:",
        "//! - `crate::case::props`, `crate::case::encoding`, and `crate::case::codec`.",
        "//!",
        "//! Key details:",
        "//! - DO NOT EDIT. Regenerate with `python3 scripts/mbstring/gen_case_tables.py",
        "//!   <php-src> [php]`; the data below was probed from php %s." % probe["php_version"],
        "//! - Property ranges come from php-src's `ext/mbstring/unicode_data.h`, so the",
        "//!   final-sigma context test sees exactly the tables mbstring consults.",
        "//! - `NAMES` lists every lowercase spelling mbstring's `mbfl_name2encoding()` accepts,",
        "//!   including the name prefixes its perfect-hash lookup lets through (`cp` names",
        "//!   CP950), mapped to the canonical encoding name.",
        "//! - A single-byte table decodes every byte (`BAD` marks a byte the encoding leaves",
        "//!   undefined) and lists every BMP code point the encoder accepts, sorted for",
        "//!   binary search; everything absent is unrepresentable.",
        "",
        "use super::codec::SingleByteTable;",
        "",
        "/// Decode-table marker for a byte the encoding does not define.",
        "pub(super) const BAD: u16 = 0xFFFF;",
        "",
    ]
    emit_ranges(out, "CASED", "mbstring's `UC_CASED` property ranges.", property_ranges(unicode_data, UC_CASED))
    emit_ranges(
        out,
        "CASE_IGNORABLE",
        "mbstring's `UC_CASE_IGNORABLE` property ranges.",
        property_ranges(unicode_data, UC_CASE_IGNORABLE),
    )

    out.append("/// Every accepted lowercase encoding name and the canonical encoding it selects.")
    out.append("pub(super) static NAMES: &[(&str, &str)] = &[")
    for name, canonical in sorted(probe["names"].items(), key=lambda item: item[0].encode()):
        out.append("    (%s, %s)," % (json.dumps(name), json.dumps(canonical)))
    out.append("];")
    out.append("")

    for encoding, table in probe["single_byte"].items():
        decode = table["decode"]
        if len(decode) != 256:
            sys.exit("%s: decode table is not 256 entries" % encoding)
        words = []
        for value in decode:
            if value < 0:
                words.append("BAD")
            elif value >= 0xFFFF:
                sys.exit("%s: decoded U+%X does not fit the u16 table" % (encoding, value))
            else:
                words.append("0x%04X" % value)
        out.append("/// mbstring's `%s` byte table." % encoding)
        out.append("pub(super) static %s: SingleByteTable = SingleByteTable {" % rust_ident(encoding))
        out.append("    decode: [")
        for start in range(0, 256, 8):
            out.append("        " + ", ".join(words[start:start + 8]) + ",")
        out.append("    ],")
        out.append("    encode: &[")
        pairs = ["(0x%04X, 0x%02X)" % (cp, byte) for cp, byte in table["encode"]]
        for start in range(0, len(pairs), 6):
            out.append("        " + ", ".join(pairs[start:start + 6]) + ",")
        out.append("    ],")
        out.append("};")
        out.append("")

    out.append("/// Returns the byte table of one canonical single-byte encoding.")
    out.append("pub(super) fn single_byte_table(canonical: &str) -> Option<&'static SingleByteTable> {")
    out.append("    match canonical {")
    for encoding in probe["single_byte"]:
        out.append("        %s => Some(&%s)," % (json.dumps(encoding), rust_ident(encoding)))
    out.append("        _ => None,")
    out.append("    }")
    out.append("}")

    with open(OUTPUT, "w") as handle:
        handle.write("\n".join(out) + "\n")
    print("wrote %s" % os.path.relpath(OUTPUT, ROOT))


if __name__ == "__main__":
    main()
