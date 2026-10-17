#!/usr/bin/env python3
"""Compare json_decode times before and after deferred error locations.

The script does not build either compiler. Pass two already-built elephc
binaries. Both binaries compile the same driver, then the runs alternate on
one CPU with the same 64 MiB heap. Input files are loaded before hrtime()
starts, and the checksum loop stays outside that interval.
"""

from __future__ import annotations

import argparse
import json
import os
import statistics
import subprocess
import time
from pathlib import Path

HEAP = "67108864"
WARMUP = 2
SAMPLES = 21


def row_200() -> str:
    """Return one compact JSON array of the integers 0 through 199."""
    return "[" + ",".join(str(i) for i in range(200)) + "]"


def nested_rows(count: int) -> str:
    """Return `count` copies of `row_200` inside one compact JSON array."""
    row = row_200()
    return "[" + ",".join(row for _ in range(count)) + "]"


def flat_ints(count: int) -> str:
    """Return a compact JSON array of the integers 0 through `count - 1`."""
    return "[" + ",".join(str(i) for i in range(count)) + "]"


def nested_objects(count: int) -> str:
    """Return `count` compact objects, each holding one nested object."""
    parts = []
    for i in range(count):
        parts.append('{"id":%d,"child":{"n":%d,"m":%d}}' % (i, i, i))
    return "[" + ",".join(parts) + "]"


def write_inputs(directory: Path) -> dict[str, dict[str, object]]:
    """Write the benchmark documents and the expected decode checksums."""
    directory.mkdir(parents=True, exist_ok=True)
    documents = {
        "nested200": nested_rows(200),
        "nested400": nested_rows(400),
        "nested800": nested_rows(800),
        "flat160000": flat_ints(160_000),
        "objects400": nested_objects(400),
        "small_row": row_200(),
        "malformed_small": "[{",
        "nested800_bad": nested_rows(800) + "x",
    }
    expected_sum = {
        "nested200": 200 * 19900,
        "nested400": 400 * 19900,
        "nested800": 800 * 19900,
        "flat160000": 159999 * 160000 // 2,
        "objects400": sum(3 * i for i in range(400)),
        "small_row": 19900,
        "malformed_small": 0,
        "nested800_bad": 0,
    }
    expected_error = {
        "malformed_small": 4,
        "nested800_bad": 4,
    }
    specs = {}
    for name, text in documents.items():
        path = directory / f"{name}.json"
        path.write_text(text)
        specs[name] = {
            "path": str(path),
            "bytes": len(text.encode()),
            "sum": expected_sum[name],
            "error": expected_error.get(name, 0),
        }
    return specs


DRIVER = r"""<?php
$json = file_get_contents($argv[1]);
$start = hrtime(true);
$decoded = json_decode($json, true);
$stop = hrtime(true);
$sum = 0;
if (is_array($decoded)) {
    foreach ($decoded as $item) {
        if (is_array($item)) {
            foreach ($item as $value) {
                if (is_int($value)) {
                    $sum += $value;
                } elseif (is_array($value)) {
                    foreach ($value as $inner) {
                        if (is_int($inner)) {
                            $sum += $inner;
                        }
                    }
                }
            }
        } elseif (is_int($item)) {
            $sum += $item;
        }
    }
}
echo ($stop - $start), "\n", json_last_error(), "\n", $sum, "\n";
"""


def compile_driver(compiler: Path, source: Path, binary: Path, extra: list[str]) -> None:
    """Compile the shared driver with the requested elephc binary.

    elephc writes the executable next to the source, using the source stem.
    """
    source.write_text(DRIVER)
    produced = source.with_suffix("")
    if produced.exists():
        produced.unlink()
    command = [
        str(compiler),
        f"--heap-size={HEAP}",
        *extra,
        str(source),
    ]
    result = subprocess.run(command, text=True, capture_output=True)
    if result.returncode != 0 or not produced.exists():
        raise SystemExit(
            f"compile failed for {compiler}:\n{result.stdout}\n{result.stderr}"
        )
    if produced != binary:
        produced.replace(binary)


def run_once(binary: Path, document: Path) -> tuple[int, int, int, float]:
    """Run one decode and return body nanoseconds, error, checksum, and wall seconds."""
    started = time.perf_counter()
    result = subprocess.run(
        ["taskset", "-c", "0", str(binary), str(document)],
        text=True,
        capture_output=True,
    )
    elapsed = time.perf_counter() - started
    if result.returncode != 0:
        raise SystemExit(
            f"{binary} failed on {document}:\n{result.stdout}\n{result.stderr}"
        )
    lines = [line for line in result.stdout.splitlines() if line]
    if len(lines) < 3:
        raise SystemExit(f"unexpected stdout from {binary}: {result.stdout!r}")
    return int(lines[0]), int(lines[1]), int(lines[2]), elapsed


