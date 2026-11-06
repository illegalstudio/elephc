#!/usr/bin/env python3
"""Compare explicitly selected compilers and real PHP on a phased mixed workload."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import shutil
import statistics
import subprocess
import tempfile
import time
from pathlib import Path
from datetime import datetime, timezone

from common import atomic_json

FIXTURE = Path(__file__).resolve().parent / "diagnostics" / "mixed.php"
FIELDS = ("arithmetic", "closure", "string", "eval", "json", "body", "wall")


def assignments(values: list[str]) -> dict[str, str]:
    """Parse unique, nonempty LABEL=VALUE options without silently replacing a variant."""
    result = {}
    for value in values:
        label, separator, item = value.partition("=")
        if not separator or not label or not item or label in result:
            raise ValueError(f"expected unique LABEL=VALUE, got {value!r}")
        result[label] = item
    return result


def parse_sample(output: str, checksum: str, wall_ns: int) -> dict[str, float]:
    """Validate each execution's observable result and in-process phase clock readings."""
    parts = output.strip().split("|")
    if len(parts) != 3 or parts[0] != checksum:
        raise ValueError(f"unexpected workload result: {output!r}")
    body = int(parts[1])
    phases = [int(value) for value in parts[2].split(":")]
    if len(phases) != 5 or any(value < 0 for value in phases) or body != sum(phases):
        raise ValueError(f"invalid workload clock readings: {output!r}")
    if wall_ns < body:
        raise ValueError("process wall time is shorter than the timed workload")
    return dict(zip(FIELDS, (value / 1e6 for value in [*phases, body, wall_ns])))


def summarize(samples: dict[str, list[dict]], noise_limit: float) -> dict:
    """Retain dispersion for every phase and mark noisy comparisons unsuitable for claims."""
    result = {}
    for label, rows in samples.items():
        result[label] = {}
        for field in FIELDS:
            values = [row[field] for row in rows]
            median = statistics.median(values)
            mad = statistics.median(abs(value - median) for value in values)
            ratio = mad / median if median else (0.0 if not mad else float("inf"))
            result[label][field] = {"median_ms": median, "mad_ratio": ratio,
                                    "noise_pass": ratio <= noise_limit}
    return result


def execute(command: list[str], cwd: Path, timeout: float, env: dict | None = None) -> str:
    """Run one bounded command and preserve diagnostics instead of timing a failed process."""
    result = subprocess.run(command, cwd=cwd, env=env, capture_output=True,
                            text=True, timeout=timeout)
    if result.returncode or result.stderr:
        raise RuntimeError(f"command failed or wrote stderr: {command!r}\n"
                           f"exit={result.returncode}\n{result.stdout}\n{result.stderr}")
    return result.stdout


def file_hash(path: Path) -> str:
    """Fingerprint an exact input or executable independently of its mutable name."""
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def bridge_environment(environment: dict, directory: Path) -> dict:
    """Select one explicit archive directory without inheriting another build's bridge overrides."""
    selected = {key: value for key, value in environment.items()
                if not (key.startswith("ELEPHC_") and key.endswith("_LIB_DIR"))}
    for archive in directory.glob("libelephc_*.a"):
        name = archive.stem.removeprefix("libelephc_").upper()
        selected[f"ELEPHC_{name}_LIB_DIR"] = str(directory)
    return selected


def bridge_fingerprints(directories: dict[str, Path]) -> dict:
    """Record every supplied bridge, including non-Magician callbacks linked by opaque eval."""
    return {label: {path.name: file_hash(path) for path in sorted(directory.glob("libelephc_*.a"))}
            for label, directory in directories.items()}


def measure_commands(commands: dict[str, list[str]], cwd: Path, checksum: str,
                     rounds: int, warmups: int, timeout: float, require_idle: bool = False) -> dict:
    """Rotate variant order each round and check warmups exactly like recorded samples."""
    samples = {label: [] for label in commands}
    labels = list(commands)
    for index in range(-warmups, rounds):
        offset = index % len(labels)
        for label in labels[offset:] + labels[:offset]:
            if require_idle:
                assert_no_builds(cwd, timeout)
            started = time.perf_counter_ns()
            output = execute(commands[label], cwd, timeout)
            sample = parse_sample(output, checksum, time.perf_counter_ns() - started)
            if require_idle:
                assert_no_builds(cwd, timeout)
            if index >= 0:
                samples[label].append(sample)
    return samples


