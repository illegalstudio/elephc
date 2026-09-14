#!/usr/bin/env python3
"""Capture PHP mbregex replacements, exact bytes, diagnostics, and retained request state."""

from concurrent.futures import ThreadPoolExecutor
import gzip
import itertools

from capture_regex_request import ROOT, capture, step


def cases():
    """Cover numbered/named references, empty matches, encodings, option errors, limits, and reentry."""
    replacements = [b"", b"X", b"[\\0]", b"\\1|\\2|\\9", b"\\10", b"\\\\1", b"$1", b"\\",
                    b"\\k<0>|\\k<1>|\\k'2'", b"\\k<x>", b"\\k'word'", b"\\k<missing>", b"\\k<01>",
                    b"\\k<10>", b"\\k<4294967296>", b"\\k<4294967297>", b"\\k<18446744073709551616>",
                    b"\\k<>", b"\\k<x", b"\\k", b"\\k!", b"\\k<-1>", b"\\q", b"\0\\0", "é\\0α".encode()]
    for pattern, replacement, op in itertools.product(
        [b"(a)(b)?", b"(?<x>a)|(?<x>b)", b"(?<word>a)?(b*)", b"(?<x>)", b"()()()()()()()()()(a)"],
        replacements, ["replace", "ireplace"],
    ):
        yield {"steps": [step(op, pattern=pattern, replacement=replacement, subject=b"aAbba")]}
    for pattern, subject, options, op in itertools.product(
        [b"", b"a*", b".*?", b"$", b"(?=a)", b"(?<=a)", b"\\b", b"\\K", b"[", b"\0"],
        [b"", b"aA\na", "éα".encode(), b"\xff"],
        [None, b"", b"pr", b"n", b"b", b"j", b"iQ", b"\0"], ["replace", "ireplace"],
    ):
        yield {"steps": [step(op, pattern=pattern, replacement=b"[\\0]", subject=subject, options=options)]}
    encodings = [("UTF-8", "utf-8", "éα🦀"), ("UTF-16", "utf-16be", "éα🦀"),
                 ("UTF-16LE", "utf-16le", "éα🦀"), ("UCS-4", "utf-32be", "éα🦀"),
                 ("UCS-4LE", "utf-32le", "éα🦀"), ("SJIS", "shift_jis", "猫表東京"),
                 ("SJIS-WIN", "cp932", "猫表東京"), ("EUC-JP", "euc_jp", "猫東京"),
                 ("BIG5", "big5", "中文"), ("EUC-CN", "gb2312", "中文"),
                 ("EUC-KR", "euc_kr", "한국"), ("KOI8R", "koi8_r", "аБ")]
    for encoding, codec, text in encodings:
        for pattern, replacement, op in itertools.product(
            ["(.)", "", "$", "(?<x>.)", ".*?"], ["X", "[\\0]", "\\1|\\k<x>", text + "\\0"], ["replace", "ireplace"]
        ):
            yield {"steps": [step("encoding", value=encoding.encode()),
                             step(op, pattern=pattern.encode(codec), replacement=replacement.encode(codec), subject=text.encode(codec))]}
    for encoding in ["ASCII", "EUC-TW"] + [f"ISO-8859-{index}" for index in [1,2,3,4,5,6,7,8,9,10,11,13,14,15,16]]:
        yield {"steps": [step("encoding", value=encoding.encode()),
                         step("ireplace", pattern=b"(a)", replacement=b"[\\1]", subject=b"aAb")]}
    for options, op in itertools.product([b"pr", b"ir", b"b", b"br"], ["replace", "ireplace"]):
        yield {"steps": [step("init", subject=b"aab", pattern=b"a"), step("regs"), step("options", value=options),
                         step(op, pattern=b"a", replacement=b"X", subject=b"Aa,a"), step("getregs"), step("regs")]}
    nested = [step("init", subject=b"bb", pattern=b"b"), step("regs"), step("options", value=b"ir"),
              step("replace", pattern=b"b", replacement=b"Y", subject=b"AbB")]
    for pattern, throwing, op in itertools.product([b"[", b"(a+)+$"], [False, True], ["replace", "ireplace"]):
        mutation = step(op, pattern=pattern, replacement=b"X", subject=b"a"*12+b"!")
        mutation.update(on_warning=nested, throw=throwing)
        yield {"steps": [step("retry", value=10), step("init", subject=b"aa", pattern=b"a"), step("regs"),
                         mutation, step("getregs"), step("regs")]}
    for setting, value, op in itertools.product(["retry", "stack"], [0, 1, 10, 100, 2**32-1, 2**32, -1], ["replace", "ireplace"]):
        yield {"steps": [step(setting, value=value), step(op, pattern=b"(a+)+$", replacement=b"X", subject=b"a"*12+b"!")]}


def main():
    """Record deterministic outputs from independent pinned PHP requests."""
    inputs = list(cases())
    with ThreadPoolExecutor(max_workers=4) as workers:
        output = b"".join(workers.map(capture, inputs))
    destination = ROOT / "crates/elephc-mbstring/tests/fixtures/regex_replace.jsonl.gz"
    destination.write_bytes(gzip.compress(output, mtime=0))
    print(f"Captured {len(inputs)} mbregex replacement traces in {destination}")


if __name__ == "__main__":
    main()
