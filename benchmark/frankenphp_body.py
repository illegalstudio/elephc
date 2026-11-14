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
    parser.add_argument("--require-ahead", action="store_true",
                        help="fail when Elephc is not ahead on body time, CPU or peak RSS")
    parser.add_argument("--gate-tolerance", type=float, default=0.0,
                        help="allowed relative shortfall per metric under --require-ahead "
                             "(default: 0, a strict win)")
    args = parser.parse_args()
    if args.iterations < 1 or args.warmup < 0 or args.timeout <= 0:
        parser.error("iterations must be positive, warmup nonnegative, timeout positive")
    if not 0 <= args.gate_tolerance < 1:
        parser.error("gate tolerance must be in [0, 1)")
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
                 warmup: int, timeout: float, env: dict | None = None) -> dict:
    """Run warmups and measured samples, returning body time plus CPU and peak RSS."""
    body_samples = []
    cpu_samples = []
    rss_samples = []
    for index in range(warmup + iterations):
        output, _stderr, usage = common.run_process_measured(
            command, cwd, timeout, env=env
        )
        elapsed = parse_body(output, expected)
        if index >= warmup:
            body_samples.append(elapsed)
            cpu_samples.append((usage.ru_utime + usage.ru_stime) * 1000)
            rss_samples.append(common.peak_rss_kb(usage))
    return {
        "body_ms": statistics.median(body_samples),
        "body_samples_ms": body_samples,
        "cpu_ms": statistics.median(cpu_samples),
        "cpu_samples_ms": cpu_samples,
        "rss_kb": statistics.median(rss_samples),
        "rss_samples_kb": rss_samples,
    }


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
        nojit_ini.write_text(
            "opcache.enable=0\nopcache.enable_cli=0\nopcache.jit=0\n"
            "opcache.file_update_protection=0\n"
        )
        jit_ini.write_text(
            "opcache.enable=1\nopcache.enable_cli=1\nopcache.jit_buffer_size=64M\n"
            "opcache.jit=tracing\nopcache.file_update_protection=0\n"
        )
        problem = common.php_jit_problem(php)
        if problem:
            raise SystemExit(problem)
        problem = common.frankenphp_jit_problem(frankenphp, jit_ini)
        if problem:
            raise SystemExit(problem)
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
                                   "-d", "opcache.jit=0",
                                   "-d", "opcache.file_update_protection=0", "timed.php"]
                commands["php-jit"] = common.php_command(php, "timed.php")
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
            rows.append({"case": case["name"], "variants": variants})
            print(f"measured {case['name']}", flush=True)

    print()
    print("| case | " + " | ".join(LABELS) + " | elephc×vs FP | elephc×vs FP+JIT |")
    print("|---|" + "---:|" * (len(LABELS) + 2))
    fp_ratios = []
    fp_jit_ratios = []
    for row in rows:
        variants = row["variants"]
        fp = variants["frankenphp"]["body_ms"] / variants["elephc"]["body_ms"]
        fp_jit = variants["frankenphp-jit"]["body_ms"] / variants["elephc"]["body_ms"]
        fp_ratios.append(fp)
        fp_jit_ratios.append(fp_jit)
        print(
            f"| {row['case']} | "
            + " | ".join(f"{variants[label]['body_ms']:.3f}" for label in LABELS)
            + f" | {fp:.2f}x | {fp_jit:.2f}x |"
        )
    print()
    fp_geomean = geometric_mean(fp_ratios)
    fp_jit_geomean = geometric_mean(fp_jit_ratios)
    print(f"Geometric mean Elephc body speedup vs FrankenPHP: **{fp_geomean:.2f}x**")
    print(f"Elephc faster on {sum(value > 1 for value in fp_ratios)}/{len(fp_ratios)} cases")
    print(f"Geometric mean Elephc body speedup vs FrankenPHP+JIT: **{fp_jit_geomean:.2f}x**")
    print(f"Elephc faster on {sum(value > 1 for value in fp_jit_ratios)}/{len(fp_jit_ratios)} cases")
    print()
    resource_labels = ("elephc", "php", "php-jit", "frankenphp", "frankenphp-jit")
    print("| case | " + " | ".join(f"{label} CPU ms" for label in resource_labels)
          + " | CPU×FP | CPU×FP+JIT |")
    print("|---|" + "---:|" * (len(resource_labels) + 2))
    cpu_fp_ratios = []
    cpu_jit_ratios = []
    rss_fp_ratios = []
    rss_jit_ratios = []
    for row in rows:
        variants = row["variants"]
        cpu = {label: variants[label]["cpu_ms"] for label in resource_labels}
        cpu_fp = variants["frankenphp"]["cpu_ms"] / variants["elephc"]["cpu_ms"]
        cpu_jit = variants["frankenphp-jit"]["cpu_ms"] / variants["elephc"]["cpu_ms"]
        cpu_fp_ratios.append(cpu_fp)
        cpu_jit_ratios.append(cpu_jit)
        print(
            f"| {row['case']} | "
            + " | ".join(f"{cpu[label]:.2f}" for label in resource_labels)
            + f" | {cpu_fp:.2f}x | {cpu_jit:.2f}x |"
        )
    print()
    print("| case | " + " | ".join(f"{label} RSS MB" for label in resource_labels)
          + " | RSS×FP | RSS×FP+JIT |")
    print("|---|" + "---:|" * (len(resource_labels) + 2))
    for row in rows:
        variants = row["variants"]
        rss = {label: variants[label]["rss_kb"] / 1024 for label in resource_labels}
        rss_fp = rss["frankenphp"] / rss["elephc"]
        rss_jit = rss["frankenphp-jit"] / rss["elephc"]
        rss_fp_ratios.append(rss_fp)
        rss_jit_ratios.append(rss_jit)
        print(
            f"| {row['case']} | "
            + " | ".join(f"{rss[label]:.2f}" for label in resource_labels)
            + f" | {rss_fp:.2f}x | {rss_jit:.2f}x |"
        )
    print()
    cpu_fp_geomean = geometric_mean(cpu_fp_ratios)
    cpu_jit_geomean = geometric_mean(cpu_jit_ratios)
    rss_fp_geomean = geometric_mean(rss_fp_ratios)
    rss_jit_geomean = geometric_mean(rss_jit_ratios)
    print(f"Geometric mean CPU efficiency vs FrankenPHP: **{cpu_fp_geomean:.2f}x** "
          f"(higher is less CPU)")
    print(f"Geometric mean CPU efficiency vs FrankenPHP+JIT: **{cpu_jit_geomean:.2f}x**")
    print(f"Geometric mean RAM efficiency vs FrankenPHP: **{rss_fp_geomean:.2f}x** "
          f"(higher is less peak RSS)")
    print(f"Geometric mean RAM efficiency vs FrankenPHP+JIT: **{rss_jit_geomean:.2f}x**")
    print("Instrumentation is inserted directly into the fixture; PHP parses the file "
          "before the timed region, so parse time is excluded for every runtime. "
          "CPU is child user+sys and RSS is the child's peak resident set.")

    if args.require_ahead:
        regressions = []
        for row in rows:
            variants = row["variants"]
            elephc = variants["elephc"]
            for label in ("frankenphp", "frankenphp-jit"):
                other = variants[label]
                for metric, key in (
                    ("body", "body_ms"),
                    ("cpu", "cpu_ms"),
                    ("rss", "rss_kb"),
                ):
                    ours = elephc[key]
                    theirs = other[key]
                    limit = theirs * (1 + args.gate_tolerance)
                    if not (ours > 0 and theirs > 0 and ours <= limit):
                        regressions.append(
                            f"{row['case']}: {metric} {ours:.3f} vs {label} {theirs:.3f}"
                        )
        if regressions:
            print("Efficiency gate failed:")
            for line in regressions:
                print(f"- {line}")
            raise SystemExit(1)
        print(f"Efficiency gate: Elephc ahead on {len(rows)}/{len(rows)} cases "
              "for body time, CPU and peak RSS")

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
            "frankenphp_cpu_geomean": cpu_fp_geomean,
            "frankenphp_jit_cpu_geomean": cpu_jit_geomean,
            "frankenphp_rss_geomean": rss_fp_geomean,
            "frankenphp_jit_rss_geomean": rss_jit_geomean,
            "cases": rows,
        }
        args.json.parent.mkdir(parents=True, exist_ok=True)
        args.json.write_text(json.dumps(record, indent=2) + "\n")
        print(f"wrote {args.json}")


if __name__ == "__main__":
    main()
