"""Renew Actions retention by copying verified package bytes without building or executing them."""

import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile
import zipfile

from artifact_retention_policy import PACKAGE
from build_artifacts import digest
from reconcile_development import gh


def unpack(archive, output, expected, repository):
    """Accept only the original tarball and checksum, verifying both container layers."""
    if not re.fullmatch(r"sha256:[0-9a-f]{64}", expected["digest"]):
        raise ValueError("Missing SHA-256 artifact digest")
    if "sha256:" + digest(archive) != expected["digest"]:
        raise ValueError("Downloaded artifact digest differs from the retained upload")
    name = f"elephc-dev-{expected['target']}-{expected['sha']}.tar.gz"
    with zipfile.ZipFile(archive) as bundle:
        entries = bundle.infolist()
        if len(entries) != 2 or {entry.filename for entry in entries} != {name, name + ".sha256"}:
            raise ValueError("Unexpected files in the development artifact")
        output.mkdir(parents=True, exist_ok=True)
        for entry in entries:
            # Copy explicit flat filenames, never use the archive's path or mode.
            with bundle.open(entry) as source, (output / entry.filename).open("wb") as destination:
                shutil.copyfileobj(source, destination)
    tarball = output / name
    if (output / (name + ".sha256")).read_text() != f"{digest(tarball)}  {name}\n":
        raise ValueError("Development tarball checksum mismatch")
    with tarfile.open(tarball, "r:gz") as bundle:
        entries = [entry for entry in bundle if entry.name == "build-manifest.json"]
        if len(entries) != 1 or not entries[0].isfile() or entries[0].size > 1024 * 1024:
            raise ValueError("Missing or invalid package provenance")
        manifest = json.load(bundle.extractfile(entries[0]))
    identity = dict(sha=expected["sha"], target=expected["target"], profile="debug")
    if manifest.get("schema") != 1 or any(manifest.get(k) != v for k, v in identity.items()):
        raise ValueError("Package source, target or profile mismatch")
    for kind in ("compiler", "bridges"):
        origin = manifest.get(kind, {}).get("identity", {})
        required = dict(identity, repository=repository, kind=kind)
        if any(origin.get(k) != v for k, v in required.items()):
            raise ValueError("Package compiler/bridge provenance mismatch")


def refresh(repository, expected, output):
    """Download an exact ledger ID, recheck metadata, then verify bytes without running them."""
    match = PACKAGE.fullmatch(expected["name"])
    if (not match or match.groups()[:2] != (expected["target"], expected["sha"])
            or not isinstance(expected["id"], int) or expected["id"] <= 0):
        raise ValueError("Invalid retained artifact identity")
    metadata = gh(repository, f"actions/artifacts/{expected['id']}")
    if (metadata.get("expired", True) or any(metadata.get(key) != expected[key]
                                             for key in ("id", "name", "digest"))):
        raise ValueError("Retained artifact changed or expired before renewal")
    with tempfile.TemporaryDirectory(prefix="elephc-artifact-renewal-") as directory:
        archive = Path(directory) / "artifact.zip"
        with archive.open("wb") as destination:
            subprocess.run(["gh", "api", f"repos/{repository}/actions/artifacts/{expected['id']}/zip"],
                           stdout=destination, check=True)
        unpack(archive, output, expected, repository)


if __name__ == "__main__":
    refresh(os.environ["GITHUB_REPOSITORY"], json.loads(os.environ["RETAINED_ARTIFACT"]),
            Path("target/development"))
