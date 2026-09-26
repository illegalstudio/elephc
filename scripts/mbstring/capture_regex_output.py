#!/usr/bin/env python3
"""Capture typed/untyped output references, destructor reentry, and pending exceptions in mbregex."""

from concurrent.futures import ThreadPoolExecutor
import gzip
import itertools
import json
import subprocess

from capture_regex_request import ROOT, step


def cases():
    """Keep ownership singular so replacing the output actually runs its previous object's destructor."""
    actions = [[], [step("encoding", value=b"ASCII"), step("options", value=b"ir")],
               [step("retry", value=1), step("init", subject=b"ba", pattern=b"a"), step("regs")]]
    for storage, pattern, subject, mutate, throwing, action, op in itertools.product(
        ["local", "mixed", "union", "object", "int"],
        [b"a", b"(?<x>a)?(b*)", b"[", b"", b"(a+)+$"],
        ["éAa".encode(), b"a"*12+b"!"], [False, True], [False, True], actions, ["mb_ereg", "mb_eregi"],
    ):
        yield {"storage": storage, "pattern": pattern.hex(), "subject": subject.hex(), "mutate": mutate,
               "throw": throwing, "actions": action, "op": op,
               "before": [step("init", pattern=b"a", subject=b"aa"), step("regs")]}


def capture(case):
    """Execute one real PHP call without keeping a second owner of the overwritten output object."""
    result = subprocess.run(["php", "-d", "display_errors=0", "-d", "log_errors=0",
                             str(ROOT / "scripts/mbstring/capture_regex_output.php")],
                            input=json.dumps(case).encode(), capture_output=True, timeout=8)
    assert result.returncode == 0 and not result.stderr, (case, result.returncode, result.stderr)
    return json.dumps(json.loads(result.stdout), separators=(",", ":")).encode() + b"\n"


def main():
    """Write independent traces in deterministic order for host-protocol development and replay."""
    inputs = list(cases())
    with ThreadPoolExecutor(max_workers=4) as workers:
        output = b"".join(workers.map(capture, inputs))
    destination = ROOT / "crates/elephc-mbstring/tests/fixtures/regex_output.jsonl.gz"
    destination.write_bytes(gzip.compress(output, mtime=0))
    print(f"Captured {len(inputs)} output-reference traces in {destination}")


if __name__ == "__main__":
    main()
