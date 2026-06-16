#!/usr/bin/env python3
"""Rebuild the historical benchmark time-series from CI artifacts.

The CI workflow (.github/workflows/ci.yml, job "Benchmark Suite") runs
scripts/benchmark_suite.py on every push/PR and uploads a `benchmark-results`
artifact containing benchmark-results.json. GitHub keeps those artifacts for ~90
days. This script reconstructs a single tidy time-series out of them so the data
survives artifact expiry as a committed file (.benchmarks/series.json).

Sampling policy: only runs on `main`, one sample per calendar day (the last run
of that day), to keep a clean product-evolution line without per-PR noise.

Requires: `gh` (authenticated) and `python3`. Run from anywhere inside the repo:

    python3 .benchmarks/fetch_series.py

Pass --out to write somewhere else, --all-runs to keep every run instead of one
per day, or --branch to sample a branch other than main.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path

ARTIFACT_NAME = "benchmark-results"


def parse_args() -> argparse.Namespace:
    """Parse CLI options controlling output path, branch, and sampling."""
    p = argparse.ArgumentParser(description="Rebuild benchmark series from CI artifacts.")
    p.add_argument("--out", type=Path, default=Path(__file__).resolve().parent / "series.json",
                   help="Where to write the series JSON (default: .benchmarks/series.json).")
    p.add_argument("--branch", default="main", help="Branch to sample (default: main).")
    p.add_argument("--all-runs", action="store_true",
                   help="Keep every run instead of one sample per day.")
    p.add_argument("--rebuild", action="store_true",
                   help="Overwrite from scratch instead of merging into the "
                        "existing series (DESTRUCTIVE: drops expired history).")
    return p.parse_args()


def repo_slug() -> str:
    """Return the current repository as `owner/name` via gh."""
    return subprocess.run(
        ["gh", "repo", "view", "--json", "nameWithOwner", "-q", ".nameWithOwner"],
        text=True, capture_output=True, check=True).stdout.strip()


def list_artifacts(repo: str, branch: str) -> list[dict]:
    """List all non-expired benchmark-results artifacts for the given branch.

    Returns one dict per artifact with created_at, run id, and head commit sha.
    """
    jq = (f'.artifacts[] | select(.name=="{ARTIFACT_NAME}" and .expired==false '
          f'and .workflow_run.head_branch=="{branch}") '
          f'| {{created_at: .created_at, run_id: .workflow_run.id, '
          f'sha: .workflow_run.head_sha}}')
    raw = subprocess.run(
        ["gh", "api", f"repos/{repo}/actions/artifacts?per_page=100", "--paginate", "--jq", jq],
        text=True, capture_output=True, check=True).stdout
    items = [json.loads(line) for line in raw.splitlines() if line.strip()]
    items.sort(key=lambda x: x["created_at"])
    return items


def sample_one_per_day(items: list[dict]) -> list[dict]:
    """Collapse to the last artifact of each calendar day (items must be sorted)."""
    by_day: dict[str, dict] = {}
    for it in items:
        by_day[it["created_at"][:10]] = it
    return [by_day[k] for k in sorted(by_day)]


def download_results(repo: str, run_id: int, dest: Path) -> dict | None:
    """Download benchmark-results.json for one run; return parsed JSON or None."""
    subprocess.run(["gh", "run", "download", str(run_id), "-R", repo,
                    "-n", ARTIFACT_NAME, "-D", str(dest)],
                   capture_output=True, check=False)
    jf = dest / "benchmark-results.json"
    if not jf.exists():
        return None
    return json.loads(jf.read_text())


def extract(record_meta: dict, data: dict) -> dict:
    """Flatten one run's benchmark JSON into a per-day series record."""
    rec = {
        "date": record_meta["created_at"][:10],
        "timestamp": record_meta["created_at"],
        "commit": record_meta["sha"][:8],
        "run_id": record_meta["run_id"],
        "cases": {},
    }
    for c in data.get("cases", []):
        rec["cases"][c["case"]] = {
            "elephc_ms": c["elephc"]["median_ms"],
            "c_ms": c["c"]["median_ms"],
            "php_ms": c["php"]["median_ms"],
        }
    return rec


def load_existing(out: Path) -> dict[str, dict]:
    """Load already-committed series points keyed by date, or empty if absent."""
    if not out.exists():
        return {}
    try:
        prior = json.loads(out.read_text())
    except (json.JSONDecodeError, OSError):
        return {}
    return {rec["date"]: rec for rec in prior.get("series", [])}


def main() -> None:
    """Fetch, sample, and merge the benchmark time-series JSON.

    By default this is INCREMENTAL: points already in the output file are kept,
    and only dates not yet recorded are fetched and added. This guarantees that
    history is never lost when old CI artifacts expire. Pass --rebuild to discard
    the existing file and rebuild purely from currently-available artifacts.
    """
    args = parse_args()
    repo = repo_slug()
    print(f"repo: {repo}  branch: {args.branch}", file=sys.stderr)

    existing = {} if args.rebuild else load_existing(args.out)
    if existing:
        print(f"existing committed points: {len(existing)} (kept)", file=sys.stderr)

    artifacts = list_artifacts(repo, args.branch)
    print(f"non-expired artifacts on {args.branch}: {len(artifacts)}", file=sys.stderr)
    if not args.all_runs:
        artifacts = sample_one_per_day(artifacts)
        print(f"sampled (1/day): {len(artifacts)}", file=sys.stderr)

    # Skip artifacts whose date we already have: avoids re-downloading the whole
    # history on every refresh and preserves the originally-recorded values.
    pending = [m for m in artifacts if m["created_at"][:10] not in existing]
    print(f"new dates to fetch: {len(pending)}", file=sys.stderr)

    merged = dict(existing)
    added = 0
    with tempfile.TemporaryDirectory(prefix="bench-series-") as tmp:
        for i, meta in enumerate(pending, 1):
            dest = Path(tmp) / str(meta["run_id"])
            data = download_results(repo, meta["run_id"], dest)
            if data is None:
                print(f"  MISS {meta['created_at'][:10]} run={meta['run_id']}", file=sys.stderr)
                continue
            merged[meta["created_at"][:10]] = extract(meta, data)
            added += 1
            print(f"  [{i}/{len(pending)}] +{meta['created_at'][:10]} {meta['sha'][:8]}",
                  file=sys.stderr)

    series = [merged[d] for d in sorted(merged)]
    print(f"added {added} new point(s); total {len(series)}", file=sys.stderr)

    payload = {
        "source": "GitHub Actions CI artifacts (benchmark-results)",
        "workflow": ".github/workflows/ci.yml :: Benchmark Suite",
        "generator": "scripts/benchmark_suite.py",
        "branch": args.branch,
        "sampling": "all-runs" if args.all_runs else "one-per-day (last run of each day)",
        "points": len(series),
        "series": series,
    }
    args.out.write_text(json.dumps(payload, indent=2) + "\n")
    print(f"wrote {args.out} ({len(series)} points)", file=sys.stderr)


if __name__ == "__main__":
    main()
