"""Validate, migrate, and merge benchmark histories without discarding old dates."""

from __future__ import annotations

import json
import math
import re
import statistics
from datetime import datetime, timezone
from pathlib import Path

from common import SCHEMA_VERSION, SUITES

VARIANTS = {"native": ("elephc", "php", "c"),
            "magician": ("elephc-native", "elephc-eval", "php-native", "php-eval"),
            "hot-path": ("buffer", "indexed", "assoc")}


def timestamp(value: str) -> datetime:
    parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    if parsed.tzinfo is None:
        raise ValueError("benchmark timestamp must include a timezone")
    return parsed.astimezone(timezone.utc)


def validate_cases(suite: str, cases: list) -> None:
    """Reject malformed/partial measurements before any history file is replaced."""
    if not isinstance(cases, list) or not cases:
        raise ValueError(f"{suite}: benchmark cases must be a non-empty list")
    names = set()
    for case in cases:
        name = case.get("case")
        if not isinstance(name, str) or not name or name in names:
            raise ValueError(f"{suite}: invalid or duplicate benchmark case")
        names.add(name)
        variants = case.get("variants", {})
        if not set(VARIANTS[suite]).issubset(variants):
            raise ValueError(f"{suite}/{name}: missing benchmark variants")
        for label, variant in variants.items():
            value = variant.get("median_ms")
            optional = label.startswith("php") or label == "c"
            if value is None:
                if not optional or variant.get("skipped") is not True:
                    raise ValueError(f"{suite}/{name}/{label}: missing mandatory measurement")
            elif type(value) not in (int, float) or not math.isfinite(value) or value < 0 or variant.get("skipped"):
                raise ValueError(f"{suite}/{name}/{label}: invalid median")
            samples = variant.get("samples_ms", [])
            if not isinstance(samples, list) or any(type(v) not in (int, float) or not math.isfinite(v) or v < 0 for v in samples):
                raise ValueError(f"{suite}/{name}/{label}: invalid raw samples")
            if value is None and samples:
                raise ValueError(f"{suite}/{name}/{label}: skipped variant has samples")
            reference = variant.get("reference")
            if reference is not None:
                estimate = reference.get("median_ms") if isinstance(reference, dict) else None
                if (not label.startswith("php") or value is not None or type(estimate) not in (int, float)
                        or not math.isfinite(estimate) or estimate < 0):
                    raise ValueError(f"{suite}/{name}/{label}: invalid historical reference")
                if not isinstance(reference.get("timestamp"), str):
                    raise ValueError(f"{suite}/{name}/{label}: missing reference timestamp")
                timestamp(reference["timestamp"])
                if (not re.fullmatch(r"[0-9a-f]{8,40}", reference.get("commit", ""))
                        or type(reference.get("run_id")) is not int or reference["run_id"] <= 0
                        or not isinstance(reference.get("environment"), dict)):
                    raise ValueError(f"{suite}/{name}/{label}: missing reference provenance")
                if any(not isinstance(reference["environment"].get(key), str) or not reference["environment"][key]
                       for key in ("runner", "os", "architecture", "php", "php_mode")):
                    raise ValueError(f"{suite}/{name}/{label}: incomplete reference environment")


def empty_history(suite: str, branch: str = "main", all_runs: bool = False) -> dict:
    return {"schema_version": SCHEMA_VERSION, "suite": suite, "branch": branch,
            "sampling": "all-runs" if all_runs else "latest-per-UTC-day-and-platform",
            "points": 0, "series": []}


def load_history(path: Path, suite: str, branch: str = "main", all_runs: bool = False) -> dict:
    """Fail on invalid saved history instead of silently rebuilding from zero."""
    if not path.exists():
        return empty_history(suite, branch, all_runs)
    data = json.loads(path.read_text())
    expected = empty_history(suite, branch, all_runs)
    for field in ("schema_version", "suite", "branch", "sampling"):
        if data.get(field) != expected[field]:
            raise ValueError(f"{path}: incompatible {field}; use a separate output directory")
    if not isinstance(data.get("series"), list) or data.get("points") != len(data["series"]):
        raise ValueError(f"{path}: invalid series/point count")
    for record in data["series"]:
        validate_record(suite, record)
    keys = [record_key(r, all_runs) for r in data["series"]]
    if len(keys) != len(set(keys)):
        raise ValueError(f"{path}: duplicate series samples")
    return data


