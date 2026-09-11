#!/usr/bin/env python3
"""Capture pinned PHP mbregex settings, syntax variants, captures, and binary subjects."""

import gzip
import itertools
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def cases():
    """Cross every option through length three and representative native matching behaviors."""
    for size in range(4):
        for options in itertools.product(b"ixmsplnrjugczbd", repeat=size):
            yield {"op": "options", "value": bytes(options).hex()}
    for value in [b"e", b"I", b"a", b"\x00", b"i\x00s", b"\xff", b"i\xffz", b" ", b"imq"]:
        yield {"op": "options", "value": value.hex()}
    aliases = json.loads(subprocess.check_output(["php", "-d", "error_reporting=0", "-r", 'echo json_encode(array_merge(mb_list_encodings(), ...array_map("mb_encoding_aliases", mb_list_encodings())));']))
    for alias in sorted(set(aliases + ["UTF-16BE", "UTF-32BE", "UCS-4LE", "KOI8", "GB18030", "CP1251", "invalid", ""])):
        for value in [alias.encode(), alias.lower().encode() + b"\0ignored"]:
            yield {"op": "encoding", "value": value.hex()}
    patterns = [b"a", b"^a", b"a$", b".", b"a|ab", b"a*", b"(a)?(b*)", b"(?<x>a)|(?<x>b)",
                b"(?<x>a)(b)?", b"(a)(b)\\1", b"(?<=a)b", b"[[:alpha:]]+", b"\\p{Greek}+", b"[", b"", b"\xff"]
    subjects = [b"", b"a", b"ab", b"aba", b"ba", b"a\nb", b"b\na", "éΩα".encode(), b"a\0b", b"\xff"]
    for op, options, pattern, subject in itertools.product(["match", "search"], ["pr", "r", "ir", "mr", "sr", "lnr", "j", "u", "g", "c", "z", "b", "d"], patterns, subjects):
        yield {"op": op, "encoding": "UTF-8", "options": options, "pattern": pattern.hex(), "subject": subject.hex()}
    encoded = json.loads(subprocess.check_output(["php", "-d", "error_reporting=0", "-r", '$out=[]; foreach(mb_list_encodings() as $e) {try {mb_regex_encoding($e);} catch (Throwable) {continue;} $out[]=[$e,bin2hex(mb_convert_encoding("(.)",$e,"UTF-8")),bin2hex(mb_convert_encoding("aéΩ",$e,"UTF-8"))];} echo json_encode($out);']))
    for encoding, pattern, subject in encoded:
        for op in ["match", "search"]:
            yield {"op": op, "encoding": encoding, "options": "pr", "pattern": pattern, "subject": subject}


def main():
    """Write deterministic compressed observations from one isolated oracle worker."""
    inputs = list(cases())
    payload = "".join(json.dumps(case, separators=(",", ":")) + "\n" for case in inputs).encode()
    result = subprocess.run(["php", str(ROOT / "scripts/mbstring/capture_regex.php")], input=payload, capture_output=True, check=True)
    assert len(result.stdout.splitlines()) == len(inputs), result.stderr
    destination = ROOT / "crates/elephc-mbstring/tests/fixtures/regex.jsonl.gz"
    destination.write_bytes(gzip.compress(result.stdout, mtime=0))
    print(f"Captured {len(inputs)} mbregex observations in {destination}")


if __name__ == "__main__":
    main()
