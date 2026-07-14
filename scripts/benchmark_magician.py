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
from typing import Any


SOURCE_KINDS = {"literal", "dynamic"}


@dataclass
class EvalFragment:
    label: str
    source: str
    invocations: int
    source_kind: str
    parse_cache_should_hit: bool

    @property
    def size_bytes(self) -> int:
        return len(self.source.encode())


@dataclass
class CaseMetadata:
    name: str
    description: str
    fragments: list[EvalFragment]

    @property
    def eval_invocations(self) -> int:
        return sum(fragment.invocations for fragment in self.fragments)

    @property
    def max_fragment_size_bytes(self) -> int:
        if not self.fragments:
            return 0
        return max(fragment.size_bytes for fragment in self.fragments)

    @property
    def parse_cache_summary(self) -> str:
        if self.eval_invocations == 0:
            return "n/a"
        if any(fragment.parse_cache_should_hit for fragment in self.fragments):
            return "hit expected"
        return "no hit expected"

    @property
    def source_kind_summary(self) -> str:
        kinds = {fragment.source_kind for fragment in self.fragments if fragment.invocations > 0}
        if not kinds:
            return "n/a"
        if len(kinds) == 1:
            return next(iter(kinds))
        return "mixed"


@dataclass
class CommandResult:
    label: str
    median_ms: float | None
    samples_ms: list[float]
    detail: str


@dataclass
class BenchmarkResult:
    case: str
    metadata: CaseMetadata
    elephc_native: CommandResult
    elephc_eval: CommandResult
    php_native: CommandResult
    php_eval: CommandResult


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run the elephc magician/eval benchmark suite.")
    parser.add_argument("--iterations", type=int, default=5, help="Measured runs per benchmark variant.")
    parser.add_argument("--warmup", type=int, default=1, help="Warmup runs per benchmark variant.")
    parser.add_argument(
        "--case",
        action="append",
        default=[],
        help="Magician benchmark case name to run. Can be passed multiple times.",
    )
    parser.add_argument("--json", type=Path, help="Write machine-readable benchmark results to this path.")
    parser.add_argument("--markdown", type=Path, help="Write the markdown summary table to this path.")
    parser.add_argument("--list", action="store_true", help="List available magician benchmark cases and exit.")
    return parser.parse_args()


def repo_root() -> Path:
    return Path(__file__).resolve().parent.parent


def benchmark_root() -> Path:
    return repo_root() / "benchmarks" / "magician" / "cases"


def release_artifacts_ready() -> bool:
    target_dir = repo_root() / "target" / "release"
    return (target_dir / "elephc").exists() and (target_dir / "libelephc_magician.a").exists()


def elephc_bin() -> Path:
    if not release_artifacts_ready():
        subprocess.run(
            ["cargo", "build", "--release"],
            cwd=repo_root(),
            check=True,
        )
    return repo_root() / "target" / "release" / "elephc"


def load_metadata(case_dir: Path) -> CaseMetadata:
    data = json.loads((case_dir / "metadata.json").read_text())
    fragments: list[EvalFragment] = []
    for index, item in enumerate(data.get("eval_fragments", [])):
        label = str(item.get("label", f"fragment-{index}"))
        source = item.get("source")
        invocations = item.get("invocations")
        source_kind = item.get("source_kind")
        parse_cache_should_hit = item.get("parse_cache_should_hit")

        if not isinstance(source, str):
            raise ValueError(f"{case_dir.name}: eval fragment {label} must provide a string source")
        if not isinstance(invocations, int) or invocations < 0:
            raise ValueError(f"{case_dir.name}: eval fragment {label} must provide non-negative invocations")
        if source_kind not in SOURCE_KINDS:
            raise ValueError(
                f"{case_dir.name}: eval fragment {label} source_kind must be one of "
                f"{', '.join(sorted(SOURCE_KINDS))}"
            )
        if not isinstance(parse_cache_should_hit, bool):
            raise ValueError(f"{case_dir.name}: eval fragment {label} must provide parse_cache_should_hit")

        fragments.append(
            EvalFragment(
                label=label,
                source=source,
                invocations=invocations,
                source_kind=source_kind,
                parse_cache_should_hit=parse_cache_should_hit,
            )
        )

    if not fragments:
        raise ValueError(f"{case_dir.name}: metadata.json must define at least one eval fragment")

    return CaseMetadata(
        name=case_dir.name,
        description=str(data.get("description", "")),
        fragments=fragments,
    )