def assert_no_builds(cwd: Path, timeout: float) -> None:
    """Reject a sample if a known compiler or linker is visible before or after execution."""
    names = execute(["ps", "-A", "-o", "comm="], cwd, timeout).splitlines()
    builders = {"cargo", "rustc", "as", "cc1", "cc1plus", "ld", "ld.lld", "ld64", "collect2"}
    active = sorted({Path(name.strip()).name for name in names} & builders)
    if active:
        raise RuntimeError("concurrent compilation detected; reject this comparison: " + ", ".join(active))


def main() -> None:
    """Compile once per explicit compiler, then compare process and workload clocks separately."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", action="append", required=True, metavar="LABEL=PATH")
    parser.add_argument("--revision", action="append", default=[], metavar="LABEL=REVISION")
    parser.add_argument("--bridge-dir", action="append", default=[], metavar="LABEL=PATH")
    parser.add_argument("--php", default="php")
    parser.add_argument("--php-version", default="8.5", help="PHP semantic profile for all compilers")
    parser.add_argument("--iterations", type=int, default=21)
    parser.add_argument("--warmup", type=int, default=2)
    parser.add_argument("--size", type=int, default=20000)
    parser.add_argument("--rows", type=int, default=800)
    parser.add_argument("--columns", type=int, default=200)
    parser.add_argument("--eval-count", type=int, default=100)
    parser.add_argument("--heap-size", type=int, default=64 * 1024 * 1024)
    parser.add_argument("--ir-opt", choices=("on", "off"), default="on")
    parser.add_argument("--cpu", type=int, help="Pin every child to this CPU on Linux")
    parser.add_argument("--timeout", type=float, default=120)
    parser.add_argument("--noise-limit", type=float, default=.05)
    parser.add_argument("--require-idle", action="store_true", help="Reject visible concurrent compilers/linkers using ps")
    parser.add_argument("--json", type=Path, required=True)
    args = parser.parse_args()
    if min(args.iterations, args.size, args.rows, args.columns, args.heap_size) < 1 \
            or args.warmup < 0 or args.eval_count < 0 or args.timeout <= 0 \
            or not 0 < args.noise_limit < 1:
        parser.error("counts must be positive, warmup/eval-count nonnegative, and noise-limit in (0,1)")
    try:
        compilers = {label: Path(path).resolve() for label, path in assignments(args.compiler).items()}
        revisions = assignments(args.revision)
        bridges = assignments(args.bridge_dir)
        if "php" in compilers or (revisions.keys() | bridges.keys()) - compilers.keys():
            raise ValueError("php is reserved; revision and bridge labels must name a compiler")
        missing_revisions = compilers.keys() - revisions.keys()
        if missing_revisions:
            raise ValueError("every compiler requires --revision; missing: " +
                             ", ".join(sorted(missing_revisions)))
        if any(not path.is_file() for path in compilers.values()):
            raise ValueError("every selected compiler must be a file")
        bridges = {label: Path(path).resolve() for label, path in bridges.items()}
        if any(not (path / "libelephc_magician.a").is_file() for path in bridges.values()):
            raise ValueError("every bridge directory must contain libelephc_magician.a")
        php = shutil.which(args.php)
        if not php:
            raise ValueError(f"PHP interpreter unavailable: {args.php}")
        if args.require_idle and not shutil.which("ps"):
            raise ValueError("--require-idle requires ps")
        if args.cpu is not None and (not hasattr(os, "sched_getaffinity")
                                    or args.cpu not in os.sched_getaffinity(0)):
            raise ValueError("--cpu requires an available Linux CPU")
    except ValueError as error:
        parser.error(str(error))
    output = args.json.resolve()
    fingerprints = {label: file_hash(path) for label, path in compilers.items()}
    bridge_hashes = bridge_fingerprints(bridges)
    # One task-owned directory holds sources, caches and binaries, and is removed on failure too.
    with tempfile.TemporaryDirectory(prefix=".benchmark-compare-", dir=Path.cwd()) as temporary:
        cwd = Path(temporary)
        source = cwd / "mixed.php"
        source.write_bytes(FIXTURE.read_bytes())
        document = cwd / "nested.json"
        document.write_text(json.dumps([list(range(args.columns)) for _ in range(args.rows)],
                                       separators=(",", ":")))
        arguments = [str(args.size), "$sum = $sum + 3;", str(document), str(args.eval_count)]
        prefix = ["taskset", "-c", str(args.cpu)] if args.cpu is not None else []
        commands = {}
        binaries = {}
        for index, (label, compiler) in enumerate(compilers.items()):
            destination = cwd / f"variant-{index}"
            environment = os.environ.copy()
            if label in bridges:
                environment = bridge_environment(environment, bridges[label])
            # Keep caches isolated from concurrent source changes and other compiler revisions.
            environment["XDG_CACHE_HOME"] = str(cwd / f"cache-{index}")
            command = [str(compiler), "--quiet", f"--php-version={args.php_version}",
                       f"--heap-size={args.heap_size}", f"--ir-opt={args.ir_opt}", str(source)]
            # Linkers can emit benign diagnostics, so compilation has its own failure contract.
            compiled = subprocess.run(command, cwd=cwd, env=environment, capture_output=True,
                                      text=True, timeout=args.timeout)
            if compiled.returncode:
                raise RuntimeError(f"compilation failed for {label}:\n{compiled.stdout}\n{compiled.stderr}")
            source.with_suffix("").rename(destination)
            binaries[label] = file_hash(destination)
            commands[label] = [*prefix, str(destination), *arguments]
        commands["php"] = [*prefix, php, "-n", "-d", "opcache.enable_cli=0", "-d", "opcache.jit=0",
                           str(source), *arguments]
        expected = f"{args.size * (args.size-1)//2}:{args.size * (args.size+1)//2}:{args.size}:" \
                   f"{3 * args.eval_count}:{args.rows}"
        measured_at = datetime.now(timezone.utc).isoformat()
        samples = measure_commands(commands, cwd, expected, args.iterations, args.warmup, args.timeout, args.require_idle)
        if fingerprints != {label: file_hash(path) for label, path in compilers.items()} \
                or bridge_hashes != bridge_fingerprints(bridges):
            raise RuntimeError("a selected compiler or bridge changed during the comparison")
        stats = summarize(samples, args.noise_limit)
        payload = {"schema_version": 1, "measurement": "mixed-body-phases-and-process-wall",
                   "measurement_started_at": measured_at,
                   "measurement_finished_at": datetime.now(timezone.utc).isoformat(),
                   "concurrent_build_guard": args.require_idle,
                   "platform": platform.platform(), "architecture": platform.machine(),
                   "php_version": execute([php, "-n", "-v"], cwd, args.timeout).splitlines()[0],
                   "semantic_profile": args.php_version, "ir_opt": args.ir_opt,
                   "cpu": args.cpu, "iterations": args.iterations, "warmup": args.warmup,
                   "size": args.size, "rows": args.rows, "columns": args.columns,
                   "eval_count": args.eval_count, "heap_size": args.heap_size,
                   "fixture_sha256": file_hash(source), "document_sha256": file_hash(document),
                   "fixture_source": source.read_text(), "noise_limit": args.noise_limit,
                   "variants": {label: {"compiler": str(path), "revision": revisions[label],
                                        "compiler_sha256": fingerprints[label], "binary_sha256": binaries[label],
                                        "bridge_directory": str(bridges[label]) if label in bridges else None,
                                        "bridge_sha256": bridge_hashes.get(label)}
                                for label, path in compilers.items()},
                   "stats": stats, "samples": samples}
        atomic_json(output, payload)
        print("| Phase (ms) | " + " | ".join(stats) + " |")
        print("|---|" + "---:|" * len(stats))
        for field in FIELDS:
            print(f"| {field} | " + " | ".join(
                f"{fields[field]['median_ms']:.6f}" + (" (noisy)" if not fields[field]['noise_pass'] else "")
                for fields in stats.values()) + " |")
        if any(not field["noise_pass"] for fields in stats.values() for field in fields.values()):
            raise SystemExit("Samples preserved, but at least one phase exceeds the noise limit; rerun when quiet.")


if __name__ == "__main__":
    main()
