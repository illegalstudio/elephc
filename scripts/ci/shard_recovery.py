"""Keep transfer recovery separate from test success and required-check verdicts."""

import os
from pathlib import Path
import re
import subprocess
import sys

TARGETS = {"macos-aarch64", "linux-x86_64", "linux-aarch64"}
FAMILIES = {"non-codegen", "cli", "web", "codegen", "pdo", "curl", "eval"}


def validate(target, family, shard, count, source, run_id, attempt, image):
    """Require immutable source/producer inputs before downloading any test bytes."""
    if target not in TARGETS or family not in FAMILIES:
        raise ValueError("Unsupported shard platform or family")
    if not 1 <= int(shard) <= int(count) <= 64:
        raise ValueError("Invalid shard partition")
    if not re.fullmatch(r"[0-9a-f]{40}", source):
        raise ValueError("An exact source SHA is required")
    if any(not re.fullmatch(r"[1-9][0-9]*", value) for value in (run_id, attempt)):
        raise ValueError("An exact producer run and attempt are required")
    if target.startswith("linux-") and not re.fullmatch(r"ghcr\.io/illegalstudio/elephc-ci@sha256:[0-9a-f]{64}", image):
        raise ValueError("Linux recovery requires the coordinator's immutable CI image")
    head = subprocess.check_output(
        ["git", "-c", f"safe.directory={Path.cwd().resolve()}", "rev-parse", "HEAD"], text=True,
    ).strip()
    if head != source:
        raise ValueError("Shard checkout does not match its requested source SHA")


def prepare(archive, curl, family):
    """Require every input download before allowing the test procedure to run."""
    if archive == "retryable-transfer" and curl == "skipped":
        return "retryable-transfer"
    if archive != "downloaded":
        raise ValueError("Test archive was not accepted")
    if family == "curl":
        if curl == "retryable-transfer":
            return "retryable-transfer"
        if curl != "downloaded":
            raise ValueError("Curl native cache was not accepted")
    elif curl != "skipped":
        raise ValueError("Unexpected supplementary download")
    return "ready"


def attempt_status(prepared, tests):
    """A skipped test phase is valid only when a transfer recovery is required."""
    if prepared == "retryable-transfer" and tests == "skipped":
        return "retryable-transfer"
    if prepared == "ready" and tests == "success":
        return "passed"
    raise ValueError("Shard has no successful test execution")


def output(status):
    """Expose a bounded state string to the next workflow job."""
    with open(os.environ["GITHUB_OUTPUT"], "a") as stream:
        stream.write(f"status={status}\n")


def main():
    """Execute validation or the same state transitions exercised by focused tests."""
    action = sys.argv[1]
    if action == "validate":
        validate(*(os.environ[name] for name in ("TARGET", "FAMILY", "SHARD", "SHARDS",
                 "SOURCE_SHA", "ARTIFACT_RUN", "ARTIFACT_ATTEMPT", "ELEPHC_CI_IMAGE")))
    elif action == "prepare":
        output(prepare(os.environ["ARCHIVE_STATUS"], os.environ["CURL_STATUS"], os.environ["FAMILY"]))
    elif action == "attempt":
        output(attempt_status(os.environ["PREPARED"], os.environ["TESTS_OUTCOME"]))
    else:
        raise ValueError("Unknown shard recovery operation")


if __name__ == "__main__":
    main()
