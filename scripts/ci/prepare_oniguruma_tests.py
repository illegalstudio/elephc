#!/usr/bin/env python3
"""Prepare the managed Oniguruma project and source/artifact cache for offline test shards."""

import argparse
import os
from pathlib import Path
import subprocess


def main():
    """Use production native commands to populate and verify the exact bundle included by nextest."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("compiler", type=Path)
    parser.add_argument("bundle", type=Path)
    parser.add_argument("--target", required=True)
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    compiler = args.compiler.resolve(strict=True)
    bundle = args.bundle.resolve()
    project = bundle / "project"
    project.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, ELEPHC_NATIVE_CACHE=str(bundle / "cache"))
    selection = ["--target", args.target, "--manifest-path", str(project / "elephc.toml")]
    subprocess.run([str(compiler), "native", "add", "oniguruma", *selection,
                    *(["--offline"] if args.offline else [])], env=env, check=True)
    subprocess.run([str(compiler), "native", "install", "--locked", "--offline", *selection],
                   env=env, check=True)
    assert (project / "elephc.lock").is_file(), "native command did not create the reviewed lock"
    assert any((bundle / "cache" / "sources").glob("*.tar.gz")), "offline shards need the verified source archive"
    print(f"Prepared offline Oniguruma test bundle: {bundle}")


if __name__ == "__main__":
    main()
