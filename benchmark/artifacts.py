"""Read and validate benchmark Actions artifacts through the official GitHub CLI."""

import json
import subprocess
import zipfile
from pathlib import Path

from history import current_results, legacy_result, timestamp

ARTIFACT_NAME = "benchmark-results"


def gh(*arguments: str) -> str:
    result = subprocess.run(["gh", *arguments], capture_output=True, text=True, timeout=180)
    if result.returncode:
        raise RuntimeError(f"gh {' '.join(arguments)} failed:\n{result.stderr}")
    return result.stdout


def list_artifacts(repo: str, branch: str) -> list[dict]:
    """Filter by artifact name server-side instead of scanning every CI archive."""
    pages = json.loads(gh("api", f"repos/{repo}/actions/artifacts?name={ARTIFACT_NAME}&per_page=100",
                         "--paginate", "--slurp"))
    artifacts = []
    for page in pages:
        for artifact in page["artifacts"]:
            run = artifact["workflow_run"]
            if artifact["name"] != ARTIFACT_NAME or artifact["expired"] or run["head_branch"] != branch:
                continue
            if run["head_repository_id"] != run["repository_id"]:
                continue
            artifacts.append({"id": artifact["id"], "created_at": artifact["created_at"],
                              "run_id": run["id"], "sha": run["head_sha"]})
    return sorted(artifacts, key=lambda a: (timestamp(a["created_at"]), a["id"]))


def download_artifact(repo: str, meta: dict, directory: Path) -> None:
    """Fetch the selected artifact ID so reruns cannot substitute an older archive."""
    archive = directory.with_suffix(".zip")
    with archive.open("wb") as output:
        result = subprocess.run(["gh", "api", f"repos/{repo}/actions/artifacts/{meta['id']}/zip"],
                                stdout=output, stderr=subprocess.PIPE, text=True, timeout=180)
    if result.returncode:
        raise RuntimeError(f"artifact {meta['id']} download failed: {result.stderr}")
    if archive.stat().st_size > 16 * 1024 * 1024:
        raise ValueError("Benchmark ZIP exceeds the size limit")
    directory.mkdir()
    with zipfile.ZipFile(archive) as contents:
        for name in ("results.json", "benchmark-results.json", "magician-benchmark-results.json"):
            if name in contents.namelist():
                if contents.namelist().count(name) != 1 or contents.getinfo(name).file_size > 4 * 1024 * 1024:
                    raise ValueError("Invalid or oversized benchmark JSON member")
                (directory / name).write_bytes(contents.read(name))


def read_artifact(directory: Path, meta: dict, branch: str) -> dict[str, dict]:
    current = directory / "results.json"
    if current.exists():
        return current_results(json.loads(current.read_text()), meta, branch)
    records = {}
    for suite, filename in (("native", "benchmark-results.json"),
                            ("magician", "magician-benchmark-results.json")):
        path = directory / filename
        if path.exists():
            records[suite] = legacy_result(suite, json.loads(path.read_text()), meta)
    if not records:
        raise ValueError(f"artifact {meta['id']}: no supported result files")
    return records