def median(values: list[float]) -> float:
    """Return the middle value of an odd-sized sample."""
    ordered = sorted(values)
    return ordered[len(ordered) // 2]


def mad(values: list[float]) -> float:
    """Return the median absolute deviation from the sample median."""
    center = median(values)
    return median([abs(value - center) for value in values])


def php_check(document: Path, expected_error: int, expected_sum: int) -> None:
    """Check the document with the host PHP CLI."""
    code = DRIVER.removeprefix("<?php\n")
    code = code.replace(
        "file_get_contents($argv[1])",
        f"file_get_contents({json.dumps(str(document))})",
    )
    result = subprocess.run(["php", "-r", code], text=True, capture_output=True)
    if result.returncode != 0:
        raise SystemExit(result.stderr)
    lines = [line for line in result.stdout.splitlines() if line]
    error = int(lines[1])
    checksum = int(lines[2])
    if error != expected_error or checksum != expected_sum:
        raise SystemExit(
            f"php mismatch for {document}: error {error} sum {checksum}, "
            f"expected {expected_error} {expected_sum}\n{result.stdout}"
        )


def summarize(samples: list[tuple[int, float]]) -> dict[str, object]:
    """Summarize decode-body nanoseconds and process wall seconds."""
    body = [float(sample[0]) for sample in samples]
    wall = [sample[1] for sample in samples]
    body_median = median(body)
    wall_median = median(wall)
    return {
        "body_ns": body,
        "wall_s": wall,
        "body_median_ns": body_median,
        "body_mad_ns": mad(body),
        "body_mad_over_median": (mad(body) / body_median) if body_median else None,
        "wall_median_s": wall_median,
        "wall_mad_s": mad(wall),
        "wall_mad_over_median": (mad(wall) / wall_median) if wall_median else None,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--work", type=Path, default=Path("/tmp/elephc-json-loc-bench"))
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    specs = write_inputs(args.work)
    baseline_bin = args.work / "driver-baseline"
    candidate_bin = args.work / "driver-candidate"
    compile_driver(args.baseline, args.work / "driver-baseline.php", baseline_bin, [])
    compile_driver(args.candidate, args.work / "driver-candidate.php", candidate_bin, [])

    for name, spec in specs.items():
        php_check(Path(spec["path"]), int(spec["error"]), int(spec["sum"]))

    report: dict[str, object] = {
        "heap_bytes": int(HEAP),
        "warmup": WARMUP,
        "samples": SAMPLES,
        "cpu": "taskset -c 0",
        "inputs": specs,
        "cases": {},
    }
    cases: dict[str, object] = {}
    for name, spec in specs.items():
        document = Path(spec["path"])
        for _ in range(WARMUP):
            run_once(baseline_bin, document)
            run_once(candidate_bin, document)
        baseline_samples = []
        candidate_samples = []
        for _ in range(SAMPLES):
            base = run_once(baseline_bin, document)
            cand = run_once(candidate_bin, document)
            if base[1] != spec["error"] or base[2] != spec["sum"]:
                raise SystemExit(f"baseline checksum mismatch {name}: {base}")
            if cand[1] != spec["error"] or cand[2] != spec["sum"]:
                raise SystemExit(f"candidate checksum mismatch {name}: {cand}")
            baseline_samples.append((base[0], base[3]))
            candidate_samples.append((cand[0], cand[3]))
        base_summary = summarize(baseline_samples)
        cand_summary = summarize(candidate_samples)
        base_median = float(base_summary["body_median_ns"])
        cand_median = float(cand_summary["body_median_ns"])
        cases[name] = {
            "bytes": spec["bytes"],
            "baseline": base_summary,
            "candidate": cand_summary,
            "candidate_over_baseline": (cand_median / base_median) if base_median else None,
        }
        print(
            f"{name}: baseline {base_median:.0f} ns "
            f"(MAD {base_summary['body_mad_ns']:.0f}), "
            f"candidate {cand_median:.0f} ns "
            f"(MAD {cand_summary['body_mad_ns']:.0f}), "
            f"ratio {cand_median / base_median:.3f}"
        )
    report["cases"] = cases
    nested = cases
    t400 = float(nested["nested400"]["candidate"]["body_median_ns"])
    t800 = float(nested["nested800"]["candidate"]["body_median_ns"])
    report["candidate_t800_over_t400"] = (t800 / t400) if t400 else None
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, indent=2) + "\n")
    print(f"wrote {args.out}")


if __name__ == "__main__":
    main()
