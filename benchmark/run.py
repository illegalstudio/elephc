#!/usr/bin/env python3
"""Run all process benchmarks through one CLI, or a focused internal stopwatch."""

from __future__ import annotations

import argparse
import subprocess
from pathlib import Path

from cases import discover
from common import REPO, SUITES, atomic_json, render_markdown, require_baselines, result_metadata, run_case

INTERNAL = {
    "script-cache": ["cargo", "test", "-p", "elephc-magician", "file_cache_format", "--", "--ignored", "--nocapture"],
    "null-representation": ["cargo", "test", "--test", "codegen_tests", "null_sentinel::benches", "--", "--ignored", "--nocapture"],
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--suite", choices=(*SUITES, "all"), default="all")
    parser.add_argument("--case", action="append", default=[], help="Name or suite/name; repeatable")
    parser.add_argument("--iterations", type=int, default=5)
    parser.add_argument("--warmup", type=int, default=1)
    parser.add_argument("--timeout", type=float, default=120, help="Timeout per compilation or execution in seconds")
    parser.add_argument("--compiler", type=Path, help="Use an explicitly selected compiler without building")
    parser.add_argument("--no-build", action="store_true", help="Use an existing release compiler (CI builds it first)")
    parser.add_argument("--require-baselines", action="store_true", help="Fail if a selected PHP or C baseline is unavailable")
    parser.add_argument("--json", type=Path)
    parser.add_argument("--markdown", type=Path)
    parser.add_argument("--list", action="store_true")
    parser.add_argument("--internal", choices=INTERNAL, help="Manual Rust microbenchmark; separate measurement scope")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    if args.internal:
        if args.json or args.markdown or args.case or args.compiler or args.no_build or args.require_baselines or args.suite != "all":
            raise SystemExit("--internal runs an internal stopwatch and cannot be combined with suite/output options")
        subprocess.run(INTERNAL[args.internal], cwd=REPO, check=True)
        return
    if args.iterations < 1 or args.warmup < 0 or args.timeout <= 0:
        raise SystemExit("iterations must be positive, warmup non-negative, and timeout positive")
    suites = list(SUITES) if args.suite == "all" else [args.suite]
    try:
        cases = discover(suites, args.case)
    except ValueError as error:
        raise SystemExit(str(error)) from error
    if args.list:
        for suite, fixtures in cases.items():
            for fixture in fixtures:
                print(f"{suite}/{fixture['name']}")
        return
    if not cases:
        raise SystemExit("no benchmark cases selected")
    if args.require_baselines:
        try:
            require_baselines(cases)
        except ValueError as error:
            raise SystemExit(str(error)) from error
    compiler = args.compiler.resolve() if args.compiler else REPO / "target" / "release" / "elephc"
    if not args.compiler and not args.no_build:
        # Always ask Cargo to refresh the selected build, even when a binary exists.
        command = ["cargo", "build", "--release", "-p", "elephc"]
        if "magician" in cases:
            command += ["-p", "elephc-magician"]
        subprocess.run(command, cwd=REPO, check=True)
    if not compiler.is_file():
        raise SystemExit(f"compiler does not exist: {compiler}")
    payload = result_metadata(args.iterations, args.warmup)
    payload["suites"] = {suite: {"cases": [run_case(case, compiler, args.iterations, args.warmup, args.timeout)
                                        for case in fixtures]} for suite, fixtures in cases.items()}
    markdown = render_markdown(payload)
    print(markdown, end="")
    if args.json:
        atomic_json(args.json, payload)
    if args.markdown:
        args.markdown.parent.mkdir(parents=True, exist_ok=True)
        args.markdown.write_text(markdown)


if __name__ == "__main__":
    main()
