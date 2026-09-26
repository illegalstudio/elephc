#!/usr/bin/env python3
"""Generate mbstring's compact tables from the Unicode 17.0.0 UCD directory."""

import argparse
import hashlib
import json
from pathlib import Path
import struct


VERSION = "17.0.0"
INPUT_SHA256 = {
    'UnicodeData.txt': '2e1efc1dcb59c575eedf5ccae60f95229f706ee6d031835247d843c11d96470c',
    'SpecialCasing.txt': 'efc25faf19de21b92c1194c111c932e03d2a5eaf18194e33f1156e96de4c9588',
    'CaseFolding.txt': 'ff8d8fefbf123574205085d6714c36149eb946d717a0c585c27f0f4ef58c4183',
    'DerivedCoreProperties.txt': '24c7fed1195c482faaefd5c1e7eb821c5ee1fb6de07ecdbaa64b56a99da22c08',
    'EastAsianWidth.txt': 'ea7ce50f3444a050333448dffef1cadd9325af55cbb764b4a2280faf52170a33',
}
ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "crates/elephc-mbstring/src/unicode/data"
INPUTS = (
    "UnicodeData.txt", "SpecialCasing.txt", "CaseFolding.txt",
    "DerivedCoreProperties.txt", "EastAsianWidth.txt",
)


def rows(path):
    """Yield semicolon fields after removing UCD comments and empty lines."""
    for line in path.read_text().splitlines():
        line = line.partition("#")[0].strip()
        if line:
            yield [field.strip() for field in line.split(";")]


def codepoints(text):
    """Decode one UCD hexadecimal codepoint sequence."""
    return tuple(int(value, 16) for value in text.split())


def ranges(path, wanted):
    """Select properties and merge their sorted, inclusive codepoint ranges."""
    selected = []
    for fields in rows(path):
        if fields[1] not in wanted:
            continue
        limits = fields[0].split("..")
        selected.append((int(limits[0], 16), int(limits[-1], 16)))
    merged = []
    for start, end in sorted(selected):
        if merged and start <= merged[-1][1] + 1:
            merged[-1] = (merged[-1][0], max(end, merged[-1][1]))
        else:
            merged.append((start, end))
    return b"".join(struct.pack("<II", *entry) for entry in merged)


def generate(directory):
    """Write sorted case mappings, property ranges, and input/output digests."""
    for name, expected in INPUT_SHA256.items():
        actual = hashlib.sha256((directory / name).read_bytes()).hexdigest()
        if actual != expected:
            raise ValueError(f"{name} does not match the pinned Unicode {VERSION} source")
    simple = {name: {} for name in ("upper", "lower", "title", "fold")}
    for fields in rows(directory / "UnicodeData.txt"):
        code = int(fields[0], 16)
        for name, index in (("upper", 12), ("lower", 13), ("title", 14)):
            mapping = fields[index] or (fields[12] if name == "title" else "")
            if mapping:
                simple[name][code] = int(mapping, 16)
    full = {
        name: {code: (target,) for code, target in mapping.items()}
        for name, mapping in simple.items()
    }
    for fields in rows(directory / "SpecialCasing.txt"):
        if fields[4]:
            continue  # Context and encoding-dependent rules are implemented in Rust.
        code = int(fields[0], 16)
        for name, index in (("lower", 1), ("title", 2), ("upper", 3)):
            full[name][code] = codepoints(fields[index])
    for fields in rows(directory / "CaseFolding.txt"):
        code, status = int(fields[0], 16), fields[1]
        mapping = codepoints(fields[2])
        if status in ("C", "S"):
            simple["fold"][code] = mapping[0]
        if status in ("C", "F"):
            full["fold"][code] = mapping

    artifacts = {}
    for name in simple:
        records = []
        for code in sorted(simple[name].keys() | full[name].keys()):
            one = simple[name].get(code, code)
            many = full[name].get(code, (code,))
            if one == code and many == (code,):
                continue
            assert 1 <= len(many) <= 3
            records.append(struct.pack("<6I", code, one, len(many), *many,
                                       *([0] * (3 - len(many)))))
        artifacts[name + ".bin"] = b"".join(records)
    for prop in ("Cased", "Case_Ignorable"):
        artifacts[prop.lower() + ".bin"] = ranges(
            directory / "DerivedCoreProperties.txt", {prop})
    artifacts["wide.bin"] = ranges(directory / "EastAsianWidth.txt", {"W", "F"})
    OUTPUT.mkdir(parents=True, exist_ok=True)
    for name, data in artifacts.items():
        (OUTPUT / name).write_bytes(data)
    manifest = {
        "unicode_version": VERSION,
        "source": f"https://www.unicode.org/Public/{VERSION}/ucd/",
        "inputs": {name: hashlib.sha256((directory / name).read_bytes()).hexdigest()
                   for name in INPUTS},
        "outputs": {name: hashlib.sha256(data).hexdigest()
                    for name, data in sorted(artifacts.items())},
    }
    (OUTPUT / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("ucd_directory", type=Path)
    generate(parser.parse_args().ucd_directory)
