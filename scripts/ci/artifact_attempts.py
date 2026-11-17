#!/usr/bin/env python3
"""Resolve each matrix payload's own attempt after partial workflow reruns."""

import argparse
import json
import os

from resolve_main_build import paginated


def select_attempts(artifacts, variants, family):
    """Never apply the last matrix job's attempt to successful siblings from an earlier run."""
    result = {}
    for variant in variants:
        prefix = (f"build-{variant}-debug-ios-default-" if family == "ios"
                  else f"build-linux-x86_64-debug-tests-pdo-{variant}-")
        attempts = [int(a["name"][len(prefix):]) for a in artifacts
                    if a["name"].startswith(prefix) and a["name"][len(prefix):].isdigit()
                    and not a.get("expired", True) and a.get("size_in_bytes", 0) > 0]
        if not attempts:
            raise ValueError(f"Missing complete matrix artifact: {variant}")
        result[variant] = str(max(attempts))
    return result


def main():
    """Expose a map, not an ambiguous scalar matrix output."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("family", choices=("ios", "pdo"))
    parser.add_argument("variants", type=json.loads)
    args = parser.parse_args()
    repo, run = os.environ["GITHUB_REPOSITORY"], os.environ["GITHUB_RUN_ID"]
    artifacts = paginated(f"repos/{repo}/actions/runs/{run}/artifacts", "artifacts")
    result = select_attempts(artifacts, args.variants, args.family)
    with open(os.environ["GITHUB_OUTPUT"], "a") as output:
        output.write(f"attempts={json.dumps(result)}\n")


if __name__ == "__main__":
    main()
