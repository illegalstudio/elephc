#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import shutil
import statistics
import subprocess
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path


@dataclass
class CommandResult:
    label: str
    median_ms: float | None
    detail: str


@dataclass
class BenchmarkResult:
    case: str
    elephc: CommandResult
    php: CommandResult
    c: CommandResult


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run the elephc benchmark suite.")
    parser.add_argument("--iterations", type=int, default=5, help="Measured runs per benchmark.")
    parser.add_argument("--warmup", type=int, default=1, help="Warmup runs per benchmark.")
    parser.add_argument(
        "--case",
        action="append",
        default=[],
        help="Benchmark case name to run. Can be passed multiple times.",
    )
    parser.add_argument("--json", type=Path, help="Write machine-readable benchmark results to this path.")
    parser.add_argument("--markdown", type=Path, help="Write the markdown summary table to this path.")
    return parser.parse_args()


def repo_root() -> Path:
    return Path(__file__).resolve().parent.parent


def benchmark_root() -> Path:
    return repo_root() / "benchmarks" / "cases"


def elephc_bin() -> Path:
    binary = repo_root() / "target" / "release" / "elephc"
    if not binary.exists():
        subprocess.run(
            ["cargo", "build", "--release"],
            cwd=repo_root(),
            check=True,
        )
    return binary


def available_cases(selected: list[str]) -> list[Path]:
    cases = sorted(path for path in benchmark_root().iterdir() if path.is_dir())
    if not selected:
        return cases
    wanted = set(selected)
    selected_cases = [case for case in cases if case.name in wanted]
    missing = sorted(wanted - {case.name for case in selected_cases})
    if missing:
        raise SystemExit(f"unknown benchmark case(s): {', '.join(missing)}")
    return selected_cases


def run_checked(cmd: list[str], cwd: Path, expected: str) -> None:
    output = subprocess.run(cmd, cwd=cwd, text=True, capture_output=True, check=True)
    if output.stdout != expected:
        raise RuntimeError(
            f"unexpected stdout for {' '.join(cmd)}\nexpected: {expected!r}\nactual:   {output.stdout!r}"
        )


def measure_command(label: str, cmd: list[str], cwd: Path, expected: str, iterations: int, warmup: int) -> CommandResult:
    for _ in range(warmup):
        run_checked(cmd, cwd, expected)

    samples: list[float] = []
    for _ in range(iterations):
        started = time.perf_counter()
        run_checked(cmd, cwd, expected)
        samples.append((time.perf_counter() - started) * 1000.0)

    return CommandResult(label=label, median_ms=statistics.median(samples), detail=f"{iterations} runs")


def maybe_measure_php(case_dir: Path, cwd: Path, expected: str, iterations: int, warmup: int) -> CommandResult:
    php = shutil.which("php")
    if php is None:
        return CommandResult(label="php", median_ms=None, detail="php not found")
    return measure_command("php", [php, str(case_dir / "main.php")], cwd, expected, iterations, warmup)


def maybe_measure_c(case_dir: Path, cwd: Path, expected: str, iterations: int, warmup: int) -> CommandResult:
    cc = shutil.which("cc") or shutil.which("clang") or shutil.which("gcc")
    if cc is None:
        return CommandResult(label="c", median_ms=None, detail="C compiler not found")

    c_binary = cwd / "bench_c"
    subprocess.run([cc, "-O2", str(case_dir / "main.c"), "-o", str(c_binary)], cwd=cwd, check=True)
    return measure_command("c", [str(c_binary)], cwd, expected, iterations, warmup)


def measure_elephc(case_dir: Path, cwd: Path, expected: str, iterations: int, warmup: int) -> CommandResult:
    php_copy = cwd / "main.php"
    shutil.copy2(case_dir / "main.php", php_copy)
    subprocess.run(
        [str(elephc_bin()), str(php_copy)],
        cwd=cwd,
        text=True,
        capture_output=True,
        check=True,
    )
    return measure_command("elephc", [str(cwd / "main")], cwd, expected, iterations, warmup)


def format_ms(value: float | None) -> str:
    if value is None:
        return "n/a"
    return f"{value:.2f}"


def ratio(base: float | None, other: float | None) -> str:
    if base is None or other is None or other == 0:
        return "n/a"
    return f"{base / other:.2f}x"


def ratio_value(base: float | None, other: float | None) -> float | None:
    if base is None or other is None or other == 0:
        return None
    return base / other


def result_to_dict(result: CommandResult) -> dict[str, object]:
    return {
        "label": result.label,
        "median_ms": result.median_ms,
        "detail": result.detail,
        "skipped": result.median_ms is None,
    }


def benchmark_to_dict(result: BenchmarkResult) -> dict[str, object]:
    return {
        "case": result.case,
        "elephc": result_to_dict(result.elephc),
        "php": result_to_dict(result.php),
        "c": result_to_dict(result.c),
        "ratios": {
            "elephc_vs_php": ratio_value(result.elephc.median_ms, result.php.median_ms),
            "elephc_vs_c": ratio_value(result.elephc.median_ms, result.c.median_ms),
        },
    }


def render_markdown(results: list[BenchmarkResult]) -> str:
    lines = [
        "| case | elephc ms | php ms | c ms | vs php | vs c |",
        "|---|---:|---:|---:|---:|---:|",
    ]
    for result in results:
        lines.append(
            "| {case} | {elephc_ms} | {php_ms} | {c_ms} | {vs_php} | {vs_c} |".format(
                case=result.case,
                elephc_ms=format_ms(result.elephc.median_ms),
                php_ms=format_ms(result.php.median_ms),
                c_ms=format_ms(result.c.median_ms),
                vs_php=ratio(result.elephc.median_ms, result.php.median_ms),
                vs_c=ratio(result.elephc.median_ms, result.c.median_ms),
            )
        )
    return "\n".join(lines) + "\n"


def run_benchmarks(cases: list[Path], iterations: int, warmup: int) -> list[BenchmarkResult]:
    results = []
    for case_dir in cases:
        expected = (case_dir / "expected.txt").read_text()
        if not expected.endswith("\n"):
            expected += "\n"

        with tempfile.TemporaryDirectory(prefix=f"elephc-bench-{case_dir.name}-") as temp_dir:
            cwd = Path(temp_dir)
            elephc_result = measure_elephc(case_dir, cwd, expected, iterations, warmup)
            php_result = maybe_measure_php(case_dir, cwd, expected, iterations, warmup)
            c_result = maybe_measure_c(case_dir, cwd, expected, iterations, warmup)

        results.append(
            BenchmarkResult(
                case=case_dir.name,
                elephc=elephc_result,
                php=php_result,
                c=c_result,
            )
        )
    return results


def main() -> None:
    args = parse_args()
    if args.iterations < 1:
        raise SystemExit("--iterations must be at least 1")
    if args.warmup < 0:
        raise SystemExit("--warmup must be at least 0")

    cases = available_cases(args.case)
    if not cases:
        raise SystemExit("no benchmark cases selected")

    results = run_benchmarks(cases, args.iterations, args.warmup)
    markdown = render_markdown(results)
    print(markdown, end="")

    if args.markdown:
        args.markdown.write_text(markdown)

    if args.json:
        payload = {
            "iterations": args.iterations,
            "warmup": args.warmup,
            "cases": [benchmark_to_dict(result) for result in results],
        }
        args.json.write_text(json.dumps(payload, indent=2) + "\n")


if __name__ == "__main__":
    main()
