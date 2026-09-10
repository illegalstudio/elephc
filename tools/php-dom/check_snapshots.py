#!/usr/bin/env python3
"""Validate checked-in PHP DOM surface and PHPT ledger snapshots."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path


COMPONENTS = ("dom", "libxml", "simplexml")
ALLOWED_STATUSES = {"pending", "direct", "translated", "not-applicable"}
CLOSED_STATUSES = {"direct", "translated", "not-applicable"}


def sha256_file(path: Path) -> str:
    """Returns the SHA-256 digest of one file."""
    return hashlib.sha256(path.read_bytes()).hexdigest()


def parse_arguments() -> argparse.Namespace:
    """Parses snapshot-checker CLI arguments."""
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-root", type=Path)
    parser.add_argument("--require-closed", action="store_true")
    return parser.parse_args()


def git_query(source_root: Path, *arguments: str) -> str:
    """Runs a read-only Git query in an optional php-src checkout."""
    return subprocess.run(
        ["git", *arguments],
        cwd=source_root,
        check=True,
        text=True,
        capture_output=True,
    ).stdout.strip()


def validate_surface(surface_path: Path) -> None:
    """Checks the pinned version and high-level Reflection surface counts."""
    surface = json.loads(surface_path.read_text())
    if (
        surface["schema"] != 2
        or surface["php_version"] != "8.5.8"
        or surface["libxml_dotted_version"] != "2.15.3"
        or surface["libxml_version"] != 21503
    ):
        raise SystemExit("surface snapshot version mismatch")

    extensions = {entry["name"].lower(): entry for entry in surface["extensions"]}
    if set(extensions) != set(COMPONENTS):
        raise SystemExit(f"unexpected extension set: {sorted(extensions)}")

    dom = extensions["dom"]
    canonical = {entry["canonical_name"] for entry in dom["classes"]}
    direct_methods = sum(len(entry["methods"]) for entry in dom["classes"])
    exported_properties = sum(len(entry["properties"]) for entry in dom["classes"])
    canonical_properties = sum(
        len(entry["properties"])
        for entry in dom["classes"]
        if entry["exported_name"] == entry["canonical_name"]
    )
    class_constants = sum(len(entry["constants"]) for entry in dom["classes"])

    expected = {
        "exported_types": 51,
        "canonical_types": 50,
        "direct_methods": 313,
        "exported_properties": 185,
        "canonical_properties": 184,
        "class_constants": 16,
        "extension_constants": 61,
        "functions": 2,
    }
    actual = {
        "exported_types": len(dom["classes"]),
        "canonical_types": len(canonical),
        "direct_methods": direct_methods,
        "exported_properties": exported_properties,
        "canonical_properties": canonical_properties,
        "class_constants": class_constants,
        "extension_constants": len(dom["constants"]),
        "functions": len(dom["functions"]),
    }
    if actual != expected:
        raise SystemExit(f"DOM surface count mismatch: expected {expected}, got {actual}")

    properties = {
        (class_spec["exported_name"], property_spec["name"]): property_spec
        for extension in surface["extensions"]
        for class_spec in extension["classes"]
        for property_spec in class_spec["properties"]
    }
    nonwritable_count = sum(
        not property_spec["writable"] for property_spec in properties.values()
    )
    if nonwritable_count != 140:
        raise SystemExit(
            f"semantic property mutability mismatch: expected 140, got {nonwritable_count}"
        )
    expected_properties = {
        ("Dom\\Node", "nodeName"): (False, False, True),
        ("Dom\\Node", "nodeValue"): (True, False, True),
        ("LibXMLError", "message"): (True, False, False),
    }
    for key, expected_property in expected_properties.items():
        property_spec = properties.get(key)
        actual_property = (
            property_spec["writable"],
            property_spec["readonly"],
            property_spec["virtual"],
        )
        if actual_property != expected_property:
            raise SystemExit(
                f"{key[0]}::${key[1]} mutability mismatch: "
                f"expected {expected_property}, got {actual_property}"
            )


def validate_ledger(
    ledger_path: Path,
    component_lock: dict,
    require_closed: bool,
    source_root: Path | None,
) -> int:
    """Validates one ledger and returns its pending-entry count."""
    ledger = json.loads(ledger_path.read_text())
    entries = ledger["entries"]
    paths = [entry["path"] for entry in entries]
    if len(entries) != component_lock["phpt_count"] or len(paths) != len(set(paths)):
        raise SystemExit(f"{ledger_path}: count or duplicate-path failure")
    if paths != sorted(paths):
        raise SystemExit(f"{ledger_path}: entries are not sorted")
    if ledger["sorted_phpt_digest"] != component_lock["sorted_phpt_digest"]:
        raise SystemExit(f"{ledger_path}: aggregate digest mismatch")

    pending = 0
    for entry in entries:
        status = entry["status"]
        if status not in ALLOWED_STATUSES:
            raise SystemExit(f"{entry['path']}: invalid status {status}")
        if status == "pending":
            pending += 1
        if status == "translated" and not entry["fixture"]:
            raise SystemExit(f"{entry['path']}: translated entry lacks fixture")
        if status == "not-applicable" and not entry["reason"]:
            raise SystemExit(f"{entry['path']}: not-applicable entry lacks reason")
        if require_closed and status not in CLOSED_STATUSES:
            raise SystemExit(f"{entry['path']}: ledger is not closed")

        if source_root is not None:
            source_path = source_root / entry["path"]
            if sha256_file(source_path) != entry["sha256"]:
                raise SystemExit(f"{entry['path']}: source SHA-256 mismatch")

    if ledger["closed"] != (pending == 0):
        raise SystemExit(f"{ledger_path}: closed flag disagrees with pending count")
    return pending


def validate_source_root(source_root: Path, lock: dict) -> None:
    """Checks the optional php-src checkout commit, trees, and stubs."""
    source_root = source_root.resolve()
    if git_query(source_root, "rev-parse", "HEAD") != lock["php"]["commit"]:
        raise SystemExit("php-src commit mismatch")
    for relative_path, expected in lock["php"]["trees"].items():
        actual = git_query(source_root, "rev-parse", f"HEAD^{{tree}}:{relative_path}")
        if actual != expected:
            raise SystemExit(f"{relative_path}: source tree mismatch")
    for relative_path, metadata in lock["php"]["stubs"].items():
        if sha256_file(source_root / relative_path) != metadata["sha256"]:
            raise SystemExit(f"{relative_path}: stub digest mismatch")


def validate_opcode_manifest(repo_root: Path) -> None:
    """Regenerates opcode artifacts in check mode and validates their public range."""
    subprocess.run(
        [
            sys.executable,
            str(repo_root / "tools/php-dom/generate_opcodes.py"),
            "--check",
        ],
        cwd=repo_root,
        check=True,
    )
    manifest = json.loads(
        (repo_root / "tests/php_dom/surface/opcodes-php-8.5.8.json").read_text()
    )
    operations = manifest["operations"]
    opcodes = [operation["opcode"] for operation in operations]
    if (
        manifest["abi_version"] != 1
        or manifest["first_public_opcode"] != 0x1000
        or opcodes != list(range(0x1000, 0x1000 + len(operations)))
        or len({operation["key"] for operation in operations}) != len(operations)
    ):
        raise SystemExit("DOM opcode manifest range or uniqueness mismatch")


def main() -> int:
    """Validates all checked-in snapshots and reports implementation progress."""
    arguments = parse_arguments()
    repo_root = Path(__file__).resolve().parents[2]
    lock = json.loads((repo_root / "tools/php-dom/source-lock.json").read_text())
    surface_path = repo_root / "tests/php_dom/surface/php-8.5.8.json"
    validate_surface(surface_path)
    validate_opcode_manifest(repo_root)

    source_root = arguments.source_root.resolve() if arguments.source_root else None
    if source_root is not None:
        validate_source_root(source_root, lock)

    pending_total = 0
    for component in COMPONENTS:
        pending_total += validate_ledger(
            repo_root / f"tests/php_dom/upstream/{component}-php-8.5.8.json",
            lock["ledgers"][component],
            arguments.require_closed,
            source_root,
        )

    print(f"snapshot check passed; pending PHPT entries: {pending_total}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
