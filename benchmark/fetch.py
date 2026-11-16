#!/usr/bin/env python3
"""Recover CI artifacts and refresh native, Magician, and hot-path daily histories."""

from __future__ import annotations

import argparse
import json
import tempfile
from pathlib import Path

from common import ROOT, SUITES, atomic_json
from history import load_history, merge, order, record_key
from artifacts import download_artifact, gh, list_artifacts, read_artifact
from archive import list_archives, read_archive


def cached_import(meta: dict, imported: dict, histories: dict) -> bool:
    """Accept retained or superseded samples, with exact-ID fallback for old manifests."""
    cached = imported.get(str(meta["id"]))
    if not cached:
        return False
    for suite in cached["suites"]:
        history = histories[suite]
        sample = cached.get("samples", {}).get(suite)
        if sample is None:
            present = any(r.get("artifact_id") == meta["id"] for r in history["series"])
        else:
            all_runs = history["sampling"] == "all-runs"
            key = record_key(sample, all_runs)
            present = any(record_key(r, all_runs) == key and order(r) >= order(sample)
                          for r in history["series"])
        if not present:
            return False
    return True


def refresh(repo: str, branch: str, directory: Path, all_runs: bool = False) -> dict:
    """Validate every input before writing any history or import checkpoint."""
    histories = {suite: load_history(directory / f"{suite}.json", suite, branch, all_runs) for suite in SUITES}
    manifest = directory / "imports.json"
    imported = json.loads(manifest.read_text()) if manifest.exists() else {}
    if not isinstance(imported, dict):
        raise ValueError("invalid artifact import manifest")
    artifacts = list_artifacts(repo, branch)
    archives = list_archives(repo, branch)
    incoming = {suite: [] for suite in SUITES}
    for entry in archives:
        cached = imported.get(str(entry["id"]), {})
        if cached.get("archive_sha") == entry["sha"] and cached_import(entry, imported, histories):
            continue
        meta, records = read_archive(repo, entry)
        for suite, record in records.items():
            incoming[suite].append(record)
        imported[str(meta["id"])] = checkpoint(meta, records, entry["sha"])
    available = {suite: merge(history, incoming[suite]) for suite, history in histories.items()}
    pending = [meta for meta in artifacts if not cached_import(meta, imported, available)]
    print(f"{repo}/{branch}: {len(artifacts)} available artifacts, {len(pending)} to import", flush=True)
    with tempfile.TemporaryDirectory(prefix="elephc-benchmark-fetch-") as temporary:
        for index, meta in enumerate(pending, 1):
            dest = Path(temporary) / str(meta["id"])
            download_artifact(repo, meta, dest)
            records = read_artifact(dest, meta, branch)
            for suite, record in records.items():
                incoming[suite].append(record)
            imported[str(meta["id"])] = checkpoint(meta, records)
            print(f"[{index}/{len(pending)}] {meta['created_at'][:10]} run {meta['run_id']}: "
                  + ", ".join(records), flush=True)
    # Select by suite, UTC day, and platform before backfilling historical PHP gaps.
    histories = {suite: merge(history, incoming[suite]) for suite, history in histories.items()}
    # An old artifact is no longer needed for refresh checkpointing after expiry.
    available_ids = {str(meta["id"]) for meta in artifacts + archives}
    imported = {key: value for key, value in imported.items() if key in available_ids}
    for suite, history in histories.items():
        atomic_json(directory / f"{suite}.json", history)
    atomic_json(manifest, imported)
    print("Saved " + ", ".join(f"{suite}: {history['points']} points" for suite, history in histories.items()), flush=True)
    return histories


def checkpoint(meta, records, archive_sha=None):
    """Remember each accepted sample and the exact durable blob when available."""
    return {"timestamp": meta["created_at"], "suites": sorted(records),
            "archive_sha": archive_sha,
            "samples": {suite: {field: record.get(field) for field in
                                ("date", "timestamp", "run_id", "run_attempt", "artifact_id", "environment")}
                        for suite, record in records.items()}}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", help="owner/repo; defaults to the current repository")
    parser.add_argument("--branch", default="main")
    parser.add_argument("--out-dir", type=Path, default=ROOT / "data")
    parser.add_argument("--all-runs", action="store_true", help="Use a separate output directory for this sampling mode")
    args = parser.parse_args()
    repo = args.repo or gh("repo", "view", "--json", "nameWithOwner", "-q", ".nameWithOwner").strip()
    refresh(repo, args.branch, args.out_dir, args.all_runs)


if __name__ == "__main__":
    main()
