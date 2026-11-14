"""Shared process measurement, result metadata, and output formatting."""

from __future__ import annotations

import hashlib
import json
import os
import platform
import resource
import shutil
import statistics
import subprocess
import tempfile
import threading
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parent
SUITES = ("native", "magician", "hot-path")
SCHEMA_VERSION = 2
# PHP baselines run the way PHP is deployed for speed: OPcache with the tracing JIT.
# PHP 8.4+ defaults to opcache.jit=disable and 8.0-8.3 to a zero JIT buffer, so both
# are pinned here, overriding whatever the host php.ini says. OPcache also refuses to
# cache (and so to JIT) a script modified in the last two seconds by default, which
# would silently run a freshly copied fixture in the interpreter.
PHP_FLAGS = ("-d", "opcache.enable=1", "-d", "opcache.enable_cli=1",
             "-d", "opcache.jit=tracing", "-d", "opcache.jit_buffer_size=128M",
             "-d", "opcache.file_update_protection=0")
PHP_MODE = "CLI; OPcache and tracing JIT enabled"
JIT_PROBE = ("$s = function_exists('opcache_get_status') ? opcache_get_status(false) : false;"
             "echo !is_array($s) || empty($s['opcache_enabled']) ? 'OPcache off'"
             " : (empty($s['jit']['on']) ? 'JIT off' : 'on');")


def command_output(command: list[str]) -> str | None:
    """Read optional tool metadata without making missing baselines fatal."""
    if not shutil.which(command[0]):
        return None
    result = subprocess.run(command, cwd=REPO, capture_output=True, text=True, timeout=30)
    lines = (result.stdout or result.stderr).splitlines()
    return lines[0] if result.returncode == 0 and lines else None


def run_process(command: list[str], cwd: Path, timeout: float = 120,
                env: dict | None = None) -> str:
    """Run a command with an optional environment override and retain its diagnostics."""
    result = subprocess.run(command, cwd=cwd, capture_output=True, text=True,
                            timeout=timeout, env=env)
    if result.returncode:
        raise RuntimeError(f"command failed: {' '.join(command)}\n{result.stdout}\n{result.stderr}")
    return result.stdout


def peak_rss_kb(usage: resource.struct_rusage) -> int:
    """Normalize one child's peak resident set size to kibibytes."""
    value = usage.ru_maxrss
    return int(value if platform.system() != "Darwin" else value / 1024)


def run_process_measured(command: list[str], cwd: Path, timeout: float = 120,
                         env: dict | None = None) -> tuple[str, str, resource.struct_rusage]:
    """Run a child, returning stdout, stderr and its exact wait4 resource usage.

    `subprocess.run()` reaps the child before exposing rusage, so this helper starts the
    process directly, drains both pipes on reader threads and waits with `os.wait4`. The
    usage block is the measured process alone (no shell), which makes CPU and peak RSS
    comparable across Elephc and PHP variants.
    """
    process = subprocess.Popen(
        command, cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        text=True, bufsize=1,
    )
    stdout_chunks: list[str] = []
    stderr_chunks: list[str] = []

    def drain(stream, chunks: list[str]) -> None:
        try:
            chunks.append(stream.read())
        finally:
            stream.close()

    readers = [
        threading.Thread(target=drain, args=(process.stdout, stdout_chunks), daemon=True),
        threading.Thread(target=drain, args=(process.stderr, stderr_chunks), daemon=True),
    ]
    for reader in readers:
        reader.start()
    deadline = time.monotonic() + timeout
    timed_out = False
    while True:
        pid, status, usage = os.wait4(process.pid, os.WNOHANG)
        if pid == process.pid:
            break
        if time.monotonic() >= deadline:
            timed_out = True
            process.kill()
            pid, status, usage = os.wait4(process.pid, 0)
            break
        time.sleep(0.005)
    for reader in readers:
        reader.join(timeout=5)
    process.returncode = os.waitstatus_to_exitcode(status)
    stdout = "".join(stdout_chunks)
    stderr = "".join(stderr_chunks)
    if timed_out:
        raise subprocess.TimeoutExpired(command, timeout, output=stdout, stderr=stderr)
    if process.returncode:
        raise RuntimeError(f"command failed: {' '.join(command)}\n{stdout}\n{stderr}")
    return stdout, stderr, usage


def measure(command: list[str], cwd: Path, expected: str, iterations: int,
            warmup: int, timeout: float, env: dict | None = None) -> dict:
    """Measure execution only, checking output on both warmup and measured runs."""
    samples = []
    cpu_samples = []
    rss_samples = []
    for index in range(warmup + iterations):
        start = time.perf_counter()
        output, _stderr, usage = run_process_measured(command, cwd, timeout, env=env)
        elapsed = (time.perf_counter() - start) * 1000
        if output != expected:
            raise RuntimeError(f"unexpected stdout for {' '.join(command)}: "
                               f"expected {expected!r}, got {output!r}")
        if index >= warmup:
            samples.append(elapsed)
            cpu_samples.append((usage.ru_utime + usage.ru_stime) * 1000)
            rss_samples.append(peak_rss_kb(usage))
    return {"median_ms": statistics.median(samples), "samples_ms": samples,
            "median_cpu_ms": statistics.median(cpu_samples),
            "samples_cpu_ms": cpu_samples,
            "peak_rss_kb": statistics.median(rss_samples),
            "samples_rss_kb": rss_samples,
            "skipped": False, "detail": f"{iterations} runs"}


def skipped(detail: str) -> dict:
    return {"median_ms": None, "samples_ms": [],
            "median_cpu_ms": None, "samples_cpu_ms": [],
            "peak_rss_kb": None, "samples_rss_kb": [],
            "skipped": True, "detail": detail}


