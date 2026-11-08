#!/usr/bin/env python3
"""Select verified queue compiler/bridge payloads after disposable test archives are gone."""

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

HOSTS = ("macos-aarch64", "linux-x86_64", "linux-aarch64")
GATES = ("Build & Test", "PDO core live suites (PostgreSQL 16 + MySQL 8.4)",
         "PDO optional-driver live suites")


def api(path):
    """Read GitHub through the authenticated official CLI, failing conservatively."""
    return json.loads(subprocess.check_output(["gh", "api", path], text=True))


def select(run, jobs, artifacts, repository, sha):
    """Whole-workflow completion is unnecessary once every required gate has passed."""
    if (run.get("head_sha") != sha or run.get("event") != "merge_group"
            or run.get("path") != ".github/workflows/ci.yml"
            or run.get("repository", {}).get("full_name") != repository
            or run.get("conclusion") in ("failure", "cancelled", "timed_out", "action_required")):
        return None
    # Check exact ordinary top-level gate names, never a similarly named nested job.
    for name in GATES:
        matches = [job for job in jobs if job.get("name") == name]
        if len(matches) != 1 or matches[0].get("conclusion") != "success":
            return None
    names = {artifact["name"] for artifact in artifacts
             if not artifact.get("expired", True) and artifact.get("size_in_bytes", 0) > 0
             and artifact.get("workflow_run", {}).get("head_sha") == sha}
    attempts = {}
    for target in HOSTS:
        prefix = f"build-{target}-debug-compiler-default-"
        candidates = sorted((int(name[len(prefix):]) for name in names
                             if name.startswith(prefix) and name[len(prefix):].isdigit()), reverse=True)
        for attempt in candidates:
            required = {f"build-{target}-debug-bridges-default-{attempt}"}
            if required <= names:
                attempts[target] = str(attempt)
                break
        else:
            return None
    # The exact top-level gate verdicts prove the tests passed. Their executables,
    # PDO variants and iOS link inputs are no longer needed to package this SHA.
    # Keep the original compiler/bridge pair for each producer attempt; consumers
    # still verify its complete source, environment and file-hash manifests.
    return {"run_id": str(run["id"]), "attempts": attempts}


def paginated(path, key):
    """Fetch all artifacts/jobs, whose counts exceed one API page on the sharded matrix."""
    result = []
    for page in range(1, 21):
        response = api(f"{path}{'&' if '?' in path else '?'}per_page=100&page={page}")
        result.extend(response[key])
        if len(result) >= response["total_count"]:
            return result
    raise ValueError(f"GitHub pagination exceeded the bounded 2000-item inventory: {path}")


def compatible_environment(repository, run_id, artifacts, expected):
    """Read a tiny explicit-run identity before skipping any main validation."""
    candidates = [a for a in artifacts if re.fullmatch(r"ci-environment-[1-9][0-9]*", a["name"])
                  and not a.get("expired", True)]
    if not candidates:
        return False
    artifact = max(candidates, key=lambda a: int(a["name"].rsplit("-", 1)[1]))
    with tempfile.TemporaryDirectory(prefix="elephc-environment-") as directory:
        subprocess.run(["gh", "run", "download", str(run_id), "--repo", repository,
                        "--name", artifact["name"], "--dir", directory], check=True)
        return (Path(directory) / "ci-image.txt").read_text().strip() == expected


def resolve(repository, sha):
    """Never reuse another PR, queue entry, repository or merely the latest successful run."""
    if not re.fullmatch(r"[0-9a-f]{40}", sha):
        raise ValueError("Expected a full source SHA")
    runs = api(f"repos/{repository}/actions/workflows/ci.yml/runs?head_sha={sha}&event=merge_group&per_page=10")
    for candidate in runs["workflow_runs"]:
        run_id = candidate["id"]
        run = api(f"repos/{repository}/actions/runs/{run_id}")
        jobs = paginated(f"repos/{repository}/actions/runs/{run_id}/jobs?filter=latest", "jobs")
        artifacts = paginated(f"repos/{repository}/actions/runs/{run_id}/artifacts", "artifacts")
        selected = select(run, jobs, artifacts, repository, sha)
        if selected:
            image = os.environ.get("ELEPHC_CI_IMAGE")
            if image and not compatible_environment(repository, run_id, artifacts, image):
                continue
            return selected
    return None


def main():
    """An unavailable or incompatible source always selects a fresh build and its tests."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--main-sha", help="A full main SHA already validated by the backfill entry point")
    args = parser.parse_args()
    result = None
    if args.main_sha or (os.environ.get("GITHUB_EVENT_NAME") == "push" and os.environ.get("GITHUB_REF") == "refs/heads/main"):
        try:
            result = resolve(os.environ["GITHUB_REPOSITORY"], args.main_sha or os.environ["GITHUB_SHA"])
        except (subprocess.CalledProcessError, ValueError, KeyError) as error:
            print(f"::warning::Cannot prove artifact reuse, selecting full CI: {error}", file=sys.stderr)
    with open(os.environ["GITHUB_OUTPUT"], "a") as output:
        output.write(f"run-id={result['run_id'] if result else ''}\n")
        output.write(f"attempts={json.dumps(result['attempts'] if result else {})}\n")
    print(f"Verified queue source: {result}" if result else "Build and validate this SHA normally")


if __name__ == "__main__":
    main()
