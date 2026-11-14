#!/usr/bin/env python3
"""Compare Elephc with FrankenPHP's embedded PHP on the native benchmark suite.

The comparison uses FrankenPHP's `php-cli` subcommand, which runs the same PHP
scripts as the regular `php` baseline without the Caddy HTTP server in the
measured path. Both FrankenPHP variants use a pinned `PHPRC`: the default
variant disables OPcache/JIT like the `php` baseline, and the `-jit` variant
enables the same tracing JIT settings as `php-jit`.

Wall-clock samples include process startup for every runtime, so tiny fused
microbenchmarks are dominated by startup while the larger native cases measure
the generated code. The JSON output retains every raw median and sample.
"""

from __future__ import annotations

import argparse
import json
import math
import os
import platform
import shutil
import subprocess
import tempfile
from datetime import datetime, timezone
from pathlib import Path

import cases
import common


VARIANTS = [
    ("elephc", "elephc", "main.php"),
    ("php", "php", "main.php"),
    ("php-jit", "php-jit", "main.php"),
    ("frankenphp", "frankenphp", "main.php"),
    ("frankenphp-jit", "frankenphp-jit", "main.php"),
    ("c", "c", "main.c"),
]
DISPLAY = ("elephc", "php", "php-jit", "frankenphp", "frankenphp-jit", "c")


def parse_args() -> argparse.Namespace:
    """Parse the comparison selectors and sampling budget."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", required=True, type=Path,
                        help="path to the Elephc release compiler")
    parser.add_argument("--frankenphp", type=Path,
                        help="path to the FrankenPHP binary (also FRANKENPHP_BIN)")
    parser.add_argument("--iterations", type=int, default=5,
                        help="measured runs per variant (default: 5)")
    parser.add_argument("--warmup", type=int, default=2,
                        help="warmup runs per variant (default: 2)")
    parser.add_argument("--timeout", type=float, default=300,
                        help="per-process timeout in seconds (default: 300)")
    parser.add_argument("--include", action="append", default=[],
                        metavar="CASE", help="native case to include (repeatable; default: all)")
    parser.add_argument("--exclude", action="append", default=[],
                        metavar="CASE", help="native case to skip (repeatable)")
    parser.add_argument("--json", type=Path,
                        help="write the full comparison record to this path")
    parser.add_argument("--require-ahead", action="store_true",
                        help="fail when Elephc is not ahead on wall time, CPU or peak RSS")
    parser.add_argument("--gate-tolerance", type=float, default=0.0,
                        help="allowed relative shortfall per metric under --require-ahead "
                             "(default: 0, a strict win)")
    args = parser.parse_args()
    if args.iterations < 1 or args.warmup < 0 or args.timeout <= 0:
        parser.error("iterations must be positive, warmup nonnegative, timeout positive")
    if not 0 <= args.gate_tolerance < 1:
        parser.error("gate tolerance must be in [0, 1)")
    return args


def first_line(command: list[str]) -> str:
    """Return the first line of a command's output, or a readable fallback."""
    result = subprocess.run(command, capture_output=True, text=True, timeout=30)
    lines = (result.stdout or result.stderr).splitlines()
    return lines[0].strip() if result.returncode == 0 and lines else "unknown"


def git_revision() -> str:
    """Return the current source revision for the comparison record."""
    try:
        return subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=common.REPO,
            capture_output=True,
            text=True,
            timeout=30,
            check=True,
        ).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        return "unknown"


def ratio(numerator: float | None, denominator: float | None) -> float | None:
    """Return a finite speed ratio, or None when a variant was skipped."""
    if numerator is None or denominator is None or numerator <= 0 or denominator <= 0:
        return None
    return numerator / denominator


def geometric_mean(values: list[float]) -> float | None:
    """Return the geometric mean used for cross-case speed summaries."""
    if not values:
        return None
    return math.exp(sum(math.log(value) for value in values) / len(values))


def format_ms(value: float | None) -> str:
    """Render one median for the Markdown table."""
    return "skip" if value is None else f"{value:.3f}"


