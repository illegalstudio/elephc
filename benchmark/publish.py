#!/usr/bin/env python3
"""Publish nightly generated outputs from an isolated checkout of current main."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path

from common import REPO, SUITES, atomic_json
from history import load_history, merge
from report import generate

STATS = ("clones.json", "clones-badge.svg", "star-history-light.svg", "star-history-dark.svg")


def git(cwd: Path, *arguments: str, check: bool = True) -> subprocess.CompletedProcess:
    """Keep Git authentication configuration out of diagnostic output."""
    result = subprocess.run(["git", *arguments], cwd=cwd, capture_output=True, text=True, timeout=180)
    if check and result.returncode:
        raise RuntimeError(f"Git operation failed ({arguments[0]}): {result.stderr}")
    return result


def publish(source: Path, attempts: int = 3) -> bool:
    """Re-merge history against fresh main on every normal, non-forced push attempt."""
    incoming = {suite: load_history(source / "benchmark" / "data" / f"{suite}.json", suite) for suite in SUITES}
    manifest_path = source / "benchmark" / "data" / "imports.json"
    incoming_imports = json.loads(manifest_path.read_text())
    remote = git(source, "remote", "get-url", "origin").stdout.strip()
    identity = {key: git(source, "config", "--get", key).stdout.strip() for key in ("user.name", "user.email")}
    # Current actions/checkout keeps credentials in an included config file.
    # Resolve includes in the source checkout and copy only its authentication headers.
    auth = git(source, "config", "--local", "--includes", "--get-regexp", r"^http\..*\.extraheader$", check=False)
    with tempfile.TemporaryDirectory(prefix="elephc-benchmark-publish-") as temporary:
        work = Path(temporary)
        git(work, "init", "--quiet")
        git(work, "remote", "add", "origin", remote)
        for key, value in identity.items():
            git(work, "config", "--local", key, value)
        for line in auth.stdout.splitlines():
            key, value = line.split(" ", 1)
            git(work, "config", "--local", "--add", key, value)
        for attempt in range(1, attempts + 1):
            git(work, "fetch", "--quiet", "--depth=1", "origin", "main")
            git(work, "checkout", "--quiet", "--detach", "FETCH_HEAD")
            data_dir = work / "benchmark" / "data"
            for suite, history in incoming.items():
                current = load_history(data_dir / f"{suite}.json", suite)
                combined = merge(current, history["series"])
                # Preserve provenance from migrated history on the first publication.
                if "legacy_source" in history:
                    combined.setdefault("legacy_source", history["legacy_source"])
                atomic_json(data_dir / f"{suite}.json", combined)
            current_imports = data_dir / "imports.json"
            imports = json.loads(current_imports.read_text()) if current_imports.exists() else {}
            atomic_json(current_imports, {**imports, **incoming_imports})
            generate(data_dir, work / "benchmark" / "assets")
            for filename in STATS:
                path = source / ".github" / "traffic" / filename
                if path.exists():
                    destination = work / ".github" / "traffic" / filename
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(path, destination)
            git(work, "add", "--", "benchmark/data", "benchmark/assets", ".github/traffic")
            if git(work, "diff", "--cached", "--quiet", check=False).returncode == 0:
                print("Generated repository statistics are already current.")
                return False
            git(work, "commit", "--quiet", "-m", "chore: update repository stats and benchmarks [skip ci]")
            push = git(work, "push", "origin", "HEAD:refs/heads/main", check=False)
            if push.returncode == 0:
                print("Published refreshed repository statistics and benchmark reports.")
                return True
            print(f"Publication attempt {attempt} did not push; retrying against current main.", flush=True)
    raise RuntimeError("could not publish generated statistics after refreshing main; no force push was attempted")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-dir", type=Path, default=REPO)
    args = parser.parse_args()
    if os.environ.get("GITHUB_ACTIONS") != "true":
        raise SystemExit("publish.py is a CI-only publisher; commit local refreshes through the normal PR workflow")
    publish(args.source_dir)


if __name__ == "__main__":
    main()