def available_cases(selected: list[str]) -> list[Path]:
    root = benchmark_root()
    cases = sorted(path for path in root.iterdir() if path.is_dir())
    if not selected:
        return cases
    wanted = set(selected)
    selected_cases = [case for case in cases if case.name in wanted]
    missing = sorted(wanted - {case.name for case in selected_cases})
    if missing:
        raise SystemExit(f"unknown magician benchmark case(s): {', '.join(missing)}")
    return selected_cases


def run_process(cmd: list[str], cwd: Path) -> subprocess.CompletedProcess[str]:
    try:
        return subprocess.run(cmd, cwd=cwd, text=True, capture_output=True, check=True)
    except subprocess.CalledProcessError as error:
        raise RuntimeError(
            "command failed: {cmd}\nstdout:\n{stdout}\nstderr:\n{stderr}".format(
                cmd=" ".join(cmd),
                stdout=error.stdout,
                stderr=error.stderr,
            )
        ) from error


def run_checked(cmd: list[str], cwd: Path, expected: str) -> None:
    output = run_process(cmd, cwd)
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

    return CommandResult(
        label=label,
        median_ms=statistics.median(samples),
        samples_ms=samples,
        detail=f"{iterations} runs",
    )


def skipped_result(label: str, detail: str) -> CommandResult:
    return CommandResult(label=label, median_ms=None, samples_ms=[], detail=detail)


def compile_elephc_variant(variant: str, cwd: Path) -> Path:
    source = cwd / f"{variant}.php"
    run_process([str(elephc_bin()), str(source)], cwd)
    return cwd / variant


def maybe_measure_php(variant: str, cwd: Path, expected: str, iterations: int, warmup: int) -> CommandResult:
    php = shutil.which("php")
    label = f"php-{variant}"
    if php is None:
        return skipped_result(label, "php not found")
    return measure_command(label, [php, str(cwd / f"{variant}.php")], cwd, expected, iterations, warmup)


def measure_case(case_dir: Path, iterations: int, warmup: int) -> BenchmarkResult:
    metadata = load_metadata(case_dir)
    expected = (case_dir / "expected.txt").read_text()
    if not expected.endswith("\n"):
        expected += "\n"

    with tempfile.TemporaryDirectory(prefix=f"elephc-magician-bench-{case_dir.name}-") as temp_dir:
        cwd = Path(temp_dir) / case_dir.name
        shutil.copytree(case_dir, cwd)

        native_binary = compile_elephc_variant("native", cwd)
        eval_binary = compile_elephc_variant("eval", cwd)

        elephc_native = measure_command(
            "elephc-native",
            [str(native_binary)],
            cwd,
            expected,
            iterations,
            warmup,
        )
        elephc_eval = measure_command(
            "elephc-eval",
            [str(eval_binary)],
            cwd,
            expected,
            iterations,
            warmup,
        )
        php_native = maybe_measure_php("native", cwd, expected, iterations, warmup)
        php_eval = maybe_measure_php("eval", cwd, expected, iterations, warmup)

    return BenchmarkResult(
        case=case_dir.name,
        metadata=metadata,
        elephc_native=elephc_native,
        elephc_eval=elephc_eval,
        php_native=php_native,
        php_eval=php_eval,
    )


def format_ms(value: float | None) -> str:
    if value is None:
        return "n/a"
    return f"{value:.2f}"


def ratio(numerator: float | None, denominator: float | None) -> str:
    value = ratio_value(numerator, denominator)
    if value is None:
        return "n/a"
    return f"{value:.2f}x"


def ratio_value(numerator: float | None, denominator: float | None) -> float | None:
    if numerator is None or denominator is None or denominator == 0:
        return None
    return numerator / denominator


def fragment_size_summary(metadata: CaseMetadata) -> str:
    sizes = [fragment.size_bytes for fragment in metadata.fragments]
    if len(sizes) == 1:
        return str(sizes[0])
    return f"{metadata.max_fragment_size_bytes} max"


