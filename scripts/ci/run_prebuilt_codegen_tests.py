#!/usr/bin/env python3
"""Run an archived codegen suite in one libtest process, preserving its memory caches."""

import os
from pathlib import Path
import re
import subprocess
import sys


def run(arguments):
    """Keep original libtest filters, ignored selection and thread counts without Cargo."""
    if not arguments:
        raise ValueError("Expected a libtest filter and optional libtest arguments")
    root = Path(os.environ["ELEPHC_CI_TEST_ROOT"]).resolve()
    # The codegen suite is six binaries split by domain, so a libtest filter runs in
    # whichever of them lists a matching test: one process per selected domain, and a
    # filter that matches nothing still fails instead of reporting a green run.
    binaries = sorted(path for path in (root / "target/debug/deps").glob("codegen_tests_*-*")
                      if path.is_file() and os.access(path, os.X_OK))
    if not binaries:
        raise ValueError("Expected archived codegen test binaries, found none")
    environment = dict(os.environ, ELEPHC_TEST_PREBUILT_BRIDGES="1",
                       CARGO_TARGET_DIR=str(root / "target"), CARGO_MANIFEST_DIR=os.getcwd(),
                       CARGO_BIN_EXE_elephc=str(root / "target/debug/elephc"))
    environment.setdefault("RUST_MIN_STACK", "33554432")
    commands = []
    total = 0
    for binary in binaries:
        command = [str(binary), *arguments]
        selected = set(re.findall(r"^\S+: test$", subprocess.check_output(
            [*command, "--list"], text=True, env=environment), re.MULTILINE))
        if "--ignored" not in arguments and "--include-ignored" not in arguments:
            ignored = subprocess.check_output([*command, "--list", "--ignored"], text=True, env=environment)
            selected.difference_update(re.findall(r"^\S+: test$", ignored, re.MULTILINE))
        if selected:
            commands.append(command)
            total += len(selected)
    if not commands:
        raise ValueError("The archived libtest filter selected no tests")
    print(f"Running {total} archived codegen tests in one libtest process per domain", flush=True)
    # The caller's existing job deadline and fixture watchdogs remain authoritative,
    # as for the former cargo-test invocation. Do not add process-per-test retries.
    failure = 0
    for command in commands:
        status = subprocess.call(command, env=environment)
        failure = failure or status
    return failure


if __name__ == "__main__":
    try:
        sys.exit(run(sys.argv[1:]))
    except (KeyError, ValueError, subprocess.CalledProcessError) as error:
        print(f"::error::{error}", file=sys.stderr)
        sys.exit(1)
