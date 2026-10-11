#!/usr/bin/env python3
"""Extract TZif footer rules from PHP 8.5.6's pinned timelib timezone database."""

from pathlib import Path
import hashlib
import re
import struct
import sys

TAG_OBJECT = "b208d6046a61da2f2dfdd45883ce02d5299d1b22"
COMMIT = "fcc29c8d6d6ee6f5ba2d941f0a2a6ea6aa6ee633"
TZDB = "2026.1"
HEADER_SHA256 = "cd32a07da0278f8c2d088ba44360c881446dba1d11ba88905e45ef33e73f58c3"
ZONE_COUNT = 598


def block_end(data: bytes, start: int, width: int) -> int:
    counts = struct.unpack(">6I", data[start + 20:start + 44])
    gmt, std, leaps, times, types, chars = counts
    return start + 44 + times * width + times + types * 6 + chars + leaps * (width + 4) + std + gmt


def runtime_data(blob: bytes) -> tuple[str, str]:
    start = blob.index(b"TZif")
    if blob[start + 4:start + 5] not in (b"2", b"3", b"4"):
        return "", ""
    # PHP's generated database defines TIMELIB_SUPPORT_SLIM_FILE: its TZif2
    # payload omits the redundant 32-bit block and stores the 64-bit block
    # directly after the one header.
    end = block_end(blob, start, 8)
    if blob[end:end + 1] != b"\n":
        return "", ""
    rule = blob[end + 1:blob.index(b"\n", end + 1)].decode("ascii")
    gmt, std, leaps, time_count, type_count, char_count = struct.unpack(
        ">6I", blob[start + 20:start + 44])
    cursor = start + 44
    times = struct.unpack(f">{time_count}q", blob[cursor:cursor + time_count * 8])
    cursor += time_count * 8
    type_indexes = blob[cursor:cursor + time_count]
    cursor += time_count
    types = []
    for _ in range(type_count):
        offset, dst, abbr = struct.unpack(">iBB", blob[cursor:cursor + 6])
        types.append((offset, dst, abbr))
        cursor += 6
    abbreviations = blob[cursor:cursor + char_count]
    initial_offset, initial_dst, initial_abbr_index = types[0]
    initial_stop = abbreviations.index(0, initial_abbr_index)
    initial_abbr = abbreviations[initial_abbr_index:initial_stop].decode("ascii")
    rows = [f"{-2**63},{initial_offset},{initial_dst},{initial_abbr}"]
    for timestamp, type_index in zip(times, type_indexes):
        offset, dst, abbr_index = types[type_index]
        stop = abbreviations.index(0, abbr_index)
        abbr = abbreviations[abbr_index:stop].decode("ascii")
        rows.append(f"{timestamp},{offset},{dst},{abbr}")
    return rule, ";".join(rows)


def main() -> None:
    check = len(sys.argv) == 4 and sys.argv[1] == "--check"
    args = sys.argv[2:] if check else sys.argv[1:]
    if len(args) != 2:
        raise SystemExit("usage: extract_posix_rules.py [--check] <php-src/timezonedb.h> <output>")
    source_bytes = Path(args[0]).read_bytes()
    digest = hashlib.sha256(source_bytes).hexdigest()
    if digest != HEADER_SHA256:
        raise SystemExit(f"expected timezonedb.h sha256 {HEADER_SHA256}, got {digest}")
    source = source_bytes.decode("ascii")
    version = re.search(r'timezonedb_builtin = \{ "([^"]+)"', source).group(1)
    if version != TZDB:
        raise SystemExit(f"expected tzdb {TZDB}, got {version}")
    entries = [(name, int(offset, 16)) for name, offset in re.findall(
        r'\{ \(char\*\) "([^"]+)"\s*, 0x([0-9A-F]+) \}', source)][:ZONE_COUNT]
    if len(entries) != ZONE_COUNT or len({name for name, _ in entries}) != ZONE_COUNT:
        raise SystemExit("expected exactly 598 unique timezone index entries")
    if any(left[1] >= right[1] for left, right in zip(entries, entries[1:])):
        raise SystemExit("timezone blob offsets are not strictly increasing")
    array = source[source.index("const unsigned char timelib_timezone_db_data_builtin") :]
    array = array[:array.index("};")]
    raw = bytes(int(value, 16) for value in re.findall(r"0x([0-9A-F]{2})", array))
    if entries[-1][1] >= len(raw):
        raise SystemExit("final timezone blob offset lies outside the embedded byte array")
    lines = [f"# php-src tag-object={TAG_OBJECT} commit={COMMIT} tzdb={TZDB}"]
    for index, (name, offset) in enumerate(entries):
        stop = entries[index + 1][1] if index + 1 < len(entries) else len(raw)
        rule, transitions = runtime_data(raw[offset:stop])
        lines.append(f"{name}\t{rule}\t{transitions}")
    rendered = "\n".join(lines) + "\n"
    output = Path(args[1])
    if check:
        if not output.exists() or output.read_text(encoding="ascii") != rendered:
            raise SystemExit(f"{output} is stale; regenerate it without --check")
    else:
        output.write_text(rendered, encoding="ascii")


if __name__ == "__main__":
    main()
