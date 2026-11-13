#!/usr/bin/env python3
"""Require all existing CI aggregate verdicts before selecting a release source."""

import json
import os
import re
import subprocess
import sys
from urllib.parse import quote

from resolve_main_build import GATES


def successful_latest(checks, name, sha):
    """A newer pending/failed rerun must not be hidden by an older completed success."""
    matches = [check for check in checks if check.get("name") == name
               and check.get("head_sha") == sha and check.get("app", {}).get("slug") == "github-actions"]
    if not matches:
        return False
    latest = max(matches, key=lambda check: check["id"])
    return latest.get("status") == "completed" and latest.get("conclusion") == "success"


def trusted_verdict_run(run, repository, sha):
    """PR-tier checks do not authorize publication, even for a reachable PR ancestor."""
    return (run.get("head_sha") == sha
            and run.get("repository", {}).get("full_name") == repository
            and run.get("path") in (".github/workflows/ci.yml", ".github/workflows/pdo-live.yml")
            and ((run.get("event") == "merge_group" and run.get("head_branch", "").startswith("gh-readonly-queue/main/"))
                 or (run.get("event") == "push" and run.get("head_branch") == "main"))
            and run.get("conclusion") not in ("failure", "cancelled", "timed_out", "action_required"))


def main():
    if len(sys.argv) != 2 or not re.fullmatch(r"[0-9a-f]{40}", sys.argv[1]):
        raise SystemExit("usage: verify_ci_verdicts.py <full source SHA>")
    sha = sys.argv[1]
    repository = os.environ["GITHUB_REPOSITORY"]
    verified_runs = {}
    for name in GATES:
        path = f"repos/{repository}/commits/{sha}/check-runs?check_name={quote(name)}&filter=latest&per_page=100"
        result = json.loads(subprocess.check_output(["gh", "api", path], text=True))
        if not successful_latest(result["check_runs"], name, sha):
            print(f"Missing successful latest {name} verdict for {sha}", file=sys.stderr)
            raise SystemExit(1)
        latest = max((check for check in result["check_runs"] if check.get("name") == name
                      and check.get("head_sha") == sha and check.get("app", {}).get("slug") == "github-actions"),
                     key=lambda check: check["id"])
        match = re.fullmatch(rf"https://github.com/{re.escape(repository)}/actions/runs/([0-9]+)/job/[0-9]+", latest.get("details_url", ""))
        if not match:
            raise SystemExit(f"Cannot identify the workflow run behind {name}")
        run_id = match[1]
        if run_id not in verified_runs:
            verified_runs[run_id] = json.loads(subprocess.check_output(
                ["gh", "api", f"repos/{repository}/actions/runs/{run_id}"], text=True))
        if not trusted_verdict_run(verified_runs[run_id], repository, sha):
            raise SystemExit(f"{name} does not prove a main or merge-group integration run")
    print(f"All three required CI gates passed for {sha}")


if __name__ == "__main__":
    main()