def validate_record(suite: str, record: dict) -> None:
    if record.get("date") != timestamp(record["timestamp"]).date().isoformat():
        raise ValueError("record date does not match its UTC timestamp")
    if not isinstance(record.get("environment"), dict):
        raise ValueError("record is missing environment metadata")
    if not isinstance(record.get("commit"), str) or not re.fullmatch(r"[0-9a-f]{8,40}", record["commit"]):
        raise ValueError("record is missing a valid source commit")
    if type(record.get("run_id")) is not int or record["run_id"] <= 0:
        raise ValueError("record is missing a valid source CI run")
    environment = record["environment"]
    if any(not isinstance(environment.get(field), str) or not environment[field]
           for field in ("runner", "os", "architecture", "measurement")):
        raise ValueError("record has incomplete platform metadata")
    validate_cases(suite, record["cases"])
    if not environment.get("legacy"):
        iterations, warmup = environment.get("iterations"), environment.get("warmup")
        if type(iterations) is not int or iterations < 1 or type(warmup) is not int or warmup < 0:
            raise ValueError("record has invalid measurement parameters")
        for case in record["cases"]:
            fingerprint = case.get("fixture_sha256")
            if not isinstance(fingerprint, str) or not re.fullmatch(r"[0-9a-f]{64}", fingerprint):
                raise ValueError("record is missing a fixture fingerprint")
            for variant in case["variants"].values():
                if variant.get("skipped"):
                    continue
                samples = variant.get("samples_ms", [])
                if len(samples) != iterations or not math.isclose(statistics.median(samples), variant["median_ms"], rel_tol=1e-12):
                    raise ValueError("raw samples do not match the measured count/median")


def legacy_environment(data: dict | None = None) -> dict:
    """Keep missing historical environment information explicitly unknown."""
    data = data or {}
    return {"runner": "unknown", "os": "unknown", "architecture": "unknown",
            "measurement": "process-wall", "iterations": data.get("iterations"),
            "warmup": data.get("warmup"), "legacy": True}


def legacy_variant(value: dict) -> dict:
    return {**value, "samples_ms": value.get("samples_ms", []),
            "skipped": value.get("median_ms") is None}


def migrate_native(data: dict) -> dict:
    """Convert every saved native point, including ones whose artifacts expired."""
    history = empty_history("native", data["branch"])
    history["legacy_source"] = {k: v for k, v in data.items() if k not in {"series", "points"}}
    for record in data["series"]:
        cases = []
        for name, measurements in record["cases"].items():
            variants = {label: legacy_variant({"median_ms": measurements[f"{label}_ms"]})
                        for label in VARIANTS["native"]}
            cases.append({"case": name, "fixture_sha256": None, "metadata": {}, "variants": variants})
        converted = {**record, "cases": cases, "environment": legacy_environment(), "legacy": True}
        validate_record("native", converted)
        history["series"].append(converted)
    history["points"] = len(history["series"])
    return history


def legacy_result(suite: str, data: dict, meta: dict) -> dict:
    """Adapt the two previously separate artifact formats into shared records."""
    cases = []
    for case in data["cases"]:
        variants = {label: legacy_variant(case[label.replace("-", "_")]) for label in VARIANTS[suite]}
        cases.append({"case": case["case"], "fixture_sha256": None,
                      "metadata": case.get("metadata", {}), "variants": variants})
    record = {"date": timestamp(meta["created_at"]).date().isoformat(), "timestamp": meta["created_at"],
              "commit": meta["sha"], "run_id": meta["run_id"], "artifact_id": meta["id"],
              "run_attempt": None, "environment": legacy_environment(data), "legacy": True, "cases": cases}
    validate_record(suite, record)
    return record


