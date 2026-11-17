#!/usr/bin/env python3
"""Report observed CI elapsed time, runner time and retained artifact bytes through gh."""

import argparse
import datetime as dt
import json

from resolve_main_build import api, paginated


def timestamp(value):
    """Parse the API's UTC timestamps."""
    return dt.datetime.fromisoformat(value.replace("Z", "+00:00"))


def measure(repo, run_id):
    """Do not compare incomplete runs or pretend runner minutes are billed minutes."""
    base = f"repos/{repo}/actions/runs/{run_id}"
    run = api(base)
    jobs = paginated(f"{base}/jobs?filter=latest", "jobs")
    artifacts = paginated(f"{base}/artifacts", "artifacts")
    complete = [job for job in jobs if job.get("started_at") and job.get("completed_at")
                and job["conclusion"] != "skipped"]
    minutes = sum((timestamp(job["completed_at"]) - timestamp(job["started_at"])).total_seconds()
                  for job in complete) / 60
    elapsed = ((max(timestamp(job["completed_at"]) for job in complete)
                - timestamp(run["run_started_at"])).total_seconds() / 60) if complete else 0
    producers = [{"name": job["name"], "conclusion": job["conclusion"],
                  "minutes": round((timestamp(job["completed_at"]) - timestamp(job["started_at"])).total_seconds() / 60, 2)}
                 for job in complete if "Build & Archive" in job["name"]
                 or job["name"].startswith(("build-", "Build "))]
    return {"run_id": run_id, "sha": run["head_sha"], "event": run["event"],
            "status": run["status"], "conclusion": run["conclusion"],
            "elapsed_minutes": round(elapsed, 2), "summed_runner_minutes": round(minutes, 2),
            "artifact_count": len(artifacts),
            "artifact_bytes": sum(a["size_in_bytes"] for a in artifacts),
            "expired_artifact_count": sum(bool(a.get("expired")) for a in artifacts),
            "producers": producers,
            "note": "Wall-clock job minutes, without billing multipliers. Artifact sizes are stored bytes, not measured network transfers. Incomplete/cancelled runs cannot establish savings."}


def main():
    """Compare one or more explicitly selected runs without inferring equivalent coverage."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", type=int, nargs="+")
    parser.add_argument("--repo", default="illegalstudio/elephc")
    args = parser.parse_args()
    print(json.dumps([measure(args.repo, run) for run in args.run], indent=2))


if __name__ == "__main__":
    main()
