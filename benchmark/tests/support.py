"""Small result fixtures shared by history and publication regression tests."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from history import VARIANTS


def record(suite="native", day="2026-10-10", hour="12", run_id=1, runner="macos-15"):
    variants = {label: {"median_ms": 2.0, "samples_ms": [1.0, 2.0, 3.0], "skipped": False}
                for label in VARIANTS[suite]}
    return {"date": day, "timestamp": f"{day}T{hour}:00:00Z", "commit": "a" * 40,
            "run_id": run_id, "run_attempt": 1, "artifact_id": run_id,
            "environment": {"runner": runner, "os": "Darwin", "architecture": "arm64",
                            "measurement": "process-wall", "iterations": 3, "warmup": 1},
            "cases": [{"case": "example", "fixture_sha256": "b" * 64, "metadata": {}, "variants": variants}]}
