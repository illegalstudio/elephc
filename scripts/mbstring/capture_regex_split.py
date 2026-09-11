#!/usr/bin/env python3
"""Capture PHP mb_split results, diagnostics, and interactions with retained regex state."""

from concurrent.futures import ThreadPoolExecutor
import gzip
import itertools

from capture_regex_request import ROOT, capture, step


def cases():
    """Exercise signed limits, empty matches, encoding validation, cache changes, and warning reentry."""
    for pattern, subject, limit, options in itertools.product(
        [b",", b"(,)", b",+", b"", b"a*", b"$", b"(?=,)", b"(?<=,)", b"\\b", b"\\K", b"[", b"\0"],
        [b"", b"a,b,,c,", b",", b"Aa,a", "é,α".encode()],
        [-(2**63), -2, -1, 0, 1, 2, 3, 2**63-1],
        [b"pr", b"ir", b"b"],
    ):
        yield {"steps": [step("options", value=options), step("split", pattern=pattern, subject=subject, limit=limit)]}
    encodings = [("UTF-8", "utf-8", "é,α,🦀"), ("UTF-16", "utf-16be", "é,α,🦀"),
                 ("UTF-16LE", "utf-16le", "é,α,🦀"), ("UCS-4", "utf-32be", "é,α,🦀"),
                 ("UCS-4LE", "utf-32le", "é,α,🦀"), ("SJIS", "shift_jis", "猫,東京"),
                 ("SJIS-WIN", "cp932", "猫,東京"), ("EUC-JP", "euc_jp", "猫,東京"),
                 ("BIG5", "big5", "中,文"), ("EUC-CN", "gb2312", "中,文"),
                 ("EUC-KR", "euc_kr", "한,국"), ("KOI8R", "koi8_r", "а,б")]
    for encoding, codec, text in encodings:
        for pattern, limit in itertools.product([",", "", ".", ".*?", "$", "(?=,)", "(?<=,)"], [-1, 0, 1, 2]):
            yield {"steps": [step("encoding", value=encoding.encode()),
                             step("split", pattern=pattern.encode(codec), subject=text.encode(codec), limit=limit)]}
    for encoding in ["ASCII", "EUC-TW"] + [f"ISO-8859-{index}" for index in [1,2,3,4,5,6,7,8,9,10,11,13,14,15,16]]:
        yield {"steps": [step("encoding", value=encoding.encode()),
                         step("split", pattern=b",", subject=b"a,b,,c,", limit=-1)]}
    for encoding, subject in [(b"UTF-8", b"a\xffb"), (b"UTF-16LE", b"a\0b"), (b"UCS-4", b"\0\0\0a\0"),
                              (b"ASCII", b"\xff"), (b"SJIS", b"\xfa\x40"), (b"SJIS-WIN", b"\xfa\x40")]:
        for pattern, limit in itertools.product([b"[", b"", b"\xff"], [-1, 0, 1]):
            yield {"steps": [step("encoding", value=encoding), step("split", pattern=pattern, subject=subject, limit=limit)]}
    for options in [b"pr", b"ir", b"b", b"br"]:
        yield {"steps": [step("init", subject=b"aab", pattern=b"a"), step("regs"), step("options", value=options),
                         step("split", pattern=b"a", subject=b"Aa,a", limit=1), step("getregs"), step("regs")]}
    nested = [step("init", subject=b"bb", pattern=b"b"), step("regs"), step("options", value=b"ir"),
              step("split", pattern=b"b", subject=b"AbB", limit=-1)]
    for pattern, throwing in itertools.product([b"[", b"$", b"(a+)+$"], [False, True]):
        mutation = step("split", pattern=pattern, subject=b"a"*12+b"!", limit=-1)
        mutation.update(on_warning=nested, throw=throwing)
        yield {"steps": [step("retry", value=10), step("init", subject=b"aa", pattern=b"a"), step("regs"),
                         mutation, step("getregs"), step("regs")]}
    for setting, value in itertools.product(["retry", "stack"], [0, 1, 10, 100, 2**32-1, 2**32, -1]):
        yield {"steps": [step(setting, value=value),
                         step("split", pattern=b"(a+)+$", subject=b"a"*12+b"!", limit=-1)]}


def main():
    """Write deterministic oracle traces while keeping each PHP request independent."""
    inputs = list(cases())
    with ThreadPoolExecutor(max_workers=4) as workers:
        output = b"".join(workers.map(capture, inputs))
    destination = ROOT / "crates/elephc-mbstring/tests/fixtures/regex_split.jsonl.gz"
    destination.write_bytes(gzip.compress(output, mtime=0))
    print(f"Captured {len(inputs)} mb_split traces in {destination}")


if __name__ == "__main__":
    main()
