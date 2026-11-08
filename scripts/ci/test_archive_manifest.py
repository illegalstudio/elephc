#!/usr/bin/env python3
"""Validate a nextest bundle's source, producer and complete payload before extraction."""

import argparse
import json
import os
from pathlib import Path

from build_environment import snapshot, validate as validate_environment
from build_artifacts import checkout_identity, digest, identity


def describe(root, target, variant, sha, repository):
    """Keep test bundle identity independent of the consumer's runner path."""
    result = identity(sha, target, "debug", "", "tests", repository)
    result["source"] = checkout_identity(root)
    result["variant"] = variant
    return result


def write(directory, expected, run_id, attempt):
    """Record the archive and its matching standalone nextest executable."""
    files = {name: digest(directory / name) for name in
             ("nextest-archive.tar.zst", "cargo-nextest", "cargo-nextest.sha256")}
    manifest = dict(schema=1, identity=expected, run_id=str(run_id), attempt=str(attempt), files=files)
    (directory / "nextest-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")


def verify(directory, expected, run_id, attempt):
    """Reject archives from another SHA, attempt or variant, even with valid zstd frames."""
    manifest = json.loads((directory / "nextest-manifest.json").read_text())
    if manifest.get("schema") != 1 or manifest.get("identity") != expected:
        raise ValueError("nextest archive source or variant mismatch")
    if manifest.get("run_id") != str(run_id) or manifest.get("attempt") != str(attempt):
        raise ValueError("nextest archive producer mismatch")
    expected_files = {"nextest-archive.tar.zst", "cargo-nextest", "cargo-nextest.sha256"}
    if set(manifest.get("files", {})) != expected_files:
        raise ValueError("nextest archive has an incomplete payload")
    for name, checksum in manifest["files"].items():
        if digest(directory / name) != checksum:
            raise ValueError(f"nextest bundle checksum mismatch: {name}")


def main():
    """Use explicit producer attempts so partial reruns reuse their original build."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("write", "verify"))
    parser.add_argument("directory", type=Path)
    parser.add_argument("--target", required=True)
    parser.add_argument("--variant", choices=("workspace", "codegen"), required=True)
    parser.add_argument("--sha", default=os.environ.get("GITHUB_SHA"), required=False)
    parser.add_argument("--run-id", default=os.environ.get("GITHUB_RUN_ID"))
    parser.add_argument("--attempt", default=os.environ.get("GITHUB_RUN_ATTEMPT"))
    args = parser.parse_args()
    expected = describe(Path.cwd(), args.target, args.variant, args.sha, os.environ["GITHUB_REPOSITORY"])
    if args.command == "write":
        write(args.directory, expected, args.run_id, args.attempt)
        path = args.directory / "nextest-manifest.json"
        manifest = json.loads(path.read_text())
        manifest["environment"] = snapshot(args.target)
        path.write_text(json.dumps(manifest, indent=2) + "\n")
    else:
        verify(args.directory, expected, args.run_id, args.attempt)
        validate_environment(json.loads((args.directory / "nextest-manifest.json").read_text())["environment"], args.target)


if __name__ == "__main__":
    main()
