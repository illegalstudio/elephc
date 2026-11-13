#!/usr/bin/env python3
"""Compare the benchmark body time of Elephc, PHP and FrankenPHP.

Each selected native fixture is copied to a temporary directory and instrumented in
place with a top-level `hrtime(true)` start and a body-time marker. The original
program is otherwise unchanged, so Elephc's stored-map lowering and PHP's
parse/execute boundary stay intact. PHP parses the file before the timed region, so
parse time is excluded for every runtime.
"""

from __future__ import annotations

import argparse
import json
import math
import os
import platform
import shutil
import statistics
import subprocess
import tempfile
from datetime import datetime, timezone
from pathlib import Path

import cases
import common


LABELS = ("elephc", "php", "php-jit", "frankenphp", "frankenphp-jit")
MARKER = "__ELEPHC_BODY_NS="


def instrument(source: str) -> str:
    """Insert a top-level `hrtime` start and a body-time marker into a fixture.

    The original top-level program is compiled unchanged apart from the two
    instrumentation statements, so the stored-map lowering sees the same `$argc`
    and the same local types as the standalone benchmark. Injecting after an
    opening `declare(strict_types=1);` keeps that directive first.
    """
    opening = source.find("<?php")
    if opening < 0:
        raise ValueError("fixture has no PHP opening tag")
    insert_at = opening + len("<?php")
    remainder = source[insert_at:]
    stripped = remainder.lstrip()
    if stripped.startswith("declare("):
        end = source.find(");", insert_at)
        if end < 0:
            raise ValueError("unterminated declare directive")
        insert_at = end + 2
    started = (
        source[:insert_at]
        + "\n$__elephc_start = hrtime(true);"
        + source[insert_at:]
    )
    return started + '\necho "\\n__ELEPHC_BODY_NS=", hrtime(true) - $__elephc_start, "\\n";\n'


