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


def main():
    if len(sys.argv) != 2 or not re.fullmatch(r"[0-9a-f]{40}", sys.argv[1]):
        raise SystemExit("usage: verify_ci_verdicts.py <full source SHA>")
    sha = sys.argv[1]
    repository = os.environ["GITHUB_REPOSITORY"]
    for name in GATES:
        path = f"repos/{repository}/commits/{sha}/check-runs?check_name={quote(name)}&filter=latest&per_page=100"
        result = json.loads(subprocess.check_output(["gh", "api", path], text=True))
        if not successful_latest(result["check_runs"], name, sha):
            print(f"Missing successful latest {name} verdict for {sha}", file=sys.stderr)
            raise SystemExit(1)
    print(f"All three required CI gates passed for {sha}")


if __name__ == "__main__":
    main()