def result_to_dict(result: CommandResult) -> dict[str, object]:
    return {
        "label": result.label,
        "median_ms": result.median_ms,
        "samples_ms": result.samples_ms,
        "detail": result.detail,
        "skipped": result.median_ms is None,
    }


def metadata_to_dict(metadata: CaseMetadata) -> dict[str, object]:
    return {
        "description": metadata.description,
        "eval_invocations": metadata.eval_invocations,
        "literal_or_dynamic": metadata.source_kind_summary,
        "parse_cache": metadata.parse_cache_summary,
        "max_fragment_size_bytes": metadata.max_fragment_size_bytes,
        "eval_fragments": [
            {
                "label": fragment.label,
                "invocations": fragment.invocations,
                "source_kind": fragment.source_kind,
                "parse_cache_should_hit": fragment.parse_cache_should_hit,
                "size_bytes": fragment.size_bytes,
            }
            for fragment in metadata.fragments
        ],
    }


def benchmark_to_dict(result: BenchmarkResult) -> dict[str, object]:
    return {
        "case": result.case,
        "metadata": metadata_to_dict(result.metadata),
        "elephc_native": result_to_dict(result.elephc_native),
        "elephc_eval": result_to_dict(result.elephc_eval),
        "php_native": result_to_dict(result.php_native),
        "php_eval": result_to_dict(result.php_eval),
        "ratios": {
            "elephc_eval_vs_native": ratio_value(result.elephc_eval.median_ms, result.elephc_native.median_ms),
            "php_eval_vs_native": ratio_value(result.php_eval.median_ms, result.php_native.median_ms),
            "elephc_eval_vs_php_eval": ratio_value(result.elephc_eval.median_ms, result.php_eval.median_ms),
        },
    }


def render_markdown(results: list[BenchmarkResult]) -> str:
    lines = [
        "| case | evals | fragment bytes | kind | cache | elephc native ms | elephc eval ms | eval/native | php native ms | php eval ms | php eval/native |",
        "|---|---:|---:|---|---|---:|---:|---:|---:|---:|---:|",
    ]
    for result in results:
        lines.append(
            "| {case} | {evals} | {fragment_bytes} | {kind} | {cache} | {elephc_native} | {elephc_eval} | {elephc_ratio} | {php_native} | {php_eval} | {php_ratio} |".format(
                case=result.case,
                evals=result.metadata.eval_invocations,
                fragment_bytes=fragment_size_summary(result.metadata),
                kind=result.metadata.source_kind_summary,
                cache=result.metadata.parse_cache_summary,
                elephc_native=format_ms(result.elephc_native.median_ms),
                elephc_eval=format_ms(result.elephc_eval.median_ms),
                elephc_ratio=ratio(result.elephc_eval.median_ms, result.elephc_native.median_ms),
                php_native=format_ms(result.php_native.median_ms),
                php_eval=format_ms(result.php_eval.median_ms),
                php_ratio=ratio(result.php_eval.median_ms, result.php_native.median_ms),
            )
        )
    return "\n".join(lines) + "\n"


def run_benchmarks(cases: list[Path], iterations: int, warmup: int) -> list[BenchmarkResult]:
    return [measure_case(case_dir, iterations, warmup) for case_dir in cases]


def list_cases(cases: list[Path]) -> None:
    for case_dir in cases:
        metadata = load_metadata(case_dir)
        print(f"{case_dir.name}: {metadata.description}")


def main() -> None:
    args = parse_args()
    if args.iterations < 1:
        raise SystemExit("--iterations must be at least 1")
    if args.warmup < 0:
        raise SystemExit("--warmup must be at least 0")

    cases = available_cases(args.case)
    if not cases:
        raise SystemExit("no magician benchmark cases selected")

    if args.list:
        list_cases(cases)
        return

    results = run_benchmarks(cases, args.iterations, args.warmup)
    markdown = render_markdown(results)
    print(markdown, end="")

    if args.markdown:
        args.markdown.write_text(markdown)

    if args.json:
        payload: dict[str, Any] = {
            "iterations": args.iterations,
            "warmup": args.warmup,
            "cases": [benchmark_to_dict(result) for result in results],
        }
        args.json.write_text(json.dumps(payload, indent=2) + "\n")


if __name__ == "__main__":
    main()
