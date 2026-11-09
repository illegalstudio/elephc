#!/usr/bin/env python3
"""Compare an Elephc origin with an Nginx + PHP-FPM reference endpoint.

Both endpoints must already be running. This script deliberately owns only the
client side: it does not start, stop, or configure either server. Point
`--elephc-url` at the Elephc origin (directly or through the edge under test)
and `--fpm-url` at the Nginx endpoint that serves the equivalent fixture.

It reports two distinct shapes:

* HTTP keep-alive (`wrk`) for steady-state throughput and latency;
* one new HTTP connection per request, sequentially, to expose connection-setup
  costs without the client-side TIME_WAIT noise of a high-concurrency churn run.

The script is a comparison tool, not a historical benchmark sample. CI keeps
its process-execution benchmarks separate.
"""

from __future__ import annotations

import argparse
import json
import socket
import statistics
import subprocess
import sys
import time
import urllib.parse
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
import run  # noqa: E402  (the sibling Elephc runner owns wrk parsing)


WRK_SCENARIO = "HTTP keep-alive"
CONNECTION_SCENARIO = "new HTTP connection"


def parse_origin(url: str) -> tuple[str, int, str]:
    """Returns `(host, port, request_target)` for an HTTP origin URL."""
    parsed = urllib.parse.urlsplit(url)
    if parsed.scheme not in ("http", ""):
        raise ValueError(f"only plain HTTP origins are supported: {url}")
    if not parsed.hostname:
        raise ValueError(f"missing host in URL: {url}")
    port = parsed.port or 80
    target = parsed.path or "/"
    if parsed.query:
        target += "?" + parsed.query
    return parsed.hostname, port, target


def new_connection_sample(url: str, timeout: float = 5.0) -> tuple[float, int]:
    """Issues one HTTP/1.1 GET over a fresh TCP connection.

    Returns `(elapsed_ms, status_code)`. The connection is always closed so the
    next sample measures a new accept path.
    """
    host, port, target = parse_origin(url)
    start = time.perf_counter()
    with socket.create_connection((host, port), timeout=timeout) as stream:
        stream.settimeout(timeout)
        stream.sendall(
            f"GET {target} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n".encode()
        )
        data = bytearray()
        while True:
            chunk = stream.recv(65536)
            if not chunk:
                break
            data.extend(chunk)
    elapsed = (time.perf_counter() - start) * 1000.0
    status = 0
    if data.startswith(b"HTTP/"):
        try:
            status = int(bytes(data.split(b" ", 2)[1]))
        except (IndexError, ValueError):
            status = 0
    return elapsed, status


