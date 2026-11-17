"""Preserve validated measurements in the CI ledger before deleting Actions artifacts."""

import base64
import json
from pathlib import Path
import re
import subprocess
import tempfile

from artifacts import ARTIFACT_NAME, download_artifact, read_artifact
from common import SUITES
from history import timestamp, validate_record

BRANCH = "ci/development-artifact-ledger"
DIRECTORY = "benchmark-results"
MAX_BYTES = 4 * 1024 * 1024


def api(repo, endpoint, method="GET", body=None):
    """Fail closed on API errors, including incomplete JSON responses."""
    command = ["gh", "api", f"repos/{repo}/{endpoint}", "--method", method]
    if body is not None:
        command += ["--input", "-"]
    result = subprocess.run(command, input=json.dumps(body) if body is not None else None,
                            capture_output=True, text=True, check=True, timeout=180)
    return json.loads(result.stdout) if result.stdout.strip() else None


def absent(error):
    """Only a confirmed missing object permits bootstrap."""
    return "HTTP 404" in (error.stderr or "")


def decode(value):
    """Accept small JSON blobs, never executable content or oversized payloads."""
    if value.get("encoding") != "base64" or value.get("size", 0) > MAX_BYTES:
        raise ValueError("Invalid benchmark archive encoding or size")
    content = base64.b64decode(value["content"])
    if len(content) > MAX_BYTES:
        raise ValueError("Benchmark archive exceeds the size limit")
    return json.loads(content)


def validate(data, identifier):
    """Bind every suite to the artifact's immutable GitHub identity."""
    if data.get("schema") != 1 or data.get("branch") != "main":
        raise ValueError("Invalid benchmark archive schema or branch")
    meta, records = data["artifact"], data["records"]
    if (type(meta.get("id")) is not int or meta["id"] != identifier
            or type(meta.get("run_id")) is not int or meta["run_id"] <= 0
            or not re.fullmatch(r"[0-9a-f]{40}", meta.get("sha", ""))):
        raise ValueError("Invalid benchmark archive identity")
    timestamp(meta["created_at"])
    if not records or not set(records).issubset(SUITES):
        raise ValueError("Invalid benchmark archive suites")
    for suite, record in records.items():
        validate_record(suite, record)
        if any(record.get(key) != meta[field] for key, field in (
                ("artifact_id", "id"), ("run_id", "run_id"),
                ("commit", "sha"), ("timestamp", "created_at"))):
            raise ValueError("Benchmark archive record does not match its source")
    return meta, records


def preserve(repo, run, artifact, request, apply=False):
    """Persist one trusted main result before the caller may remove its artifact."""
    if (artifact["name"] != ARTIFACT_NAME or run.get("head_branch") != "main"
            or run.get("event") != "push" or run.get("path") != ".github/workflows/ci.yml"):
        return
    source = artifact.get("workflow_run", {})
    if source.get("id") != run["id"] or source.get("head_sha") != run["head_sha"]:
        raise ValueError("Benchmark artifact does not match its inventoried CI run")
    meta = {"id": artifact["id"], "created_at": artifact["created_at"],
            "run_id": run["id"], "sha": run["head_sha"]}
    path = f"contents/{DIRECTORY}/{meta['id']}.json"
    try:
        value = request(f"{path}?ref={BRANCH}")
        if value.get("encoding") == "none":
            value = request(f"git/blobs/{value['sha']}")
        saved = decode(value)
    except subprocess.CalledProcessError as error:
        if not absent(error):
            raise
    else:
        saved_meta, _ = validate(saved, meta["id"])
        if saved_meta != meta:
            raise ValueError("Saved benchmark identity differs from the Actions artifact")
        return
    with tempfile.TemporaryDirectory(prefix="elephc-benchmark-archive-") as temporary:
        directory = Path(temporary) / "result"
        download_artifact(repo, meta, directory)
        records = read_artifact(directory, meta, "main")
    data = {"schema": 1, "branch": "main", "artifact": meta, "records": records}
    validate(data, meta["id"])
    content = (json.dumps(data, sort_keys=True, separators=(",", ":")) + "\n").encode()
    if len(content) > MAX_BYTES:
        raise ValueError("Benchmark archive exceeds the size limit")
    if apply:
        request(path, "PUT", {"branch": BRANCH,
                              "message": f"ci: preserve benchmark artifact {meta['id']}",
                              "content": base64.b64encode(content).decode()})


def list_archives(repo, branch):
    """Read one immutable tree snapshot without the Contents API's directory limit."""
    if branch != "main":
        return []
    try:
        root = api(repo, f"git/trees/{BRANCH}")
    except subprocess.CalledProcessError as error:
        if not absent(error):
            raise
        return []
    if root.get("truncated"):
        raise ValueError("Truncated benchmark ledger tree")
    directory = [item for item in root["tree"] if item["path"] == DIRECTORY]
    if not directory:
        return []
    if len(directory) != 1 or directory[0]["type"] != "tree":
        raise ValueError("Invalid benchmark archive directory")
    tree = api(repo, f"git/trees/{directory[0]['sha']}")
    if tree.get("truncated"):
        raise ValueError("Truncated benchmark archive inventory")
    result = []
    for item in tree["tree"]:
        match = re.fullmatch(r"([1-9][0-9]*)\.json", item["path"])
        if not match or item["type"] != "blob" or item["size"] > MAX_BYTES:
            raise ValueError("Invalid benchmark archive entry")
        result.append({"id": int(match[1]), "sha": item["sha"]})
    return sorted(result, key=lambda item: item["id"])


def read_archive(repo, entry):
    """Fetch a content-addressed record, even if the ledger branch advances."""
    data = decode(api(repo, f"git/blobs/{entry['sha']}"))
    return validate(data, entry["id"])