def parse_args() -> argparse.Namespace:
    """Parse the fixture selection and sampling budget."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", required=True, type=Path,
                        help="path to the Elephc release compiler")
    parser.add_argument("--frankenphp", type=Path,
                        help="path to the FrankenPHP binary (also FRANKENPHP_BIN)")
    parser.add_argument("--include", action="append", default=[], metavar="CASE",
                        help="native case to include (repeatable; default: all)")
    parser.add_argument("--exclude", action="append", default=[], metavar="CASE",
                        help="native case to skip (repeatable)")
    parser.add_argument("--iterations", type=int, default=5)
    parser.add_argument("--warmup", type=int, default=2)
    parser.add_argument("--timeout", type=float, default=300)
    parser.add_argument("--json", type=Path)
    args = parser.parse_args()
    if args.iterations < 1 or args.warmup < 0 or args.timeout <= 0:
        parser.error("iterations must be positive, warmup nonnegative, timeout positive")
    return args


def git_revision() -> str:
    """Return the current source revision for the record."""
    try:
        return subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=common.REPO, capture_output=True,
            text=True, timeout=30, check=True,
        ).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        return "unknown"


def parse_body(output: str, expected: str) -> float:
    """Validate the fixture result and return the wrapped body time in milliseconds."""
    prefix, separator, tail = output.partition(MARKER)
    if not separator:
        raise RuntimeError(f"missing body marker in output: {output!r}")
    if prefix.rstrip("\n") != expected.rstrip("\n"):
        raise RuntimeError(
            f"fixture output mismatch: expected {expected.rstrip()!r}, "
            f"got {prefix.rstrip()!r}"
        )
    try:
        nanoseconds = int(tail.split()[0])
    except (IndexError, ValueError) as error:
        raise RuntimeError(f"invalid body timing tail: {tail!r}") from error
    return nanoseconds / 1_000_000


def measure_body(command: list[str], cwd: Path, expected: str, iterations: int,
                 warmup: int, timeout: float, env: dict | None = None) -> float:
    """Run warmups and measured samples, returning the median body time."""
    samples = []
    for index in range(warmup + iterations):
        result = subprocess.run(
            command, cwd=cwd, capture_output=True, text=True,
            timeout=timeout, env=env,
        )
        if result.returncode:
            raise RuntimeError(
                f"command failed: {command!r}\n{result.stdout}\n{result.stderr}"
            )
        elapsed = parse_body(result.stdout, expected)
        if index >= warmup:
            samples.append(elapsed)
    return statistics.median(samples)


def geometric_mean(values: list[float]) -> float | None:
    """Return the geometric mean used for cross-case summaries."""
    if not values:
        return None
    return math.exp(sum(math.log(value) for value in values) / len(values))


def main() -> None:
    """Run the body-time comparison and print a Markdown table."""
    args = parse_args()
    compiler = args.compiler.resolve()
    if not compiler.is_file():
        raise SystemExit(f"Elephc compiler not found: {compiler}")
    frankenphp = args.frankenphp or os.environ.get("FRANKENPHP_BIN") or shutil.which("frankenphp")
    if not frankenphp:
        raise SystemExit("FrankenPHP binary not found; pass --frankenphp or set FRANKENPHP_BIN")
    frankenphp = str(Path(frankenphp).resolve())
    included = set(args.include)
    excluded = set(args.exclude)
    discovered = cases.discover(["native"], [])
    selected = [
        case for case in discovered.get("native", [])
        if case["name"] not in excluded and (not included or case["name"] in included)
    ]
    if not selected:
        raise SystemExit("no native benchmark cases selected")

    php = shutil.which("php")
    if not php:
        raise SystemExit("PHP interpreter not found")
    nojit_ini = None
    jit_ini = None
    with tempfile.TemporaryDirectory(prefix="elephc-frankenphp-body-") as temporary:
        ini_dir = Path(temporary)
        nojit_ini = ini_dir / "frankenphp.ini"
        jit_ini = ini_dir / "frankenphp-jit.ini"
        nojit_ini.write_text("opcache.enable_cli=0\nopcache.jit=0\n")
        jit_ini.write_text(
            "opcache.enable_cli=1\nopcache.jit_buffer_size=64M\nopcache.jit=tracing\n"
        )
        rows = []
        for case in selected:
            expected = (case["directory"] / "expected.txt").read_text()
            if not expected.endswith("\n"):
                expected += "\n"
            with tempfile.TemporaryDirectory(prefix="elephc-body-case-") as run_temporary:
                run_dir = Path(run_temporary) / case["name"]
                shutil.copytree(case["directory"], run_dir)
                (run_dir / "timed.php").write_text(
                    instrument((run_dir / "main.php").read_text())
                )
                commands = {}
                common.run_process([str(compiler), "timed.php"], run_dir, args.timeout)
                commands["elephc"] = [str(run_dir / "timed")]
                commands["php"] = [php, "-d", "opcache.enable_cli=0",
                                   "-d", "opcache.jit=0", "timed.php"]
                commands["php-jit"] = [php, "-d", "opcache.enable_cli=1",
                                       "-d", "opcache.jit_buffer_size=64M",
                                       "-d", "opcache.jit=tracing", "timed.php"]
                commands["frankenphp"] = [frankenphp, "php-cli", "timed.php"]
                commands["frankenphp-jit"] = [frankenphp, "php-cli", "timed.php"]
                envs = {label: None for label in LABELS}
                plain_env = os.environ.copy()
                plain_env["PHPRC"] = str(nojit_ini)
                jit_env = os.environ.copy()
                jit_env["PHPRC"] = str(jit_ini)
                envs["frankenphp"] = plain_env
                envs["frankenphp-jit"] = jit_env
                variants = {
                    label: measure_body(
                        commands[label], run_dir, expected, args.iterations,
                        args.warmup, args.timeout, envs[label],
                    )
                    for label in LABELS
                }
            rows.append({"case": case["name"], "body_ms": variants})
            print(f"measured {case['name']}", flush=True)

    print()
    print("| case | " + " | ".join(LABELS) + " | elephc×vs FP | elephc×vs FP+JIT |")
    print("|---|" + "---:|" * (len(LABELS) + 2))
    fp_ratios = []
    fp_jit_ratios = []
    for row in rows:
        body = row["body_ms"]
        fp = body["frankenphp"] / body["elephc"]
        fp_jit = body["frankenphp-jit"] / body["elephc"]
        fp_ratios.append(fp)
        fp_jit_ratios.append(fp_jit)
        print(
            f"| {row['case']} | "
            + " | ".join(f"{body[label]:.3f}" for label in LABELS)
            + f" | {fp:.2f}x | {fp_jit:.2f}x |"
        )
    print()
    fp_geomean = geometric_mean(fp_ratios)
    fp_jit_geomean = geometric_mean(fp_jit_ratios)
    print(f"Geometric mean Elephc body speedup vs FrankenPHP: **{fp_geomean:.2f}x**")
    print(f"Elephc faster on {sum(value > 1 for value in fp_ratios)}/{len(fp_ratios)} cases")
    print(f"Geometric mean Elephc body speedup vs FrankenPHP+JIT: **{fp_jit_geomean:.2f}x**")
    print(f"Elephc faster on {sum(value > 1 for value in fp_jit_ratios)}/{len(fp_jit_ratios)} cases")
    print("Instrumentation is inserted directly into the fixture; PHP parses the file "
          "before the timed region, so parse time is excluded for every runtime.")

    if args.json:
        record = {
            "generated_at": datetime.now(timezone.utc).isoformat(),
            "platform": platform.platform(),
            "machine": platform.machine(),
            "compiler": str(compiler),
            "compiler_revision": git_revision(),
            "frankenphp": frankenphp,
            "iterations": args.iterations,
            "warmup": args.warmup,
            "frankenphp_geomean_speedup": fp_geomean,
            "frankenphp_jit_geomean_speedup": fp_jit_geomean,
            "cases": rows,
        }
        args.json.parent.mkdir(parents=True, exist_ok=True)
        args.json.write_text(json.dumps(record, indent=2) + "\n")
        print(f"wrote {args.json}")


if __name__ == "__main__":
    main()
