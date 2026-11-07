#!/usr/bin/env python3
"""HTTP throughput benchmark for an elephc `--web` binary.

Compiles one PHP fixture with `--web`, starts the prefork server, drives it with
`wrk` (and optionally k6), and reports per-round samples plus a median. The
default fixture is the minimal handler used by the FrankenPHP-vs-PHP-FPM article:
one echo and one `$_SERVER` read.

This is deliberately separate from `benchmark/run.py`: that runner times
short-lived processes, while this one owns a long-running server and an external
load generator. Results are not appended to the process-execution history.

Examples:
    python3 benchmark/web/run.py
    python3 benchmark/web/run.py --workers 4 --threads 4 --connections 100 --duration 10s
    python3 benchmark/web/run.py --compiler target/release/elephc --no-build \
        --json web-results.json --markdown web-results.md
    python3 benchmark/web/run.py --k6 vus --k6 constant
    python3 benchmark/web/run.py --php-baseline
"""

from __future__ import annotations

import argparse
import ctypes
import json
import os
import platform
import shutil
import socket
import statistics
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_FIXTURE = Path(__file__).resolve().parent / "hello.php"
# Resolve the platform string before `libproc` is loaded through ctypes. On
# macOS, importing `plistlib` after `libproc` crashes the system Python 3.9.
HOST_PLATFORM = platform.platform()
K6_SCRIPTS = {
    "vus": Path(__file__).resolve().parent / "k6-vus.js",
    "constant": Path(__file__).resolve().parent / "k6-constant.js",
}


def free_port() -> int:
    """Reserves and releases an ephemeral localhost port."""
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def process_tree(master: int) -> list[int]:
    """Returns the master PID and every descendant currently visible to `ps`."""
    completed = subprocess.run(
        ["ps", "-axo", "pid=,ppid="],
        capture_output=True,
        text=True,
        check=False,
    )
    children: dict[int, list[int]] = {}
    for line in completed.stdout.splitlines():
        parts = line.split()
        if len(parts) != 2:
            continue
        try:
            pid, ppid = int(parts[0]), int(parts[1])
        except ValueError:
            continue
        children.setdefault(ppid, []).append(pid)
    tree = [master]
    pending = [master]
    while pending:
        parent = pending.pop()
        for child in children.get(parent, []):
            tree.append(child)
            pending.append(child)
    return tree


class _MachTimebase(ctypes.Structure):
    """Mirrors `struct mach_timebase_info` from `<mach/mach_time.h>`."""

    _fields_ = [("numer", ctypes.c_uint32), ("denom", ctypes.c_uint32)]


_MACH_TIMEBASE: tuple[int, int] | None = None


def _mach_timebase() -> tuple[int, int]:
    """Returns the `mach_absolute_time` numerator/denominator, cached."""
    global _MACH_TIMEBASE
    if _MACH_TIMEBASE is None:
        try:
            libsystem = ctypes.CDLL("/usr/lib/libSystem.B.dylib")
            timebase = _MachTimebase()
            if libsystem.mach_timebase_info(ctypes.byref(timebase)) != 0 or not timebase.denom:
                _MACH_TIMEBASE = (1, 1)
            else:
                _MACH_TIMEBASE = (int(timebase.numer), int(timebase.denom))
        except OSError:
            _MACH_TIMEBASE = (1, 1)
    return _MACH_TIMEBASE


_LIBPROC: ctypes.CDLL | None = None


def _libproc() -> ctypes.CDLL | None:
    """Loads `libproc` once and installs the `proc_pid_rusage` signature."""
    global _LIBPROC
    if _LIBPROC is None:
        try:
            _LIBPROC = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
        except OSError:
            return None
        _LIBPROC.proc_pid_rusage.argtypes = [
            ctypes.c_int,
            ctypes.c_int,
            ctypes.c_void_p,
        ]
        _LIBPROC.proc_pid_rusage.restype = ctypes.c_int
    return _LIBPROC