def php_command(php: str, *arguments: str, isolated: bool = False) -> list[str]:
    """Build a PHP baseline command with OPcache and the tracing JIT pinned on."""
    return [php, *(["-n"] if isolated else []), *PHP_FLAGS, *arguments]


def php_jit_problem(php: str, isolated: bool = False, timeout: float = 30) -> str | None:
    """Explain why the pinned JIT configuration is inactive, or return None when it runs."""
    result = subprocess.run(php_command(php, "-r", JIT_PROBE, isolated=isolated), cwd=REPO,
                            capture_output=True, text=True, timeout=timeout)
    if result.returncode == 0 and result.stdout == "on" and not result.stderr:
        return None
    detail = " ".join((result.stdout + result.stderr).split()) or f"exit {result.returncode}"
    return f"OPcache tracing JIT inactive for {php} ({detail})"


def frankenphp_jit_problem(frankenphp: str, ini: Path, timeout: float = 30) -> str | None:
    """Explain why the pinned FrankenPHP JIT configuration is inactive, or return None."""
    env = os.environ.copy()
    env["PHPRC"] = str(ini)
    result = subprocess.run(
        [frankenphp, "php-cli", "-r", JIT_PROBE], cwd=REPO, env=env,
        capture_output=True, text=True, timeout=timeout,
    )
    if result.returncode == 0 and result.stdout == "on" and not result.stderr:
        return None
    detail = " ".join((result.stdout + result.stderr).split()) or f"exit {result.returncode}"
    return f"FrankenPHP tracing JIT inactive ({detail})"


def require_baselines(cases: dict[str, list[dict]]) -> None:
    """Reject missing comparison tools before CI builds or records partial results."""
    kinds = {kind for fixtures in cases.values() for case in fixtures
             for _, kind, _ in case["variants"]}
    missing = []
    if "php" in kinds:
        php = shutil.which("php")
        problem = php_jit_problem(php) if php else "PHP interpreter (php)"
        if problem:
            missing.append(problem)
    if "c" in kinds and not any(shutil.which(tool) for tool in ("cc", "clang", "gcc")):
        missing.append("C compiler (cc, clang, or gcc)")
    if any(kind.startswith("frankenphp") for kind in kinds) and not (
            os.environ.get("FRANKENPHP_BIN") or shutil.which("frankenphp")):
        missing.append("FrankenPHP (frankenphp)")
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
            if kind in ("php", "php-jit"):
                php = shutil.which("php")
                problem = php_jit_problem(php) if php else "PHP interpreter unavailable"
                if problem:
                    # Never record an interpreter-only PHP time as the JIT baseline.
                    results[label] = skipped(problem)
                    continue
                command = php_command(php, source)
            elif kind == "c":
                cc = shutil.which("cc") or shutil.which("clang") or shutil.which("gcc")
                if not cc:
                    results[label] = skipped("C compiler unavailable")
                    continue
                binary = cwd / "baseline_c"
                run_process([cc, "-O2", source, "-o", str(binary)], cwd, timeout)
                command = [str(binary)]
            elif kind in ("frankenphp", "frankenphp-jit"):
                frankenphp = os.environ.get("FRANKENPHP_BIN") or shutil.which("frankenphp")
                if not frankenphp:
                    results[label] = skipped("FrankenPHP unavailable")
                    continue
                environment = os.environ.copy()
                ini_key = "FRANKENPHP_JIT_PHPRC" if kind == "frankenphp-jit" else "FRANKENPHP_PHPRC"
                ini = os.environ.get(ini_key)
                if ini:
                    environment["PHPRC"] = ini
                command = [frankenphp, "php-cli", source]
                results[label] = measure(command, cwd, expected, iterations, warmup,
                                        timeout, env=environment)
                continue
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
                            "php_mode": PHP_MODE,
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
    """Render every suite with wall, CPU and peak-RSS medians."""
    sections = []
    for suite, data in payload["suites"].items():
        labels = list(dict.fromkeys(label for case in data["cases"] for label in case["variants"]))
        sections.extend([f"## {suite}", "", "| case | " + " | ".join(f"{label} ms" for label in labels) + " |",
                         "|---|" + "---:|" * len(labels)])
        for case in data["cases"]:
            values = [format_ms(case["variants"].get(label, {}).get("median_ms")) for label in labels]
            sections.append("| " + " | ".join([case["case"], *values]) + " |")
        sections.append("")
        cpu_labels = [label for label in labels if any(
            case["variants"].get(label, {}).get("median_cpu_ms") is not None
            for case in data["cases"]
        )]
        if cpu_labels:
            sections.extend([f"### {suite} CPU", "",
                             "| case | " + " | ".join(f"{label} CPU ms" for label in cpu_labels) + " |",
                             "|---|" + "---:|" * len(cpu_labels)])
            for case in data["cases"]:
                values = [format_ms(case["variants"].get(label, {}).get("median_cpu_ms"))
                          for label in cpu_labels]
                sections.append("| " + " | ".join([case["case"], *values]) + " |")
            sections.append("")
        rss_labels = [label for label in labels if any(
            case["variants"].get(label, {}).get("peak_rss_kb") is not None
            for case in data["cases"]
        )]
        if rss_labels:
            sections.extend([f"### {suite} peak RSS", "",
                             "| case | " + " | ".join(f"{label} RSS MB" for label in rss_labels) + " |",
                             "|---|" + "---:|" * len(rss_labels)])
            for case in data["cases"]:
                values = []
                for label in rss_labels:
                    rss = case["variants"].get(label, {}).get("peak_rss_kb")
                    values.append("n/a" if rss is None else f"{rss / 1024:.2f}")
                sections.append("| " + " | ".join([case["case"], *values]) + " |")
            sections.append("")
    return "\n".join(sections) + "\n"
