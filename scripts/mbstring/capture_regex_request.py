#!/usr/bin/env python3
"""Capture isolated PHP mbregex sequences covering cache and progressive state transitions."""

from concurrent.futures import ThreadPoolExecutor
import gzip
import itertools
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def step(op, **values):
    """Encode typed byte strings while retaining omitted/null inputs and integer offsets."""
    return {"op": op, **{name: value.hex() if isinstance(value, bytes) else value for name, value in values.items()}}


def cases():
    """Exercise ordinary searches and mutations after an existing match or an absent subject."""
    for pattern, subject, options in itertools.product(
        [b"a", b".", b"a*", b"$", b"(?<x>a)|(?<x>b)", b"(?<x>a)?(b*)", b"(?i)a"],
        [b"", b"ababa", b"B\na", "éα".encode(), b"a\xffb"],
        [b"pr", b"ir", b"b", b"j"],
    ):
        yield {"steps": [step("init", subject=subject, pattern=pattern, options=options)] +
               [step(op) for op in ["pos", "getregs", "regs", "getregs", "search", "getregs", "search"]]}
    mutations = [
        step("encoding", value=b"ASCII"), step("encoding", value=b"UTF-16LE"), step("encoding", value=b"invalid"),
        step("options", value=b"i"), step("options", value=b"iq"),
        step("init", subject=b"bb"), step("init", subject=b"bb", pattern=b"b"),
        step("init", subject=b"\xff"), step("init", subject=b"bb", pattern=b"["),
        step("init", subject=b"bb", pattern=b""), step("init", subject=b"bb", pattern=b"b", options=b"iQ"),
        step("match", pattern=b"a", subject=b"a", options=b"ir"),
        step("match", pattern=b"a", subject=b"a", options=b"pr"),
        step("match", pattern=b"[", subject=b"a"), step("match", pattern=b"a", subject=b"\xff", options=b"i"),
    ]
    mutations += [step("setpos", value=value) for value in [-(2**63), -4, -3, -1, 0, 2, 3, 4, 2**63-1]]
    mutations += [step(op, pattern=pattern, options=options)
                  for op, pattern, options in itertools.product(["search", "pos", "regs"], [None, b"", b"a", b"["], [None, b"i", b"iQ", b"jbQ", b"\0", b"\xff"])]
    for seed, mutation in itertools.product([False, True], mutations):
        steps = [step("init", subject=b"aab", pattern=b"a"), step("regs")] if seed else []
        yield {"steps": steps + [mutation, step("search"), step("getregs"), step("setpos", value=0), step("regs")]}
    for before, after in itertools.product([b"SJIS", b"SJIS-WIN"], repeat=2):
        yield {"steps": [step("encoding", value=before), step("init", subject=b"a\xfa\x40", pattern=b"."),
                         step("encoding", value=after), step("match", pattern=b".", subject=b"\xfa\x40"), step("regs"), step("regs")]}
    for options in [b"r", b"ir", b"br", b"b"]:
        yield {"steps": [step("init", subject=b"aaa", pattern=b"(?i)a", options=options), step("regs"),
                         step("match", pattern=b"(?i)a", subject=b"a", options=options), step("regs")]}
    for limit in [0, 1, 10, 100, 2**32-1, 2**32, -1]:
        yield {"steps": [step("retry", value=limit), step("init", subject=b"a"*12+b"!", pattern=b"(a+)+$", options=b"r"),
                         step("search"), step("getregs"), step("regs", pattern=b"a")]}
    for op in ["match", "search"]:
        yield {"steps": [step("encoding", value=b"SJIS-WIN"), step("init", subject=b"\xfa\x40\xfa\x40", pattern=b"\xfa\x40"),
                         step("encoding", value=b"SJIS"), step(op, pattern=b"\xfa\x40", subject=b"\xfa\x40" if op == "match" else b""),
                         step("regs"), step("getregs")]}
    nested = [step("init", subject=b"bb", pattern=b"b"), step("regs"), step("options", value=b"ir")]
    for op, throwing, invalid_options in itertools.product(["match", "init", "search", "regs"], [False, True], [False, True]):
        mutation = step(op, pattern=b"[", subject=b"aa", options=b"iQ" if invalid_options else None)
        mutation.update(on_warning=nested, throw=throwing)
        yield {"steps": [step("init", subject=b"aab", pattern=b"a"), step("regs"), mutation, step("search"), step("getregs")]}
    for throwing in [False, True]:
        mutation = step("search")
        mutation.update(on_warning=nested, throw=throwing)
        yield {"steps": [step("retry", value=10), step("init", subject=b"a"*12+b"!", pattern=b"(a+)+$", options=b"r"),
                         mutation, step("getregs"), step("search"), step("getregs")]}
    for subject, pattern, rewind in itertools.product(
        [b"\xc3", b"\xe2\x82", b"\xf0\x90\x80", b"a\xc3b", b"\x80", b"\xff"], [b"$", b".", b"a*"], [False, True]
    ):
        steps = [step("init", subject=subject, pattern=pattern)]
        if rewind:
            steps.append(step("setpos", value=0))
        yield {"steps": steps + [step("regs"), step("getregs"), step("pos"), step("getregs")]}
    for encoding, codec in [(b"UTF-16LE", "utf-16le"), (b"UTF-16", "utf-16be"), (b"UCS-4", "utf-32be"), (b"UCS-4LE", "utf-32le")]:
        for pattern, rewind in itertools.product(["$", "."], [False, True]):
            steps = [step("encoding", value=encoding), step("init", subject="ab".encode(codec)[:-1], pattern=pattern.encode(codec))]
            if rewind:
                steps.append(step("setpos", value=0))
            yield {"steps": steps + [step("regs"), step("getregs"), step("pos"), step("getregs")]}
    for encoding, codec in [(b"UTF-8", "utf-8"), (b"UTF-16LE", "utf-16le"), (b"UTF-16", "utf-16be"), (b"UCS-4", "utf-32be"), (b"UCS-4LE", "utf-32le")]:
        subject = "éα🦀".encode(codec)
        for pattern, offset in itertools.product([".", "$", "(.*)"], range(len(subject)+1)):
            yield {"steps": [step("encoding", value=encoding), step("init", subject=subject, pattern=pattern.encode(codec)),
                             step("setpos", value=offset), step("pos"), step("getregs"), step("regs")]}


def capture(case):
    """Use a new PHP process so unavailable subjects, empty caches, and default options are authentic."""
    result = subprocess.run(["php", str(ROOT / "scripts/mbstring/capture_regex_request.php")],
                            input=json.dumps(case).encode(), capture_output=True, timeout=8)
    assert result.returncode == 0, (case, result.returncode, result.stderr.decode(errors="replace"))
    parsed = json.loads(result.stdout)
    assert len(parsed["trace"]) == len(case["steps"])
    return json.dumps(parsed, separators=(",", ":")).encode() + b"\n"


def main():
    """Write deterministic complete traces in input order while isolating independent PHP requests."""
    inputs = list(cases())
    with ThreadPoolExecutor(max_workers=4) as workers:
        output = b"".join(workers.map(capture, inputs))
    destination = ROOT / "crates/elephc-mbstring/tests/fixtures/regex_request.jsonl.gz"
    destination.write_bytes(gzip.compress(output, mtime=0))
    print(f"Captured {len(inputs)} mbregex request traces in {destination}")


if __name__ == "__main__":
    main()