def _darwin_process_cpu_split(pid: int) -> tuple[float, float]:
    """Reads one process's user and system CPU seconds from `proc_pid_rusage`."""
    libproc = _libproc()
    if libproc is None:
        return 0.0, 0.0
    buffer = ctypes.create_string_buffer(256)
    # RUSAGE_INFO_V4: ri_user_time at byte 16, ri_system_time at byte 24.
    # Both are in mach absolute time units, not nanoseconds.
    if libproc.proc_pid_rusage(pid, 4, buffer) != 0:
        return 0.0, 0.0
    user = ctypes.c_uint64.from_buffer(buffer, 16).value
    system = ctypes.c_uint64.from_buffer(buffer, 24).value
    numer, denom = _mach_timebase()
    scale = numer / denom / 1_000_000_000.0
    return user * scale, system * scale


def process_cpu_split(pid: int) -> tuple[float, float]:
    """Returns one process's `(user, system)` CPU seconds, best effort."""
    if sys.platform == "darwin":
        return _darwin_process_cpu_split(pid)
    try:
        stat = Path(f"/proc/{pid}/stat").read_text()
        fields = stat[stat.rfind(")") + 2 :].split()
        ticks = os.sysconf("SC_CLK_TCK")
        return int(fields[11]) / ticks, int(fields[12]) / ticks
    except (OSError, IndexError, ValueError):
        return 0.0, 0.0


def process_cpu_seconds(pid: int) -> float:
    """Returns one process's user+system CPU time in seconds, best effort."""
    user, system = process_cpu_split(pid)
    return user + system


def server_cpu_split(master: int) -> tuple[float, float]:
    """Sums `(user, system)` CPU seconds over the server master and workers."""
    user = 0.0
    system = 0.0
    for pid in process_tree(master):
        part_user, part_system = process_cpu_split(pid)
        user += part_user
        system += part_system
    return user, system


def server_cpu_seconds(master: int) -> float:
    """Sums the user+system CPU time of the master and its worker descendants."""
    user, system = server_cpu_split(master)
    return user + system


def wait_for_server(process: subprocess.Popen[str], url: str, log_path: Path) -> None:
    """Polls `url` until the server answers, failing fast if it exits."""
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        if process.poll() is not None:
            log = log_path.read_text(errors="replace") if log_path.exists() else ""
            raise SystemExit(f"server exited with {process.returncode}:\n{log}")
        try:
            with urllib.request.urlopen(url, timeout=0.5) as response:
                response.read()
                return
        except (urllib.error.URLError, ConnectionError, TimeoutError, OSError):
            time.sleep(0.05)
    log = log_path.read_text(errors="replace") if log_path.exists() else ""
    raise SystemExit(f"server did not answer within 15s:\n{log}")


def parse_ms(text: str) -> float:
    """Converts a wrk duration (`us`, `ms`, or `s`) to milliseconds."""
    value = text.strip()
    for suffix, scale in (("us", 1e-3), ("ms", 1.0), ("s", 1000.0)):
        if value.endswith(suffix):
            return float(value[: -len(suffix)]) * scale
    return float(value)


def parse_wrk(text: str) -> dict[str, Any]:
    """Extracts throughput, latency, and socket errors from one wrk report."""
    result: dict[str, Any] = {"raw": text}
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("Latency") and len(stripped.split()) >= 3:
            fields = stripped.split()
            result["latency_avg_ms"] = parse_ms(fields[1])
            result["latency_stdev_ms"] = parse_ms(fields[2])
            result["latency_max_ms"] = parse_ms(fields[3])
        elif stripped.startswith("Req/Sec") and len(stripped.split()) >= 3:
            result["req_per_sec_avg"] = stripped.split()[1]
        elif stripped.startswith("50%") and len(stripped.split()) == 2:
            result["latency_p50_ms"] = parse_ms(stripped.split()[1])
        elif stripped.startswith("75%") and len(stripped.split()) == 2:
            result["latency_p75_ms"] = parse_ms(stripped.split()[1])
        elif stripped.startswith("90%") and len(stripped.split()) == 2:
            result["latency_p90_ms"] = parse_ms(stripped.split()[1])
        elif stripped.startswith("99%") and len(stripped.split()) == 2:
            result["latency_p99_ms"] = parse_ms(stripped.split()[1])
        elif stripped.startswith("Requests/sec:"):
            result["requests_per_sec"] = float(stripped.split(":")[1])
        elif stripped.startswith("Transfer/sec:"):
            result["transfer_per_sec"] = stripped.split(":", 1)[1].strip()
        elif stripped.startswith("Socket errors:"):
            result["socket_errors"] = stripped.split(":", 1)[1].strip()
        elif " requests in " in stripped and " read" in stripped:
            result["summary"] = stripped
            result["requests"] = float(stripped.split()[0])
    if "requests_per_sec" not in result:
        raise SystemExit(f"could not parse wrk output:\n{text}")
    return result


