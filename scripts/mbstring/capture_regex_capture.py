#!/usr/bin/env python3
"""Capture PHP mb_ereg/mb_eregi outputs and their observable reference-initialization order."""

from concurrent.futures import ThreadPoolExecutor
import gzip
import itertools

from capture_regex_request import ROOT, capture, step


def string(value):
    """Encode a PHP byte string independently of JSON's Unicode string representation."""
    return {"bytes": value.hex()}


def cases():
    """Exercise capture keys, empty/unmatched groups, output resets, aliases, cache effects, and failures."""
    for pattern, subject, options, op in itertools.product(
        [b"a", b"(a)(b)?", b"(?<x>a)|(?<x>b)", b"(?<x>a)?(b*)", b"(?<empty>)", b"a*", b"$", b"[", b"", b"\0"],
        [b"", b"aAbba", "éαa".encode(), b"a\xffb"],
        [b"pr", b"ir", b"b", b"n"], ["ereg", "eregi"],
    ):
        yield {"steps": [step("options", value=options), step(op, pattern=pattern, subject=subject)]}
    seeds = [None, False, True, 17, 2.5, string(b"old\0\xff"), {"array": []},
             {"array": [[string(b"key"), string(b"old")], [7, {"array": [[0, 12]]}]]}]
    for seed, pattern, subject, supplied, op in itertools.product(
        seeds, [b"", b"[", b"(?<x>a)?(b*)"], [b"b", b"\xff"], [False, True], ["ereg", "eregi"]
    ):
        yield {"steps": [step(op, pattern=pattern, subject=subject, matches=seed, with_matches=supplied)]}
    encodings = [("UTF-8", "utf-8", "éα🦀"), ("UTF-16", "utf-16be", "éα🦀"),
                 ("UTF-16LE", "utf-16le", "éα🦀"), ("UCS-4", "utf-32be", "éα🦀"),
                 ("UCS-4LE", "utf-32le", "éα🦀"), ("SJIS", "shift_jis", "猫表東京"),
                 ("SJIS-WIN", "cp932", "猫表東京"), ("EUC-JP", "euc_jp", "猫東京"),
                 ("BIG5", "big5", "中文"), ("EUC-CN", "gb2312", "中文"),
                 ("EUC-KR", "euc_kr", "한국"), ("KOI8R", "koi8_r", "аБ")]
    for encoding, codec, text in encodings:
        for pattern, op in itertools.product(["(.)", "(?<x>.)", "(?<x>)(.*)", "$", ".*?"], ["ereg", "eregi"]):
            yield {"steps": [step("encoding", value=encoding.encode()),
                             step(op, pattern=pattern.encode(codec), subject=text.encode(codec))]}
    for encoding, op in itertools.product(
        ["ASCII", "EUC-TW"] + [f"ISO-8859-{index}" for index in [1,2,3,4,5,6,7,8,9,10,11,13,14,15,16]], ["ereg", "eregi"]
    ):
        yield {"steps": [step("encoding", value=encoding.encode()), step(op, pattern=b"(a)", subject=b"aAb")]}
    for before, after, op in itertools.product([b"SJIS", b"SJIS-WIN"], [b"SJIS", b"SJIS-WIN"], ["ereg", "eregi"]):
        yield {"steps": [step("encoding", value=before), step("init", pattern=b"\xfa\x40", subject=b"\xfa\x40a"),
                         step("encoding", value=after), step(op, pattern=b"\xfa\x40", subject=b"\xfa\x40"), step("regs")]}
    for options, op in itertools.product([b"pr", b"ir", b"b", b"br"], ["ereg", "eregi"]):
        yield {"steps": [step("init", subject=b"aab", pattern=b"a"), step("regs"), step("options", value=options),
                         step(op, pattern=b"a", subject=b"Aa,a"), step("getregs"), step("regs")]}
    nested = [step("init", subject=b"bb", pattern=b"b"), step("regs"), step("options", value=b"ir"),
              step("ereg", pattern=b"b", subject=b"AbB")]
    for throwing, supplied, op in itertools.product([False, True], [False, True], ["ereg", "eregi"]):
        mutation = step(op, pattern=b"[", subject=b"aa", with_matches=supplied, warning_matches=string(b"handler"))
        mutation.update(on_warning=nested, throw=throwing)
        yield {"steps": [step("init", subject=b"aa", pattern=b"a"), step("regs"), mutation, step("getregs"), step("regs")]}
    for setting, value, op in itertools.product(["retry", "stack"], [0, 1, 10, 100, 2**32-1, 2**32, -1], ["ereg", "eregi"]):
        yield {"steps": [step(setting, value=value), step(op, pattern=b"(a+)+$", subject=b"a"*12+b"!")]}


def main():
    """Write deterministic traces from fresh pinned PHP processes in generation order."""
    inputs = list(cases())
    with ThreadPoolExecutor(max_workers=4) as workers:
        output = b"".join(workers.map(capture, inputs))
    destination = ROOT / "crates/elephc-mbstring/tests/fixtures/regex_capture.jsonl.gz"
    destination.write_bytes(gzip.compress(output, mtime=0))
    print(f"Captured {len(inputs)} mb_ereg/mb_eregi traces in {destination}")


if __name__ == "__main__":
    main()
