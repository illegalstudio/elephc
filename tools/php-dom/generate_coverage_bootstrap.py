#!/usr/bin/env python3
"""Generate an unmapped, authority-derived PHP DOM coverage bootstrap.

The final coverage gate accepts only reviewed Rust-test owners, passed PHPT
evidence, and independently attested target binaries.  This generator must not
invent any of those facts: it records the frozen source inventory and writes a
separate machine-readable list of the still-unmapped cells.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import tempfile
from pathlib import Path
from typing import Any


COMPONENTS = ("dom", "libxml", "simplexml")
TARGETS = ("macos-aarch64", "linux-aarch64", "linux-x86_64")
FROZEN_COUNTS = {"dom": 868, "libxml": 32, "simplexml": 156}
FROZEN_ROUTE_COUNT = 603


class BootstrapError(ValueError):
    """Describe invalid or escaping bootstrap authority input."""


def canonical_json(document: dict[str, Any]) -> bytes:
    """Encode one JSON object in the stable form used for artifact digests."""
    return (json.dumps(document, indent=2, sort_keys=True) + "\n").encode("utf-8")


def sha256_file(path: Path) -> str:
    """Return the SHA-256 digest of one authority file."""
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def repo_path(repo_root: Path, relative: object, subject: str) -> Path:
    """Resolve a regular repository-relative authority path without escapes."""
    if not isinstance(relative, str):
        raise BootstrapError(f"INPUT_INVALID:{subject}")
    raw = Path(relative)
    if raw.is_absolute() or ".." in raw.parts:
        raise BootstrapError(f"UNSAFE_PATH:{subject}")
    path = repo_root / raw
    if path.is_symlink() or not path.is_file():
        raise BootstrapError(f"AUTHORITY_MISSING:{subject}")
    resolved = path.resolve()
    try:
        resolved.relative_to(repo_root)
    except ValueError as error:
        raise BootstrapError(f"UNSAFE_PATH:{subject}") from error
    return resolved


def output_path(repo_root: Path, relative: object, subject: str) -> Path:
    """Resolve an output path while permitting a not-yet-created leaf file."""
    if not isinstance(relative, str):
        raise BootstrapError(f"INPUT_INVALID:{subject}")
    raw = Path(relative)
    if raw.is_absolute() or ".." in raw.parts:
        raise BootstrapError(f"UNSAFE_PATH:{subject}")
    path = repo_root / raw
    parent = path.parent.resolve()
    try:
        parent.relative_to(repo_root)
    except ValueError as error:
        raise BootstrapError(f"UNSAFE_PATH:{subject}") from error
    if path.is_symlink():
        raise BootstrapError(f"UNSAFE_PATH:{subject}")
    return path


def load_object(path: Path, subject: str) -> dict[str, Any]:
    """Load an authority object and reject non-object JSON documents."""
    document = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(document, dict):
        raise BootstrapError(f"INPUT_INVALID:{subject}")
    return document


def load_input(repo_root: Path, input_path: Path | str) -> tuple[dict[str, Any], dict[str, Path]]:
    """Load input and resolve all declared authority paths once."""
    source = load_object(repo_path(repo_root, input_path, "input"), "input")
    if source.get("schema") != 1:
        raise BootstrapError("INPUT_INVALID:schema")
    ledgers = source.get("ledgers")
    if not isinstance(ledgers, dict):
        raise BootstrapError("INPUT_INVALID:ledgers")
    authorities = {
        "source_lock": repo_path(repo_root, source.get("source_lock"), "source_lock"),
        "opcode_authority": repo_path(repo_root, source.get("opcode_authority"), "opcode_authority"),
    }
    for component in COMPONENTS:
        authorities[f"ledger:{component}"] = repo_path(
            repo_root, ledgers.get(component), f"ledger:{component}"
        )
    return source, authorities


def source_metadata(lock: dict[str, Any]) -> dict[str, str]:
    """Extract the only source provenance the coverage manifest may claim."""
    php = lock.get("php")
    if not isinstance(php, dict):
        raise BootstrapError("AUTHORITY_INVALID:source_lock")
    commit = php.get("commit")
    version = php.get("version")
    if not isinstance(commit, str) or not isinstance(version, str):
        raise BootstrapError("AUTHORITY_INVALID:source_lock")
    return {"php_commit": commit, "php_version": version}


def route_rows(opcodes: dict[str, Any]) -> list[dict[str, str]]:
    """Return the exact generated opcode-key set without assigning a test owner."""
    operations = opcodes.get("operations")
    if not isinstance(operations, list) or len(operations) != FROZEN_ROUTE_COUNT:
        raise BootstrapError("AUTHORITY_INVALID:opcode_authority")
    ids = [operation.get("key") for operation in operations if isinstance(operation, dict)]
    if any(not isinstance(route_id, str) or not route_id for route_id in ids) or len(set(ids)) != len(ids):
        raise BootstrapError("AUTHORITY_INVALID:opcode_authority")
    return [{"id": route_id} for route_id in ids]


def phpt_rows(ledgers: dict[str, dict[str, Any]]) -> tuple[list[dict[str, str]], dict[str, int], list[dict[str, Any]]]:
    """Copy every frozen PHPT source record while preserving its pending status."""
    phpts: list[dict[str, str]] = []
    counts: dict[str, int] = {}
    holes: list[dict[str, Any]] = []
    for component in COMPONENTS:
        ledger = ledgers[component]
        entries = ledger.get("entries")
        if (
            ledger.get("component") != component
            or ledger.get("closed") is not False
            or not isinstance(entries, list)
            or len(entries) != FROZEN_COUNTS[component]
        ):
            raise BootstrapError(f"AUTHORITY_INVALID:ledger:{component}")
        counts[component] = len(entries)
        for entry in entries:
            path = entry.get("path") if isinstance(entry, dict) else None
            digest = entry.get("sha256") if isinstance(entry, dict) else None
            status = entry.get("status") if isinstance(entry, dict) else None
            if not isinstance(path, str) or not isinstance(digest, str) or status != "pending":
                raise BootstrapError(f"AUTHORITY_INVALID:ledger:{component}")
            manifest_path = f"php-src/{path}"
            phpts.append({"path": manifest_path, "sha256": digest, "status": "pending"})
            holes.append(
                {
                    "path": manifest_path,
                    "component": component,
                    "status": "pending",
                    "reason": "Upstream ledger entry has no reviewed coverage evidence.",
                }
            )
    if len(phpts) != sum(FROZEN_COUNTS.values()) or len({row["path"] for row in phpts}) != len(phpts):
        raise BootstrapError("AUTHORITY_INVALID:ledgers")
    return phpts, counts, holes


def validate_ledger_lock(lock: dict[str, Any], ledgers: dict[str, dict[str, Any]]) -> None:
    """Bind each pending-ledger inventory and digest to the source lock."""
    locked_ledgers = lock.get("ledgers")
    if not isinstance(locked_ledgers, dict):
        raise BootstrapError("AUTHORITY_INVALID:source_lock")
    for component in COMPONENTS:
        locked = locked_ledgers.get(component)
        ledger = ledgers[component]
        if (
            not isinstance(locked, dict)
            or locked.get("phpt_count") != FROZEN_COUNTS[component]
            or ledger.get("phpt_count") != locked.get("phpt_count")
            or ledger.get("sorted_phpt_digest") != locked.get("sorted_phpt_digest")
        ):
            raise BootstrapError(f"AUTHORITY_INVALID:ledger:{component}")


def build_artifacts(source: dict[str, Any], authorities: dict[str, Path]) -> tuple[dict[str, Any], dict[str, Any]]:
    """Build the fail-closed manifest and its explicit no-owner gap ledger."""
    targets = source.get("supported_targets")
    if targets != list(TARGETS):
        raise BootstrapError("INPUT_INVALID:supported_targets")
    lock = load_object(authorities["source_lock"], "source_lock")
    opcodes = load_object(authorities["opcode_authority"], "opcode_authority")
    routes = route_rows(opcodes)
    ledgers = {
        component: load_object(authorities[f"ledger:{component}"], f"ledger:{component}")
        for component in COMPONENTS
    }
    validate_ledger_lock(lock, ledgers)
    phpts, components, phpt_holes = phpt_rows(ledgers)
    requirements = [{"id": row["id"]} for row in routes]
    manifest = {
        "schema": 1,
        "source": source_metadata(lock),
        "supported_targets": list(TARGETS),
        "inventory": {
            "requirements": len(requirements),
            "routes": len(routes),
            "families": 0,
            "phpts": len(phpts),
            "components": components,
        },
        "requirements": requirements,
        "routes": routes,
        "families": [],
        "rust_tests": [],
        "phpts": phpts,
        "reports": [],
    }
    manifest_sha256 = hashlib.sha256(canonical_json(manifest)).hexdigest()
    gaps = {
        "schema": 1,
        "manifest_sha256": manifest_sha256,
        "authority_sha256": {
            "source_lock": sha256_file(authorities["source_lock"]),
            "opcode_authority": sha256_file(authorities["opcode_authority"]),
            **{
                f"ledger:{component}": sha256_file(authorities[f"ledger:{component}"])
                for component in COMPONENTS
            },
        },
        "unmapped_requirements": [
            {"id": row["id"], "reason": "No reviewed Rust test owner."} for row in requirements
        ],
        "unmapped_routes": [
            {"id": row["id"], "reason": "No reviewed Rust test owner."} for row in routes
        ],
        "unmapped_families": [],
        "pending_phpts": phpt_holes,
        "missing_target_attestations": list(TARGETS),
    }
    return manifest, gaps


def write_json_atomically(path: Path, document: dict[str, Any]) -> None:
    """Durably replace one JSON artifact without exposing a partial document."""
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary_name = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    temporary = Path(temporary_name)
    try:
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(canonical_json(document))
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
    finally:
        if temporary.exists():
            temporary.unlink()


def parse_args() -> argparse.Namespace:
    """Parse the repository-scoped authority/bootstrap artifact locations."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", required=True, type=Path)
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--manifest", required=True, type=Path)
    parser.add_argument("--gaps", required=True, type=Path)
    return parser.parse_args()


def main() -> int:
    """Generate both artifacts or return a stable nonzero configuration error."""
    arguments = parse_args()
    try:
        repo_root = arguments.repo_root.resolve()
        source, authorities = load_input(repo_root, str(arguments.input))
        manifest, gaps = build_artifacts(source, authorities)
        write_json_atomically(output_path(repo_root, str(arguments.manifest), "manifest"), manifest)
        write_json_atomically(output_path(repo_root, str(arguments.gaps), "gaps"), gaps)
    except (OSError, BootstrapError, json.JSONDecodeError) as error:
        print(f"BOOTSTRAP_ERROR:{error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
