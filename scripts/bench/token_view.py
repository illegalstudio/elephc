#!/usr/bin/env python3
"""Compare TOKEN_PARSE validation cost between the token view and the text view.

The script builds `elephc-chopper`'s benchmark example once, then alternates the two
views on the same generated fixtures. Results are medians of per-process runs, so a
single scheduling hiccup does not decide the comparison. Reproduce the PR numbers with:

    python3 scripts/bench/token_view.py

Set `ELEPHC_TOKEN_VIEW=text` inside the child processes only; the parent environment is
left untouched.
"""

from __future__ import annotations

import argparse
import os
import statistics
import subprocess
from pathlib import Path

SHAPES = ("plain", "hooks", "strings", "namespaced", "fallback")
ROOT = Path(__file__).resolve().parents[2]
EXAMPLE = "token_view_bench"


def build_example() -> None:
    """Builds the release benchmark example."""
    subprocess.run(
        ["cargo", "build", "--release", "-p", "elephc-chopper", "--example", EXAMPLE],
        cwd=ROOT,
        check=True,
    )


def binary_path() -> Path:
    """Returns the release example path for the host target directory."""
    return ROOT / "target" / "release" / "examples" / EXAMPLE


def run_once(binary: Path, shape: str, rounds: int, view: str) -> int:
    """Runs one benchmark process and returns its nanoseconds-per-call value."""
    environment = os.environ.copy()
    if view == "text":
        environment["ELEPHC_TOKEN_VIEW"] = "text"
    else:
        environment.pop("ELEPHC_TOKEN_VIEW", None)
    result = subprocess.run(
        [str(binary), shape, str(rounds)],
        cwd=ROOT,
        env=environment,
        text=True,
        capture_output=True,
        check=True,
    )
    field = next(
        part for part in result.stdout.split() if part.startswith("ns_per_call=")
    )
    return int(field.split("=", 1)[1])


def main() -> None:
    """Runs the alternating token/text benchmark and prints medians."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rounds", type=int, default=200, help="rounds per process")
    parser.add_argument("--samples", type=int, default=9, help="process samples per view")
    parser.add_argument(
        "--shapes",
        default=",".join(SHAPES),
        help=f"comma-separated shapes from: {', '.join(SHAPES)}",
    )
    arguments = parser.parse_args()
    shapes = [shape for shape in arguments.shapes.split(",") if shape]
    unknown = sorted(set(shapes) - set(SHAPES))
    if unknown:
        raise SystemExit(f"unknown shapes: {', '.join(unknown)}")

    build_example()
    binary = binary_path()
    print(f"{'shape':<12}{'text ns':>14}{'token ns':>14}{'reduction':>12}")
    for shape in shapes:
        text_samples = []
        token_samples = []
        for sample in range(arguments.samples):
            # Alternate the order so a slow drift in the machine does not always penalize
            # the same view.
            if sample % 2 == 0:
                text_samples.append(run_once(binary, shape, arguments.rounds, "text"))
                token_samples.append(run_once(binary, shape, arguments.rounds, "token"))
            else:
                token_samples.append(run_once(binary, shape, arguments.rounds, "token"))
                text_samples.append(run_once(binary, shape, arguments.rounds, "text"))
        text = statistics.median(text_samples)
        token = statistics.median(token_samples)
        reduction = (text - token) / text * 100 if text else 0.0
        print(f"{shape:<12}{text:>14}{token:>14}{reduction:>11.1f}%")


if __name__ == "__main__":
    main()
