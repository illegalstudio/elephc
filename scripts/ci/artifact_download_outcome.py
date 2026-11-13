"""Defer only three positively classified transport failures to another runner."""

import json
import os


def classify(downloads, validations, transfers):
    """Reject unknown states and integrity failures when deciding runner recovery."""
    if len(downloads) != 3 or len(validations) != 3 or len(transfers) != 3:
        raise ValueError("Expected exactly three download attempts")
    allowed = {"success", "failure", "skipped"}
    if any(value not in allowed for value in downloads + validations):
        raise ValueError("Incomplete artifact attempt outcomes")
    active = [index for index, value in enumerate(downloads) if value != "skipped"]
    if not active or active != list(range(len(active))):
        raise ValueError("Artifact attempts must run in order")
    last = active[-1]
    if downloads[last] == "success" and validations[last] in {"success", "skipped"}:
        return "downloaded"
    if downloads == ["failure"] * 3 and validations == ["skipped"] * 3 and transfers == ["true"] * 3:
        return "retryable-transfer"
    return "failure"


def main():
    """Publish the result, failing normally unless transfer deferral was opted into."""
    status = classify(json.loads(os.environ["DOWNLOAD_OUTCOMES"]),
                      json.loads(os.environ["VALIDATION_OUTCOMES"]),
                      json.loads(os.environ["TRANSFER_FLAGS"]))
    with open(os.environ["GITHUB_OUTPUT"], "a") as output:
        output.write(f"status={status}\n")
    if status == "downloaded":
        return
    if status == "retryable-transfer" and os.environ.get("DEFER_TRANSFER_FAILURE") == "true":
        print("::warning::Artifact transfer attempts exhausted; the shard verdict requires fresh-runner recovery.")
        return
    raise SystemExit("Artifact download or integrity validation failed")


if __name__ == "__main__":
    main()
