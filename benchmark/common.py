"""Shared process measurement, result metadata, and output formatting."""

from __future__ import annotations

import hashlib
import json
import os
import platform
import shutil
import statistics
import subprocess
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parent
SUITES = ("native", "magician", "hot-path")
SCHEMA_VERSION = 2


def command_output(command: list[str]) -> str | None:
    """Read optional tool metadata without making missing baselines fatal."""
    if not shutil.which(command[0]):
        return None
    result = subprocess.run(command, cwd=REPO, capture_output=True, text=True, timeout=30)
    lines = (result.stdout or result.stderr).splitlines()
    return lines[0] if result.returncode == 0 and lines else None


def run_process(command: list[str], cwd: Path, timeout: float = 120) -> str:
    """Run a command and retain its diagnostics when it fails."""
    result = subprocess.run(command, cwd=cwd, capture_output=True, text=True, timeout=timeout)
    if result.returncode:
        raise RuntimeError(f"command failed: {' '.join(command)}\n{result.stdout}\n{result.stderr}")
    return result.stdout


def measure(command: list[str], cwd: Path, expected: str, iterations: int,
            warmup: int, timeout: float) -> dict:
    """Measure execution only, checking output on both warmup and measured runs."""
    samples = []
    for index in range(warmup + iterations):
        start = time.perf_counter()
        output = run_process(command, cwd, timeout)
        elapsed = (time.perf_counter() - start) * 1000
        if output != expected:
            raise RuntimeError(f"unexpected stdout for {' '.join(command)}: "
                               f"expected {expected!r}, got {output!r}")
        if index >= warmup:
            samples.append(elapsed)
    return {"median_ms": statistics.median(samples), "samples_ms": samples,
            "skipped": False, "detail": f"{iterations} runs"}


def skipped(detail: str) -> dict:
    return {"median_ms": None, "samples_ms": [], "skipped": True, "detail": detail}


def require_baselines(cases: dict[str, list[dict]]) -> None:
    """Reject missing comparison tools before CI builds or records partial results."""
    kinds = {kind for fixtures in cases.values() for case in fixtures
             for _, kind, _ in case["variants"]}
    missing = []
    if "php" in kinds and not shutil.which("php"):
        missing.append("PHP interpreter (php)")
    if "c" in kinds and not any(shutil.which(tool) for tool in ("cc", "clang", "gcc")):
        missing.append("C compiler (cc, clang, or gcc)")
    if missing:
        raise ValueError("required benchmark baseline unavailable: " + ", ".join(missing))


def fixture_hash(directory: Path) -> str:
    """Fingerprint measured source, metadata, and expected output."""
    digest = hashlib.sha256()
    for path in sorted(directory.rglob("*")):
        if path.is_file() and path.suffix in {".php", ".c", ".txt", ".json"}:
            digest.update(path.relative_to(directory).as_posix().encode())
            digest.update(b"\0")
            digest.update(path.read_bytes())
    return digest.hexdigest()


def run_case(case: dict, compiler: Path, iterations: int, warmup: int,
             timeout: float) -> dict:
    """Compile each variant once, then measure it through the shared harness."""
    directory = case["directory"]
    expected = (directory / "expected.txt").read_text()
    if not expected.endswith("\n"):
        expected += "\n"
    results = {}
    with tempfile.TemporaryDirectory(prefix="elephc-benchmark-") as temporary:
        cwd = Path(temporary) / directory.name
        shutil.copytree(directory, cwd)
        for label, kind, source in case["variants"]:
            if kind == "php":
                php = shutil.which("php")
                if not php:
                    results[label] = skipped("PHP interpreter unavailable")
                    continue
                # Pin OPcache/JIT mode, regardless of the host's php.ini.
                command = [php, "-d", "opcache.enable_cli=0", "-d", "opcache.jit=0", source]
            elif kind == "c":
                cc = shutil.which("cc") or shutil.which("clang") or shutil.which("gcc")
                if not cc:
                    results[label] = skipped("C compiler unavailable")
                    continue
                binary = cwd / "baseline_c"
                run_process([cc, "-O2", source, "-o", str(binary)], cwd, timeout)
                command = [str(binary)]
            else:
                run_process([str(compiler), str(cwd / source)], cwd, timeout)
                command = [str((cwd / source).with_suffix(""))]
            results[label] = measure(command, cwd, expected, iterations, warmup, timeout)
    return {"case": case["name"], "fixture_sha256": fixture_hash(directory),
            "metadata": case.get("metadata", {}), "variants": results}


def result_metadata(iterations: int, warmup: int) -> dict:
    """Describe the measured revision and environment, not a publishing commit."""
    sha = os.environ.get("GITHUB_SHA") or command_output(["git", "rev-parse", "HEAD"])
    branch = os.environ.get("GITHUB_REF_NAME") or command_output(["git", "branch", "--show-current"])
    return {"schema_version": SCHEMA_VERSION,
            "timestamp": datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z"),
            "commit": sha, "branch": branch,
            "run_id": int(os.environ["GITHUB_RUN_ID"]) if os.environ.get("GITHUB_RUN_ID") else None,
            "run_attempt": int(os.environ["GITHUB_RUN_ATTEMPT"]) if os.environ.get("GITHUB_RUN_ATTEMPT") else None,
            "environment": {"runner": os.environ.get("BENCHMARK_RUNNER", "local"),
                            "os": platform.system(), "architecture": platform.machine(),
                            "rust": command_output(["rustc", "--version"]),
                            "php": command_output(["php", "--version"]),
                            "cc": command_output(["cc", "--version"]),
                            "php_mode": "CLI; OPcache/JIT disabled",
                            "measurement": "process-wall", "iterations": iterations, "warmup": warmup}}


def atomic_json(path: Path, data: dict) -> None:
    """Replace an output atomically without leaving a temporary file behind."""
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(mode="w", dir=path.parent, prefix=f".{path.name}.",
                                     delete=False) as output:
        temporary = Path(output.name)
        try:
            output.write(json.dumps(data, indent=2, allow_nan=False) + "\n")
            output.close()
            temporary.replace(path)
        finally:
            temporary.unlink(missing_ok=True)


def format_ms(value: float | None) -> str:
    return "n/a" if value is None else f"{value:.2f}"


def effective_ms(variant: dict) -> float | None:
    """Display an explicit historical reference without changing a raw measurement."""
    value = variant.get("median_ms")
    return value if value is not None else variant.get("reference", {}).get("median_ms")


def render_markdown(payload: dict) -> str:
    """Render every suite without assuming native-only variant names."""
    sections = []
    for suite, data in payload["suites"].items():
        labels = list(dict.fromkeys(label for case in data["cases"] for label in case["variants"]))
        sections.extend([f"## {suite}", "", "| case | " + " | ".join(f"{label} ms" for label in labels) + " |",
                         "|---|" + "---:|" * len(labels)])
        for case in data["cases"]:
            values = [format_ms(case["variants"].get(label, {}).get("median_ms")) for label in labels]
            sections.append("| " + " | ".join([case["case"], *values]) + " |")
        sections.append("")
    return "\n".join(sections) + "\n"