def measure_new_connections(
    url: str, count: int, timeout: float = 5.0
) -> dict[str, Any]:
    """Runs `count` sequential fresh-connection requests against `url`."""
    samples: list[float] = []
    failures = 0
    for _ in range(count):
        try:
            elapsed, status = new_connection_sample(url, timeout=timeout)
        except OSError:
            failures += 1
            continue
        if status != 200:
            failures += 1
            continue
        samples.append(elapsed)
    if not samples:
        return {
            "requests": 0,
            "failures": failures,
            "requests_per_sec": 0.0,
            "latency_p50_ms": None,
            "latency_p99_ms": None,
        }
    samples.sort()
    p50 = samples[len(samples) // 2]
    p99 = samples[min(len(samples) - 1, int(len(samples) * 0.99))]
    return {
        "requests": len(samples),
        "failures": failures,
        "requests_per_sec": 1000.0 / statistics.fmean(samples),
        "latency_p50_ms": p50,
        "latency_p99_ms": p99,
    }


def measure_wrk(
    url: str, threads: int, connections: int, duration: str
) -> dict[str, Any]:
    """Runs one `wrk` measurement and returns its parsed metrics."""
    completed = subprocess.run(
        [
            "wrk",
            "-t",
            str(threads),
            "-c",
            str(connections),
            "-d",
            duration,
            "--latency",
            url,
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    text = completed.stdout + completed.stderr
    if completed.returncode != 0 and "Requests/sec" not in text:
        raise RuntimeError(f"wrk failed for {url}:\n{text}")
    return run.parse_wrk(text)


def summarize(samples: list[dict[str, Any]]) -> dict[str, Any]:
    """Reduces repeat samples to medians for each numeric metric."""
    metrics = (
        "requests_per_sec",
        "latency_avg_ms",
        "latency_p50_ms",
        "latency_p99_ms",
        "non_2xx",
        "requests",
    )
    summary: dict[str, Any] = {}
    for metric in metrics:
        values = [
            float(sample[metric])
            for sample in samples
            if isinstance(sample.get(metric), (int, float))
        ]
        if values:
            summary[metric] = statistics.median(values)
    summary["failures"] = sum(
        int(sample.get("non_2xx", 0)) + int(sample.get("failures", 0))
        for sample in samples
    )
    summary["socket_errors"] = [
        sample.get("socket_errors", "none") for sample in samples
    ]
    summary["raw"] = samples
    return summary


def ratio(left: float | None, right: float | None) -> str:
    """Formats a right/left ratio, or `n/a` when a side is missing."""
    if not left or right is None:
        return "n/a"
    return f"{right / left:.2f}x"


def compare(
    elephc: dict[str, Any], fpm: dict[str, Any]
) -> dict[str, dict[str, Any]]:
    """Builds the per-scenario comparison payload."""
    result: dict[str, dict[str, Any]] = {}
    for scenario in (WRK_SCENARIO, CONNECTION_SCENARIO):
        left = elephc.get(scenario)
        right = fpm.get(scenario)
        if not left or not right:
            continue
        result[scenario] = {
            "elephc": left,
            "fpm": right,
            "throughput_ratio": ratio(
                left.get("requests_per_sec"), right.get("requests_per_sec")
            ),
            "latency_p50_ratio": ratio(
                left.get("latency_p50_ms"), right.get("latency_p50_ms")
            ),
            "latency_p99_ratio": ratio(
                left.get("latency_p99_ms"), right.get("latency_p99_ms")
            ),
        }
    return result


def markdown_report(
    args: argparse.Namespace, result: dict[str, dict[str, Any]]
) -> str:
    """Renders a compact Markdown comparison table."""
    lines = [
        "# Elephc vs Nginx + PHP-FPM",
        "",
        f"- Elephc: `{args.elephc_url}`",
        f"- FPM: `{args.fpm_url}`",
        f"- Repeats: {args.repeat}",
        "",
        "| Scenario | Metric | Elephc | FPM | FPM / Elephc |",
        "|---|---:|---:|---:|---:|",
    ]
    for scenario, values in result.items():
        left = values["elephc"]
        right = values["fpm"]
        for metric, label, key in (
            ("requests_per_sec", "Throughput (req/s)", "throughput_ratio"),
            ("latency_p50_ms", "p50 (ms)", "latency_p50_ratio"),
            ("latency_p99_ms", "p99 (ms)", "latency_p99_ratio"),
        ):
            lv = left.get(metric)
            rv = right.get(metric)
            if lv is None and rv is None:
                continue
            lines.append(
                f"| {scenario} | {label} | "
                f"{lv if lv is not None else 'n/a'} | "
                f"{rv if rv is not None else 'n/a'} | "
                f"{values[key]} |"
            )
    lines.append("")
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--elephc-url", required=True)
    parser.add_argument("--fpm-url", required=True)
    parser.add_argument("--threads", type=int, default=4)
    parser.add_argument("--connections", type=int, default=100)
    parser.add_argument("--duration", default="10s")
    parser.add_argument("--repeat", type=int, default=3)
    parser.add_argument("--new-connections", type=int, default=500)
    parser.add_argument("--skip-keepalive", action="store_true")
    parser.add_argument("--skip-new-connection", action="store_true")
    parser.add_argument("--json")
    parser.add_argument("--markdown")
    parser.add_argument("--timeout", type=float, default=5.0)
    args = parser.parse_args()

    if args.repeat < 1:
        parser.error("--repeat must be >= 1")
    if args.new_connections < 1:
        parser.error("--new-connections must be >= 1")
    if not args.skip_new_connection:
        for url in (args.elephc_url, args.fpm_url):
            parse_origin(url)

    results: dict[str, Any] = {
        "elephc_url": args.elephc_url,
        "fpm_url": args.fpm_url,
        "threads": args.threads,
        "connections": args.connections,
        "duration": args.duration,
        "repeat": args.repeat,
        "new_connections": args.new_connections,
        "scenarios": {},
    }

    if not args.skip_keepalive:
        left_samples: list[dict[str, Any]] = []
        right_samples: list[dict[str, Any]] = []
        for index in range(args.repeat):
            # Alternate order so a machine-load drift does not always favour the
            # first server.
            pair = (
                (args.elephc_url, left_samples),
                (args.fpm_url, right_samples),
            )
            if index % 2:
                pair = tuple(reversed(pair))
            for url, samples in pair:
                samples.append(
                    measure_wrk(url, args.threads, args.connections, args.duration)
                )
                time.sleep(0.5)
        results["scenarios"][WRK_SCENARIO] = {
            "elephc": summarize(left_samples),
            "fpm": summarize(right_samples),
        }

    if not args.skip_new_connection:
        left_samples: list[dict[str, Any]] = []
        right_samples: list[dict[str, Any]] = []
        for index in range(args.repeat):
            pair = (
                (args.elephc_url, left_samples),
                (args.fpm_url, right_samples),
            )
            if index % 2:
                pair = tuple(reversed(pair))
            for url, samples in pair:
                samples.append(
                    measure_new_connections(url, args.new_connections, args.timeout)
                )
                time.sleep(0.5)
        results["scenarios"][CONNECTION_SCENARIO] = {
            "elephc": summarize(left_samples),
            "fpm": summarize(right_samples),
        }

    comparison = compare(
        {
            scenario: values["elephc"]
            for scenario, values in results["scenarios"].items()
        },
        {
            scenario: values["fpm"]
            for scenario, values in results["scenarios"].items()
        },
    )
    results["comparison"] = comparison

    report = markdown_report(args, comparison)
    print(report)
    if args.json:
        Path(args.json).write_text(json.dumps(results, indent=2) + "\n")
    if args.markdown:
        Path(args.markdown).write_text(report + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