def current_results(data: dict, meta: dict, branch: str) -> dict[str, dict]:
    """Check artifact identity before accepting its self-described measurements."""
    if data.get("schema_version") != SCHEMA_VERSION or data.get("branch") != branch:
        raise ValueError("artifact schema or branch does not match the requested history")
    if data.get("commit") != meta["sha"] or data.get("run_id") != meta["run_id"]:
        raise ValueError("artifact source revision/run does not match GitHub metadata")
    timestamp(data["timestamp"])
    suites = data.get("suites", {})
    if set(suites) != set(SUITES):
        raise ValueError("CI results must contain every process benchmark suite")
    records = {}
    for suite, result in suites.items():
        record = {"date": timestamp(meta["created_at"]).date().isoformat(), "timestamp": meta["created_at"],
                  "measurement_timestamp": data["timestamp"], "commit": data["commit"],
                  "run_id": data["run_id"], "run_attempt": data.get("run_attempt"), "artifact_id": meta["id"],
                  "environment": data["environment"], "cases": result["cases"]}
        validate_record(suite, record)
        records[suite] = record
    return records


def record_key(record: dict, all_runs: bool = False) -> tuple:
    environment = record["environment"]
    platform = tuple(environment.get(k, "unknown") for k in ("runner", "os", "architecture"))
    if all_runs:
        return (record["run_id"], record.get("run_attempt"), record["timestamp"], platform)
    return (record["date"], platform)


def order(record: dict) -> tuple:
    return (timestamp(record["timestamp"]), record.get("run_attempt") or 0,
            record.get("artifact_id") or 0, record.get("run_id") or 0)


def backfill_php(series: list[dict]) -> list[dict]:
    """Use the first compatible later PHP sample as an explicit historical reference."""
    sources = {}
    for record in series:
        for case in record["cases"]:
            for label, variant in case["variants"].items():
                if label.startswith("php") and variant["median_ms"] is not None:
                    sources.setdefault((case["case"], label), []).append((record, case, variant))
    filled = []
    for record in series:
        cases = []
        for case in record["cases"]:
            variants = dict(case["variants"])
            for label, variant in variants.items():
                if not label.startswith("php") or variant["median_ms"] is not None or variant.get("reference"):
                    continue
                for source, fixture, measured in sources.get((case["case"], label), []):
                    if timestamp(source["timestamp"]) < timestamp(record["timestamp"]):
                        continue
                    if any((record["environment"].get(key) or "unknown") not in ("unknown", source["environment"].get(key))
                           for key in ("runner", "os", "architecture", "php", "php_mode")):
                        continue
                    if case.get("fixture_sha256") and case["fixture_sha256"] != fixture.get("fixture_sha256"):
                        continue
                    reference = {"median_ms": measured["median_ms"],
                                 "timestamp": source.get("measurement_timestamp", source["timestamp"]),
                                 "commit": source["commit"], "run_id": source["run_id"],
                                 "fixture_sha256": fixture.get("fixture_sha256"),
                                 "environment": {key: source["environment"].get(key) or "unknown"
                                                 for key in ("runner", "os", "architecture", "php", "php_mode")}}
                    variants[label] = {**variant, "reference": reference}
                    break
            cases.append({**case, "variants": variants})
        filled.append({**record, "cases": cases})
    return filled


def merge(history: dict, records: list[dict]) -> dict:
    """Merge newest daily samples while retaining unknown legacy fields and old days."""
    all_runs = history["sampling"] == "all-runs"
    merged = {record_key(r, all_runs): r for r in history["series"]}
    for record in records:
        validate_record(history["suite"], record)
        key = record_key(record, all_runs)
        if key not in merged or order(record) > order(merged[key]):
            # Carry any fields unknown to today's importer forward on replacement.
            merged[key] = {**merged.get(key, {}), **record}
    series = backfill_php(sorted(merged.values(), key=order))
    return {**history, "points": len(series), "series": series}
