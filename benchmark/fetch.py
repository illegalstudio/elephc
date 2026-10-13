#!/usr/bin/env python3
"""Recover CI artifacts and refresh native, Magician, and hot-path daily histories."""

from __future__ import annotations

import argparse
import json
import subprocess
import tempfile
import zipfile
from pathlib import Path

from common import ROOT, SUITES, atomic_json
from history import current_results, legacy_result, load_history, merge, order, record_key, timestamp

ARTIFACT_NAME = "benchmark-results"


def gh(*arguments: str) -> str:
    result = subprocess.run(["gh", *arguments], capture_output=True, text=True, timeout=180)
    if result.returncode:
        raise RuntimeError(f"gh {' '.join(arguments)} failed:\n{result.stderr}")
    return result.stdout


def list_artifacts(repo: str, branch: str) -> list[dict]:
    """Filter by artifact name server-side instead of scanning every CI archive."""
    pages = json.loads(gh("api", f"repos/{repo}/actions/artifacts?name={ARTIFACT_NAME}&per_page=100",
                         "--paginate", "--slurp"))
    artifacts = []
    for page in pages:
        for artifact in page["artifacts"]:
            run = artifact["workflow_run"]
            if artifact["name"] != ARTIFACT_NAME or artifact["expired"] or run["head_branch"] != branch:
                continue
            if run["head_repository_id"] != run["repository_id"]:
                continue
            artifacts.append({"id": artifact["id"], "created_at": artifact["created_at"],
                              "run_id": run["id"], "sha": run["head_sha"]})
    return sorted(artifacts, key=lambda a: (timestamp(a["created_at"]), a["id"]))


def download_artifact(repo: str, meta: dict, directory: Path) -> None:
    """Fetch the selected artifact ID so reruns cannot substitute an older archive."""
    archive = directory.with_suffix(".zip")
    with archive.open("wb") as output:
        result = subprocess.run(["gh", "api", f"repos/{repo}/actions/artifacts/{meta['id']}/zip"],
                                stdout=output, stderr=subprocess.PIPE, text=True, timeout=180)
    if result.returncode:
        raise RuntimeError(f"artifact {meta['id']} download failed: {result.stderr}")
    directory.mkdir()
    with zipfile.ZipFile(archive) as contents:
        for name in ("results.json", "benchmark-results.json", "magician-benchmark-results.json"):
            if name in contents.namelist():
                (directory / name).write_bytes(contents.read(name))


def read_artifact(directory: Path, meta: dict, branch: str) -> dict[str, dict]:
    current = directory / "results.json"
    if current.exists():
        return current_results(json.loads(current.read_text()), meta, branch)
    records = {}
    for suite, filename in (("native", "benchmark-results.json"),
                            ("magician", "magician-benchmark-results.json")):
        path = directory / filename
        if path.exists():
            records[suite] = legacy_result(suite, json.loads(path.read_text()), meta)
    if not records:
        raise ValueError(f"artifact {meta['id']}: no supported result files")
    return records


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
    pending = [meta for meta in artifacts if not cached_import(meta, imported, histories)]
    print(f"{repo}/{branch}: {len(artifacts)} available artifacts, {len(pending)} to import", flush=True)
    incoming = {suite: [] for suite in SUITES}
    with tempfile.TemporaryDirectory(prefix="elephc-benchmark-fetch-") as temporary:
        for index, meta in enumerate(pending, 1):
            dest = Path(temporary) / str(meta["id"])
            download_artifact(repo, meta, dest)
            records = read_artifact(dest, meta, branch)
            for suite, record in records.items():
                incoming[suite].append(record)
            imported[str(meta["id"])] = {
                "timestamp": meta["created_at"], "suites": sorted(records),
                "samples": {suite: {field: record.get(field) for field in
                                    ("date", "timestamp", "run_id", "run_attempt", "artifact_id", "environment")}
                            for suite, record in records.items()}}
            print(f"[{index}/{len(pending)}] {meta['created_at'][:10]} run {meta['run_id']}: "
                  + ", ".join(records), flush=True)
    # Select by suite, UTC day, and platform before backfilling historical PHP gaps.
    histories = {suite: merge(history, incoming[suite]) for suite, history in histories.items()}
    # An old artifact is no longer needed for refresh checkpointing after expiry.
    available_ids = {str(meta["id"]) for meta in artifacts}
    imported = {key: value for key, value in imported.items() if key in available_ids}
    for suite, history in histories.items():
        atomic_json(directory / f"{suite}.json", history)
    atomic_json(manifest, imported)
    print("Saved " + ", ".join(f"{suite}: {history['points']} points" for suite, history in histories.items()), flush=True)
    return histories


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