def aggregate_wrk_results(results: list[dict[str, Any]]) -> dict[str, Any]:
    """Merges one or more concurrent wrk reports into one aggregate sample.

    Aggregate throughput is the sum. Latency is not additive, so the report
    keeps the mean average latency and the worst p99 across clients; the
    per-client reports remain available under `per_client`.
    """
    if len(results) == 1:
        merged = dict(results[0])
        merged["clients"] = 1
        return merged
    average_latencies = [
        float(result["latency_avg_ms"])
        for result in results
        if "latency_avg_ms" in result
    ]
    p99_latencies = [
        float(result["latency_p99_ms"])
        for result in results
        if "latency_p99_ms" in result
    ]
    return {
        "clients": len(results),
        "requests": sum(float(result.get("requests", 0)) for result in results),
        "requests_per_sec": sum(float(result["requests_per_sec"]) for result in results),
        "latency_avg_ms": statistics.mean(average_latencies) if average_latencies else 0.0,
        "latency_p99_ms": max(p99_latencies) if p99_latencies else None,
        "per_client": results,
        "raw": "\n".join(result.get("raw", "") for result in results),
    }


def run_wrk(
    url: str,
    threads: int,
    connections: int,
    duration: str,
    timeout: float,
    clients: int = 1,
) -> dict[str, Any]:
    """Runs one or more concurrent wrk measurements and returns their aggregate."""
    commands = [
        [
            "wrk",
            "--latency",
            "-t",
            str(threads),
            "-c",
            str(connections),
            "-d",
            duration,
            url,
        ]
        for _ in range(clients)
    ]
    processes = [
        subprocess.Popen(
            command,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        for command in commands
    ]
    results: list[dict[str, Any]] = []
    for command, process in zip(commands, processes):
        stdout, stderr = process.communicate(timeout=timeout)
        if process.returncode != 0:
            raise SystemExit(
                f"wrk exited with {process.returncode}:\n{stderr or stdout}"
            )
        parsed = parse_wrk(stdout)
        parsed["command"] = " ".join(command)
        results.append(parsed)
    return aggregate_wrk_results(results)


def run_k6(
    scenario: str,
    url: str,
    duration: str,
    work_dir: Path,
    timeout: float,
) -> dict[str, Any]:
    """Runs one k6 scenario and returns its summary metrics."""
    script = K6_SCRIPTS[scenario]
    summary_path = work_dir / f"k6-{scenario}.json"
    command = [
        "k6",
        "run",
        "--quiet",
        "--summary-export",
        str(summary_path),
        "--env",
        f"TARGET={url}",
        "--env",
        f"DURATION={duration}",
        str(script),
    ]
    completed = subprocess.run(
        command,
        capture_output=True,
        text=True,
        timeout=timeout,
    )
    if completed.returncode != 0:
        raise SystemExit(
            f"k6 exited with {completed.returncode}:\n{completed.stderr or completed.stdout}"
        )
    summary = json.loads(summary_path.read_text())
    metrics = summary.get("metrics", {})
    return {
        "scenario": scenario,
        "command": " ".join(command),
        "metrics": metrics,
        # k6's summary-export puts the value at the metric's top level; older
        # exports nest it under "values". Normalize both shapes for the report.
        "http_reqs_per_sec": metrics.get("http_reqs", {}).get(
            "rate", metrics.get("http_reqs", {}).get("values", {}).get("rate", 0)
        ),
        "http_req_avg_ms": metrics.get("http_req_duration", {}).get(
            "avg", metrics.get("http_req_duration", {}).get("values", {}).get("avg", 0)
        ),
        "http_req_p95_ms": metrics.get("http_req_duration", {}).get(
            "p(95)", metrics.get("http_req_duration", {}).get("values", {}).get("p(95)", 0)
        ),
        "http_req_failed_rate": metrics.get("http_req_failed", {}).get(
            "value", metrics.get("http_req_failed", {}).get("values", {}).get("rate", 0)
        ),
        "raw": completed.stderr or completed.stdout,
    }


def compile_fixture(
    compiler: Path,
    fixture: Path,
    work_dir: Path,
    timeout: float,
) -> Path:
    """Copies a fixture into the work dir and compiles it as a `--web` binary."""
    source = work_dir / "app.php"
    shutil.copyfile(fixture, source)
    completed = subprocess.run(
        [str(compiler), "--web", "--quiet", source.name],
        cwd=work_dir,
        capture_output=True,
        text=True,
        timeout=timeout,
    )
    binary = work_dir / "app"
    if completed.returncode != 0 or not binary.exists():
        raise SystemExit(
            f"compilation failed ({completed.returncode}):\n"
            f"{completed.stdout}{completed.stderr}"
        )
    return binary


def server_environment(compiler: Path) -> dict[str, Any]:
    """Records the host and tool versions that affect the numbers."""
    version = subprocess.run(
        [str(compiler), "--version"],
        capture_output=True,
        text=True,
        check=False,
    ).stdout.strip()
    wrk = shutil.which("wrk")
    k6 = shutil.which("k6")
    return {
        "platform": HOST_PLATFORM,
        "machine": platform.machine(),
        "processor": platform.processor(),
        "cpu_count": os.cpu_count(),
        "python": platform.python_version(),
        "compiler": str(compiler),
        "compiler_version": version,
        "wrk": wrk or "missing",
        "k6": k6 or "missing",
        "load_average": list(os.getloadavg()),
    }


def median(values: list[float]) -> float | None:
    """Returns the median of a non-empty list, or None when no samples exist."""
    return statistics.median(values) if values else None


def summarize(rounds: list[dict[str, Any]]) -> dict[str, Any]:
    """Reduces repeated wrk rounds to median throughput and latency."""
    rps = [float(round_["requests_per_sec"]) for round_ in rounds]
    avg = [float(round_["latency_avg_ms"]) for round_ in rounds if "latency_avg_ms" in round_]
    p99 = [float(round_["latency_p99_ms"]) for round_ in rounds if "latency_p99_ms" in round_]
    cpu = [
        float(round_["server_cpu_us_per_request"])
        for round_ in rounds
        if "server_cpu_us_per_request" in round_
    ]
    user_cpu = [
        float(round_["server_user_us_per_request"])
        for round_ in rounds
        if "server_user_us_per_request" in round_
    ]
    system_cpu = [
        float(round_["server_system_us_per_request"])
        for round_ in rounds
        if "server_system_us_per_request" in round_
    ]
    return {
        "rounds": len(rounds),
        "requests_per_sec_median": median(rps),
        "requests_per_sec_min": min(rps) if rps else None,
        "requests_per_sec_max": max(rps) if rps else None,
        "latency_avg_ms_median": median(avg),
        "latency_p99_ms_median": median(p99),
        "server_cpu_us_per_request_median": median(cpu),
        "server_user_us_per_request_median": median(user_cpu),
        "server_system_us_per_request_median": median(system_cpu),
    }


def markdown_report(results: dict[str, Any]) -> str:
    """Renders a compact Markdown report for one run."""
    lines = [
        f"# elephc web benchmark: {results['fixture']}",
        "",
        f"- Server: `elephc --web`, workers={results['workers']}",
        f"- Load: `wrk -t{results['threads']} -c{results['connections']} "
        f"-d{results['duration']}` x{results.get('clients', 1)} client(s)",
        f"- Platform: {results['environment']['platform']} "
        f"({results['environment']['cpu_count']} CPUs)",
        "",
        "| Repeat | Requests/sec | Avg latency | p99 latency |",
        "|---:|---:|---:|---:|",
    ]
    for index, round_ in enumerate(results["wrk"]["raw_rounds"], start=1):
        p99 = round_.get("latency_p99_ms")
        p99_text = f"{p99:.2f} ms" if p99 is not None else "n/a"
        lines.append(
            f"| {index} | {round_['requests_per_sec']:.0f} | "
            f"{round_.get('latency_avg_ms', 0):.2f} ms | {p99_text} |"
        )
    summary = results["wrk"]["summary"]
    p99_median = summary.get("latency_p99_ms_median")
    p99_text = f"{p99_median:.2f} ms" if p99_median is not None else "n/a"
    cpu_median = summary.get("server_cpu_us_per_request_median")
    lines.extend(
        [
            "",
            f"Median: **{summary['requests_per_sec_median']:.0f} req/s**, "
            f"average latency **{summary['latency_avg_ms_median']:.2f} ms**, "
            f"p99 **{p99_text}**.",
            "",
        ]
    )
    if cpu_median is not None:
        user_median = summary.get("server_user_us_per_request_median") or 0.0
        system_median = summary.get("server_system_us_per_request_median") or 0.0
        lines.extend(
            [
                f"Server CPU: **{cpu_median:.2f} µs/request** "
                f"(user {user_median:.2f}, sys {system_median:.2f}; median).",
                "",
            ]
        )
    if results.get("k6"):
        lines.extend(["## k6", "", "| Scenario | HTTP req/s | avg | p95 | failed |", "|---|---:|---:|---:|---:|"])
        for scenario in results["k6"]:
            lines.append(
                f"| {scenario['scenario']} | {scenario['http_reqs_per_sec']:.1f} | "
                f"{scenario['http_req_avg_ms']:.2f} ms | "
                f"{scenario['http_req_p95_ms']:.2f} ms | "
                f"{scenario['http_req_failed_rate'] * 100:.2f}% |"
            )
        lines.append("")
    if results.get("php"):
        php = results["php"]
        lines.extend(
            [
                "## PHP built-in server baseline",
                "",
                f"`php -S` median: **{php['summary']['requests_per_sec_median']:.0f} req/s** "
                f"(single process, not PHP-FPM).",
                "",
            ]
        )
    return "\n".join(lines)


def run_php_baseline(
    fixture: Path,
    work_dir: Path,
    threads: int,
    connections: int,
    duration: str,
    timeout: float,
    clients: int,
) -> dict[str, Any]:
    """Measures the fixture under PHP's single-process built-in server."""
    php = shutil.which("php")
    if php is None:
        raise SystemExit("--php-baseline requires `php` on PATH")
    document_root = work_dir / "php-root"
    document_root.mkdir(exist_ok=True)
    shutil.copyfile(fixture, document_root / "index.php")
    port = free_port()
    url = f"http://127.0.0.1:{port}/"
    log_path = work_dir / "php-server.log"
    with log_path.open("w") as log:
        process = subprocess.Popen(
            [php, "-S", f"127.0.0.1:{port}", "-t", str(document_root)],
            stdout=log,
            stderr=subprocess.STDOUT,
            text=True,
        )
        try:
            wait_for_server(process, url, log_path)
            round_ = run_wrk(url, threads, connections, duration, timeout, clients)
        finally:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
    return {"summary": summarize([round_]), "raw_rounds": [round_]}


def build_compiler(no_build: bool) -> Path:
    """Builds the release compiler unless the caller supplied one."""
    compiler = ROOT / "target" / "release" / "elephc"
    if no_build:
        if not compiler.exists():
            raise SystemExit(f"--no-build requested but {compiler} does not exist")
        return compiler
    completed = subprocess.run(
        ["cargo", "build", "--release"],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    if completed.returncode != 0:
        raise SystemExit(f"cargo build --release failed:\n{completed.stderr[-4000:]}")
    return compiler


def main() -> int:
    """Parses arguments, runs the server, and writes the reports."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixture", type=Path, default=DEFAULT_FIXTURE)
    parser.add_argument("--compiler", type=Path)
    parser.add_argument("--no-build", action="store_true")
    parser.add_argument("--workers", type=int, default=4)
    parser.add_argument("--clients", type=int, default=1)
    parser.add_argument("--threads", type=int, default=4)
    parser.add_argument("--connections", type=int, default=100)
    parser.add_argument("--duration", default="10s")
    parser.add_argument("--warmup", default="2s")
    parser.add_argument("--repeat", type=int, default=3)
    parser.add_argument("--cpu", action="store_true")
    parser.add_argument("--k6", action="append", choices=sorted(K6_SCRIPTS), default=[])
    parser.add_argument("--php-baseline", action="store_true")
    parser.add_argument("--json", type=Path)
    parser.add_argument("--markdown", type=Path)
    parser.add_argument("--timeout", type=float, default=120.0)
    args = parser.parse_args()

    if shutil.which("wrk") is None:
        raise SystemExit("wrk is required: install it (brew install wrk) and retry")
    if args.clients < 1:
        raise SystemExit("--clients must be at least 1")
    if args.repeat < 1:
        raise SystemExit("--repeat must be at least 1")

    compiler = (args.compiler or build_compiler(args.no_build)).resolve()
    fixture = args.fixture.resolve()
    if not fixture.exists():
        raise SystemExit(f"fixture not found: {fixture}")

    with tempfile.TemporaryDirectory(prefix="elephc-web-bench-") as temp:
        work_dir = Path(temp)
        binary = compile_fixture(compiler, fixture, work_dir, args.timeout)
        port = free_port()
        url = f"http://127.0.0.1:{port}/"
        log_path = work_dir / "server.log"
        with log_path.open("w") as log:
            process = subprocess.Popen(
                [
                    str(binary),
                    "--listen",
                    f"127.0.0.1:{port}",
                    "--workers",
                    str(args.workers),
                ],
                stdout=log,
                stderr=subprocess.STDOUT,
                text=True,
            )
            try:
                wait_for_server(process, url, log_path)
                if args.warmup != "0":
                    run_wrk(
                        url,
                        args.threads,
                        args.connections,
                        args.warmup,
                        args.timeout,
                        args.clients,
                    )
                rounds = []
                for _ in range(args.repeat):
                    before_user, before_system = (
                        server_cpu_split(process.pid) if args.cpu else (0.0, 0.0)
                    )
                    round_ = run_wrk(
                        url,
                        args.threads,
                        args.connections,
                        args.duration,
                        args.timeout,
                        args.clients,
                    )
                    if args.cpu:
                        after_user, after_system = server_cpu_split(process.pid)
                        user = max(0.0, after_user - before_user)
                        system = max(0.0, after_system - before_system)
                        round_["server_user_cpu_seconds"] = user
                        round_["server_system_cpu_seconds"] = system
                        round_["server_cpu_seconds"] = user + system
                        requests = float(round_.get("requests", 0))
                        if requests:
                            round_["server_cpu_us_per_request"] = (
                                round_["server_cpu_seconds"] / requests * 1e6
                            )
                            round_["server_user_us_per_request"] = user / requests * 1e6
                            round_["server_system_us_per_request"] = system / requests * 1e6
                    rounds.append(round_)
                k6_results = [
                    run_k6(scenario, url, args.duration, work_dir, args.timeout)
                    for scenario in args.k6
                ]
            finally:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()

        results: dict[str, Any] = {
            "fixture": str(fixture),
            "workers": args.workers,
            "clients": args.clients,
            "cpu": args.cpu,
            "threads": args.threads,
            "connections": args.connections,
            "duration": args.duration,
            "environment": server_environment(compiler),
            "wrk": {"summary": summarize(rounds), "raw_rounds": rounds},
        }
        if k6_results:
            results["k6"] = k6_results
        if args.php_baseline:
            results["php"] = run_php_baseline(
                fixture,
                work_dir,
                args.threads,
                args.connections,
                args.duration,
                args.timeout,
                args.clients,
            )

    report = markdown_report(results)
    print(report)
    if args.json:
        args.json.write_text(json.dumps(results, indent=2) + "\n")
    if args.markdown:
        args.markdown.write_text(report + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