def main() -> None:
    """Run every selected native fixture and print the comparison table."""
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
        case
        for case in discovered.get("native", [])
        if case["name"] not in excluded and (not included or case["name"] in included)
    ]
    if not selected:
        raise SystemExit("no native benchmark cases selected")

    with tempfile.TemporaryDirectory(prefix="elephc-frankenphp-bench-") as temporary:
        ini_dir = Path(temporary)
        nojit = ini_dir / "frankenphp.ini"
        jit = ini_dir / "frankenphp-jit.ini"
        nojit.write_text(
            "opcache.enable=0\nopcache.enable_cli=0\nopcache.jit=0\n"
            "opcache.file_update_protection=0\n"
        )
        jit.write_text(
            "opcache.enable=1\n"
            "opcache.enable_cli=1\n"
            "opcache.jit_buffer_size=64M\n"
            "opcache.jit=tracing\n"
            "opcache.file_update_protection=0\n"
        )
        problem = common.php_jit_problem(php) if (php := shutil.which("php")) else "PHP interpreter (php)"
        if problem:
            raise SystemExit(problem)
        problem = common.frankenphp_jit_problem(frankenphp, jit)
        if problem:
            raise SystemExit(problem)
        os.environ["FRANKENPHP_BIN"] = frankenphp
        os.environ["FRANKENPHP_PHPRC"] = str(nojit)
        os.environ["FRANKENPHP_JIT_PHPRC"] = str(jit)

        rows = []
        for case in selected:
            runnable = {**case, "variants": VARIANTS}
            record = common.run_case(
                runnable, compiler, args.iterations, args.warmup, args.timeout
            )
            rows.append(record)
            print(f"measured {case['name']}", flush=True)

    print()
    print("| case | " + " | ".join(DISPLAY) + " | elephc×vs FP | elephc×vs FP+JIT |")
    print("|---|" + "---:|" * (len(DISPLAY) + 2))
    fp_ratios = []
    fp_jit_ratios = []
    for row in rows:
        medians = {
            label: row["variants"].get(label, {}).get("median_ms")
            for label in DISPLAY
        }
        fp = ratio(medians.get("frankenphp"), medians.get("elephc"))
        fp_jit = ratio(medians.get("frankenphp-jit"), medians.get("elephc"))
        if fp is not None:
            fp_ratios.append(fp)
        if fp_jit is not None:
            fp_jit_ratios.append(fp_jit)
        fp_text = "n/a" if fp is None else f"{fp:.2f}x"
        fp_jit_text = "n/a" if fp_jit is None else f"{fp_jit:.2f}x"
        print(
            f"| {row['case']} | "
            + " | ".join(format_ms(medians[label]) for label in DISPLAY)
            + f" | {fp_text} | {fp_jit_text} |"
        )

    fp_geomean = geometric_mean(fp_ratios)
    fp_jit_geomean = geometric_mean(fp_jit_ratios)
    print()
    if fp_geomean is not None:
        print(f"Geometric mean Elephc speedup vs FrankenPHP: **{fp_geomean:.2f}x**")
        print(f"Elephc faster on {sum(value > 1 for value in fp_ratios)}/{len(fp_ratios)} cases")
    if fp_jit_geomean is not None:
        print(f"Geometric mean Elephc speedup vs FrankenPHP+JIT: **{fp_jit_geomean:.2f}x**")
        print(f"Elephc faster on {sum(value > 1 for value in fp_jit_ratios)}/{len(fp_jit_ratios)} cases")

    cpu_labels = ("elephc", "php", "php-jit", "frankenphp", "frankenphp-jit")
    rss_labels = cpu_labels
    print()
    print("| case | " + " | ".join(f"{label} CPU ms" for label in cpu_labels)
          + " | CPU×FP | CPU×FP+JIT |")
    print("|---|" + "---:|" * (len(cpu_labels) + 2))
    cpu_fp_ratios = []
    cpu_jit_ratios = []
    for row in rows:
        variants = row["variants"]
        cpu = {label: variants.get(label, {}).get("median_cpu_ms") for label in cpu_labels}
        fp = ratio(cpu.get("frankenphp"), cpu.get("elephc"))
        fp_jit = ratio(cpu.get("frankenphp-jit"), cpu.get("elephc"))
        if fp is not None:
            cpu_fp_ratios.append(fp)
        if fp_jit is not None:
            cpu_jit_ratios.append(fp_jit)
        print(
            f"| {row['case']} | "
            + " | ".join(format_ms(cpu[label]) for label in cpu_labels)
            + f" | {'n/a' if fp is None else f'{fp:.2f}x'}"
            + f" | {'n/a' if fp_jit is None else f'{fp_jit:.2f}x'} |"
        )
    print()
    print("| case | " + " | ".join(f"{label} RSS MB" for label in rss_labels)
          + " | RSS×FP | RSS×FP+JIT |")
    print("|---|" + "---:|" * (len(rss_labels) + 2))
    rss_fp_ratios = []
    rss_jit_ratios = []
    for row in rows:
        variants = row["variants"]
        rss = {
            label: (
                variants.get(label, {}).get("peak_rss_kb") / 1024
                if variants.get(label, {}).get("peak_rss_kb") is not None
                else None
            )
            for label in rss_labels
        }
        fp = ratio(rss.get("frankenphp"), rss.get("elephc"))
        fp_jit = ratio(rss.get("frankenphp-jit"), rss.get("elephc"))
        if fp is not None:
            rss_fp_ratios.append(fp)
        if fp_jit is not None:
            rss_jit_ratios.append(fp_jit)
        print(
            f"| {row['case']} | "
            + " | ".join(format_ms(rss[label]) for label in rss_labels)
            + f" | {'n/a' if fp is None else f'{fp:.2f}x'}"
            + f" | {'n/a' if fp_jit is None else f'{fp_jit:.2f}x'} |"
        )
    cpu_fp_geomean = geometric_mean(cpu_fp_ratios)
    cpu_jit_geomean = geometric_mean(cpu_jit_ratios)
    rss_fp_geomean = geometric_mean(rss_fp_ratios)
    rss_jit_geomean = geometric_mean(rss_jit_ratios)
    print()
    if cpu_fp_geomean is not None:
        print(f"Geometric mean CPU efficiency vs FrankenPHP: **{cpu_fp_geomean:.2f}x**")
    if cpu_jit_geomean is not None:
        print(f"Geometric mean CPU efficiency vs FrankenPHP+JIT: **{cpu_jit_geomean:.2f}x**")
    if rss_fp_geomean is not None:
        print(f"Geometric mean RAM efficiency vs FrankenPHP: **{rss_fp_geomean:.2f}x**")
    if rss_jit_geomean is not None:
        print(f"Geometric mean RAM efficiency vs FrankenPHP+JIT: **{rss_jit_geomean:.2f}x**")

    if args.require_ahead:
        regressions = []
        for row in rows:
            variants = row["variants"]
            elephc = variants.get("elephc", {})
            for label in ("frankenphp", "frankenphp-jit"):
                other = variants.get(label, {})
                for metric, key in (
                    ("wall", "median_ms"),
                    ("cpu", "median_cpu_ms"),
                    ("rss", "peak_rss_kb"),
                ):
                    ours = elephc.get(key)
                    theirs = other.get(key)
                    if not (
                        ours is not None
                        and theirs is not None
                        and ours > 0
                        and theirs > 0
                        and ours <= theirs * (1 + args.gate_tolerance)
                    ):
                        regressions.append(
                            f"{row['case']}: {metric} {ours} vs {label} {theirs}"
                        )
        if regressions:
            print("Efficiency gate failed:")
            for line in regressions:
                print(f"- {line}")
            raise SystemExit(1)
        print(f"Efficiency gate: Elephc ahead on {len(rows)}/{len(rows)} cases "
              "for wall time, CPU and peak RSS")

    record = {
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "platform": platform.platform(),
        "machine": platform.machine(),
        "compiler": str(compiler),
        "compiler_revision": git_revision(),
        "frankenphp": frankenphp,
        "frankenphp_version": first_line([frankenphp, "version"]),
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
    if args.json:
        args.json.parent.mkdir(parents=True, exist_ok=True)
        args.json.write_text(json.dumps(record, indent=2) + "\n")
        print(f"\nwrote {args.json}")


if __name__ == "__main__":
    main()
