#!/usr/bin/env python3
"""Fail closed on incomplete or stale PHP DOM coverage evidence manifests.

This is intentionally a gate rather than a progress reporter.  A manifest is
accepted only when every reviewed atomic cell has a concrete Rust test, every
upstream PHPT is byte-current and passed, and each supported target has binary
provenance for the same build commit.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from collections.abc import Iterable
from pathlib import Path
from typing import Any


ANCHOR_PATTERN = re.compile(r"coverage-anchor:\s*([^\s]+)")
RUST_TEST_PATTERN = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(")
MODULE_PATTERN = re.compile(r"\bmod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;")
COMPONENTS = ("dom", "libxml", "simplexml")
TARGETS = ("macos-aarch64", "linux-aarch64", "linux-x86_64")
LOCKED_PHP_COMMIT = "26b97507444c4fbda072f57dda1820f7b7d5e467"
LOCKED_PHP_VERSION = "8.5.8"
FROZEN_INVENTORY = {"requirements": 603, "routes": 603, "phpts": 1056}
FROZEN_COMPONENTS = {"dom": 868, "libxml": 32, "simplexml": 156}
LOCKED_LEDGER_DIGESTS = {
    "dom": "b8ad9a9366ddafa2442ae786e76bb653c0e9b8c840c8b0abab3dcc320b31d5f3",
    "libxml": "8ebbfea4d882c7f78e5541fce049910482e63f19000642734b51e7ad40084e66",
    "simplexml": "6f43e190d3931b41b627f0d5d09ad88a46b2ac35ab82e11d0db0df8f1b0e6d68",
}
LOCKED_SOURCE_LOCK_SHA256 = "9188198de4473c21c4e2107838da201ea763631ccf89aa0bb691bdd709d00ee3"
LOCKED_OPCODE_AUTHORITY_SHA256 = "7e81fa8b4ea80f22990085baaec19d9b0ff8d021fb50e9749c246af7bb3d26d4"
LOCKED_OPCODE_MANIFEST_SHA256 = "d471b44820908f1ddda4194d89341a4ce2fd53ef408ced1b1fb1e9576592e41d"
LOCKED_OPCODE_SURFACE_SHA256 = "d8744c340659eb66526f5370383cde2fa16e55a977383ed8a131426b7bfef5e9"
LEDGER_STATUSES = {"direct", "translated", "not-applicable"}


class UnsafePathError(ValueError):
    """Describe a manifest path which leaves the repository root after resolution."""


def repo_path(repo_root: Path, value: str, subject: str) -> Path:
    """Resolve one manifest-relative file path while rejecting traversal and links."""
    raw = Path(value)
    if raw.is_absolute() or ".." in raw.parts:
        raise UnsafePathError(f"UNSAFE_PATH:{subject}")
    resolved = (repo_root / raw).resolve()
    try:
        resolved.relative_to(repo_root)
    except ValueError as error:
        raise UnsafePathError(f"UNSAFE_PATH:{subject}") from error
    return resolved


def authority_path(repo_root: Path, relative: str, subject: str) -> Path | None:
    """Return a checked-in authority only when every component is a real in-tree file."""
    path = repo_root / relative
    current = repo_root
    for component in Path(relative).parts:
        current /= component
        if current.is_symlink():
            return None
    try:
        resolved = path.resolve(strict=True)
        resolved.relative_to(repo_root)
    except (OSError, ValueError):
        return None
    return resolved if resolved.is_file() else None


def sha256_file(path: Path) -> str:
    """Return the SHA-256 digest of one file without decoding its contents."""
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def sorted_phpt_digest(entries: list[dict[str, Any]]) -> str:
    """Recompute the locked digest from deterministic ledger path and SHA records."""
    lines = [f"{entry['sha256']}  {entry['path']}\n" for entry in entries]
    return hashlib.sha256("".join(lines).encode("utf-8")).hexdigest()


def validate_closed_ledger_entries(
    component: str,
    entries: list[dict[str, Any]],
    errors: list[str],
) -> bool:
    """Validate the closed-ledger evidence shape for every source PHPT row."""
    valid = True
    for entry in entries:
        status = entry.get("status")
        if status not in LEDGER_STATUSES:
            errors.append(f"LEDGER_STATUS_INVALID:{component}:{status}")
            valid = False
            continue
        observations = entry.get("observations")
        if not isinstance(observations, list):
            errors.append(f"LEDGER_OBSERVATIONS_INVALID:{component}")
            valid = False
        if status == "translated":
            if not isinstance(entry.get("fixture"), str) or not entry["fixture"]:
                errors.append(f"LEDGER_TRANSLATED_FIXTURE_MISSING:{component}")
                valid = False
            if not isinstance(entry.get("reason"), str) or not entry["reason"]:
                errors.append(f"LEDGER_TRANSLATED_REASON_MISSING:{component}")
                valid = False
        elif status == "not-applicable":
            if not isinstance(entry.get("reason"), str) or not entry["reason"]:
                errors.append(f"LEDGER_NOT_APPLICABLE_REASON_MISSING:{component}")
                valid = False
    return valid


def validate_authorities(repo_root: Path, document: dict[str, Any], errors: list[str]) -> dict[str, str]:
    """Validate pinned source metadata and return the authoritative PHPT SHA map."""
    source = document.get("source")
    if not isinstance(source, dict):
        errors.append("SCHEMA_ERROR:source")
    else:
        if source.get("php_commit") != LOCKED_PHP_COMMIT:
            errors.append("SOURCE_LOCK_MISMATCH:php_commit")
        if source.get("php_version") != LOCKED_PHP_VERSION:
            errors.append("SOURCE_LOCK_MISMATCH:php_version")

    lock_path = authority_path(repo_root, "tools/php-dom/source-lock.json", "source-lock")
    if lock_path is None:
        raw_lock_path = repo_root / "tools/php-dom/source-lock.json"
        errors.append("UNSAFE_AUTHORITY:source-lock" if raw_lock_path.exists() or raw_lock_path.is_symlink() else "SOURCE_LOCK_MISSING")
    else:
        try:
            if sha256_file(lock_path) != LOCKED_SOURCE_LOCK_SHA256:
                errors.append("SOURCE_LOCK_INVALID:provenance")
            lock = json.loads(lock_path.read_text(encoding="utf-8"))
            php = lock.get("php")
            ledgers = lock.get("ledgers")
            if not isinstance(php, dict) or php.get("commit") != LOCKED_PHP_COMMIT:
                errors.append("SOURCE_LOCK_INVALID:php_commit")
            if not isinstance(php, dict) or php.get("version") != LOCKED_PHP_VERSION:
                errors.append("SOURCE_LOCK_INVALID:php_version")
            if not isinstance(ledgers, dict):
                errors.append("SOURCE_LOCK_INVALID:ledgers")
            else:
                for component, expected_count in FROZEN_COMPONENTS.items():
                    metadata = ledgers.get(component)
                    if (
                        not isinstance(metadata, dict)
                        or metadata.get("phpt_count") != expected_count
                        or metadata.get("sorted_phpt_digest") != LOCKED_LEDGER_DIGESTS[component]
                    ):
                        errors.append(f"SOURCE_LOCK_INVALID:{component}")
        except (OSError, ValueError, json.JSONDecodeError):
            errors.append("SOURCE_LOCK_INVALID")

    expected: dict[str, str] = {}
    for component, expected_count in FROZEN_COMPONENTS.items():
        relative_ledger = f"tests/php_dom/upstream/{component}-php-8.5.8.json"
        ledger_path = authority_path(repo_root, relative_ledger, f"ledger:{component}")
        if ledger_path is None:
            raw_ledger_path = repo_root / relative_ledger
            errors.append(
                f"UNSAFE_AUTHORITY:ledger:{component}"
                if raw_ledger_path.exists() or raw_ledger_path.is_symlink()
                else f"LEDGER_MISSING:{component}"
            )
            continue
        try:
            ledger = json.loads(ledger_path.read_text(encoding="utf-8"))
            entries = ledger.get("entries")
            if (
                not isinstance(entries, list)
                or ledger.get("component") != component
                or ledger.get("phpt_count") != expected_count
                or len(entries) != expected_count
                or ledger.get("sorted_phpt_digest") != LOCKED_LEDGER_DIGESTS[component]
                or sorted_phpt_digest(entries) != LOCKED_LEDGER_DIGESTS[component]
            ):
                errors.append(f"LEDGER_INVALID:{component}")
                continue
            if ledger.get("closed") is not True:
                errors.append(f"LEDGER_NOT_CLOSED:{component}")
                continue
            if not all(isinstance(entry, dict) for entry in entries):
                errors.append(f"LEDGER_INVALID:{component}")
                continue
            if not validate_closed_ledger_entries(component, entries, errors):
                continue
            for entry in entries:
                path = entry.get("path") if isinstance(entry, dict) else None
                sha256 = entry.get("sha256") if isinstance(entry, dict) else None
                if not isinstance(path, str) or not isinstance(sha256, str) or path in expected:
                    errors.append(f"LEDGER_INVALID:{component}")
                    break
                expected[path] = sha256
        except (OSError, TypeError, ValueError, json.JSONDecodeError, KeyError):
            errors.append(f"LEDGER_INVALID:{component}")
    return expected


def rows(document: dict[str, Any], name: str, errors: list[str]) -> list[dict[str, Any]]:
    """Return object rows from one manifest collection or record a schema error."""
    value = document.get(name)
    if not isinstance(value, list):
        errors.append(f"SCHEMA_ERROR:{name}")
        return []
    object_rows: list[dict[str, Any]] = []
    for index, row in enumerate(value):
        if not isinstance(row, dict):
            errors.append(f"SCHEMA_ROW_INVALID:{name}:{index}")
            continue
        object_rows.append(row)
    return object_rows


def identifier(row: dict[str, Any], fallback: str = "") -> str:
    """Extract a human-readable row identifier for stable diagnostics."""
    value = row.get("id", row.get("path", fallback))
    return value if isinstance(value, str) else fallback


def coverage_test(row: dict[str, Any]) -> str | None:
    """Return the named Rust test owner from one coverage object, if present."""
    coverage = row.get("coverage")
    if not isinstance(coverage, dict):
        return None
    rust_test = coverage.get("rust_test")
    return rust_test if isinstance(rust_test, str) and rust_test else None


def duplicate_ids(rows_to_check: Iterable[dict[str, Any]]) -> set[str]:
    """Return all nonempty identifiers which occur more than once."""
    seen: set[str] = set()
    duplicates: set[str] = set()
    for row in rows_to_check:
        row_id = identifier(row)
        if row_id in seen:
            duplicates.add(row_id)
        else:
            seen.add(row_id)
    return duplicates


def source_phpts(repo_root: Path) -> set[str]:
    """Discover the authoritative DOM/libxml/SimpleXML PHPT paths in this tree."""
    paths: set[str] = set()
    for component in COMPONENTS:
        test_root = repo_root / "php-src" / "ext" / component / "tests"
        if test_root.is_dir():
            paths.update(path.relative_to(repo_root).as_posix() for path in test_root.rglob("*.phpt"))
    return paths


def dispatcher_anchors(repo_root: Path) -> set[str]:
    """Read declared native dispatcher anchors from the DOM bridge sources."""
    root = repo_root / "crates" / "elephc-dom" / "src"
    if not root.is_dir():
        return set()
    anchors: set[str] = set()
    for path in root.rglob("*.rs"):
        anchors.update(ANCHOR_PATTERN.findall(path.read_text(encoding="utf-8")))
    return anchors


def module_is_registered(repo_root: Path, test_path: str, registered_in: str) -> bool:
    """Check that a Rust test file's module name is declared by its registry file."""
    try:
        registry = repo_path(repo_root, registered_in, f"rust_registry:{registered_in}")
    except UnsafePathError:
        return False
    module_name = Path(test_path).stem
    if not registry.is_file():
        return False
    return module_name in MODULE_PATTERN.findall(registry.read_text(encoding="utf-8"))


def validate_coverage_rows(document: dict[str, Any], errors: list[str]) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    """Validate explicit coverage owners and return requirement and route rows."""
    requirements = rows(document, "requirements", errors)
    routes = rows(document, "routes", errors)
    families = rows(document, "families", errors)
    phpts = rows(document, "phpts", errors)
    for row in requirements:
        if coverage_test(row) is None:
            errors.append(f"UNMAPPED_REQUIREMENT:{identifier(row)}")
    for row in routes:
        if coverage_test(row) is None:
            errors.append(f"UNMAPPED_ROUTE:{identifier(row)}")
    for row in families:
        if coverage_test(row) is None:
            errors.append(f"UNMAPPED_FAMILY:{identifier(row)}")
    for row in phpts:
        mapping = row.get("mapping")
        if not isinstance(mapping, dict) or not all(
            isinstance(mapping.get(key), str) and mapping[key]
            for key in ("requirement", "route", "family", "rust_test")
        ):
            errors.append(f"UNMAPPED_PHPT:{identifier(row)}")
    return requirements, routes


def validate_phpt_mapping_owners(document: dict[str, Any], errors: list[str]) -> None:
    """Resolve every PHPT evidence edge to a declared requirement, route, family, and test."""
    owners = {
        "requirement": ({identifier(row) for row in rows(document, "requirements", errors)}, "UNKNOWN_REQUIREMENT"),
        "route": ({identifier(row) for row in rows(document, "routes", errors)}, "UNKNOWN_ROUTE"),
        "family": ({identifier(row) for row in rows(document, "families", errors)}, "UNKNOWN_FAMILY"),
        "rust_test": ({identifier(row) for row in rows(document, "rust_tests", errors)}, "UNKNOWN_RUST_TEST"),
    }
    for row in rows(document, "phpts", errors):
        mapping = row.get("mapping")
        if not isinstance(mapping, dict):
            continue
        for field, (known, diagnostic) in owners.items():
            value = mapping.get(field)
            if isinstance(value, str) and value and value not in known:
                errors.append(f"{diagnostic}:{value}")


def validate_route_inventory(requirements: list[dict[str, Any]], routes: list[dict[str, Any]], errors: list[str]) -> None:
    """Require one route row for each named requirement route and no duplicates."""
    route_ids = {identifier(row) for row in routes}
    for route_id in sorted(duplicate_ids(routes)):
        errors.append(f"ROUTE_DUPLICATE:{route_id}")
    for requirement in requirements:
        coverage = requirement.get("coverage")
        route_id = coverage.get("route") if isinstance(coverage, dict) else None
        if isinstance(route_id, str) and route_id and route_id not in route_ids:
            errors.append(f"ROUTE_MISSING:{route_id}")


def validate_route_authority(repo_root: Path, routes: list[dict[str, Any]], errors: list[str]) -> None:
    """Bind coverage route identifiers to the exact generated DOM opcode key set."""
    opcode_path = authority_path(
        repo_root,
        "tests/php_dom/surface/opcodes-php-8.5.8.json",
        "opcodes",
    )
    if opcode_path is None:
        errors.append("ROUTE_AUTHORITY_MISSING")
        return
    try:
        if sha256_file(opcode_path) != LOCKED_OPCODE_AUTHORITY_SHA256:
            errors.append("OPCODE_AUTHORITY_INVALID:provenance")
            return
        manifest = json.loads(opcode_path.read_text(encoding="utf-8"))
        operations = manifest.get("operations")
        if (
            manifest.get("schema") != 1
            or manifest.get("php_version") != LOCKED_PHP_VERSION
            or manifest.get("manifest_sha256") != LOCKED_OPCODE_MANIFEST_SHA256
            or manifest.get("surface_sha256") != LOCKED_OPCODE_SURFACE_SHA256
            or not isinstance(operations, list)
        ):
            raise ValueError("operations")
        expected = {operation.get("key") for operation in operations if isinstance(operation, dict)}
        actual = {identifier(route) for route in routes}
        if len(operations) != FROZEN_INVENTORY["routes"] or len(expected) != len(operations) or actual != expected:
            errors.append("ROUTE_SET_MISMATCH")
    except (OSError, TypeError, ValueError, json.JSONDecodeError):
        errors.append("ROUTE_AUTHORITY_INVALID")


def validate_inventory(document: dict[str, Any], errors: list[str]) -> None:
    """Require the frozen campaign totals as well as internal row-count agreement."""
    inventory = document.get("inventory")
    if not isinstance(inventory, dict):
        errors.append("SCHEMA_ERROR:inventory")
        return
    for collection in ("requirements", "routes", "families", "phpts"):
        expected = inventory.get(collection)
        actual = document.get(collection)
        if not isinstance(expected, int) or not isinstance(actual, list) or expected != len(actual):
            errors.append(f"INVENTORY_COUNT_MISMATCH:{collection}")
    for collection, expected_count in FROZEN_INVENTORY.items():
        if inventory.get(collection) != expected_count or len(document.get(collection, [])) != expected_count:
            errors.append(f"FROZEN_INVENTORY_MISMATCH:{collection}")
    components = inventory.get("components")
    if not isinstance(components, dict):
        for component in FROZEN_COMPONENTS:
            errors.append(f"FROZEN_COMPONENT_COUNT_MISMATCH:{component}")
    else:
        for component, expected_count in FROZEN_COMPONENTS.items():
            if components.get(component) != expected_count:
                errors.append(f"FROZEN_COMPONENT_COUNT_MISMATCH:{component}")


def validate_phpts(
    repo_root: Path,
    phpts: list[dict[str, Any]],
    ledger_phpts: dict[str, str],
    errors: list[str],
) -> None:
    """Require one current, passed row for every locked-ledger PHPT."""
    rows_by_path: dict[str, list[dict[str, Any]]] = {}
    for row in phpts:
        path = identifier(row)
        try:
            repo_path(repo_root, path, f"phpt:{path}")
        except UnsafePathError as error:
            errors.append(str(error))
            continue
        rows_by_path.setdefault(path, []).append(row)
    expected_paths = ledger_phpts or {path: "" for path in source_phpts(repo_root)}
    for path in sorted(expected_paths):
        matching = rows_by_path.get(path, [])
        if not matching:
            errors.append(f"PHPT_MISSING:{path}")
            continue
        if len(matching) > 1:
            errors.append(f"PHPT_DUPLICATE:{path}")
        row = matching[0]
        try:
            source_path = repo_path(repo_root, path, f"phpt:{path}")
        except UnsafePathError as error:
            errors.append(str(error))
            continue
        expected_sha = expected_paths[path]
        if (
            not source_path.is_file()
            or row.get("sha256") != sha256_file(source_path)
            or (expected_sha and row.get("sha256") != expected_sha)
        ):
            errors.append(f"PHPT_SHA_MISMATCH:{path}")
        status = row.get("status")
        if status == "pending":
            errors.append(f"PENDING_PHPT:{path}")
        elif status != "passed":
            errors.append(f"FORBIDDEN_PHPT_STATUS:{path}:{status}")
        if row.get("fixture_kind") == "translated":
            mapping = row.get("mapping")
            original = row.get("original_phpt")
            mapped = isinstance(mapping, dict) and isinstance(mapping.get("original_phpt"), str)
            if not (isinstance(original, str) and original) and not mapped:
                errors.append(f"TRANSLATED_FIXTURE_UNMAPPED:{path}")


def validate_rust_tests(repo_root: Path, document: dict[str, Any], errors: list[str]) -> None:
    """Resolve declared Rust test owners to a function and registered module."""
    for row in rows(document, "rust_tests", errors):
        test_id = identifier(row)
        path = row.get("path")
        function = row.get("function")
        registered_in = row.get("registered_in")
        try:
            source_path = repo_path(repo_root, path, f"rust_test:{test_id}") if isinstance(path, str) else None
        except UnsafePathError as error:
            errors.append(str(error))
            source_path = None
        if source_path is None or not source_path.is_file():
            errors.append(f"RUST_TEST_MISSING:{test_id}")
            continue
        source = source_path.read_text(encoding="utf-8")
        functions = set(RUST_TEST_PATTERN.findall(source))
        if not isinstance(function, str) or function not in functions:
            errors.append(f"RUST_TEST_RENAMED:{test_id}")
        elif re.search(rf"(?m)^\s*#\[test\]\s*\n\s*fn\s+{re.escape(function)}\s*\(", source) is None:
            errors.append(f"RUST_TEST_NOT_TEST:{test_id}")
        if not isinstance(registered_in, str) or not module_is_registered(repo_root, path, registered_in):
            errors.append(f"RUST_TEST_UNREGISTERED:{test_id}")


def validate_referenced_test_owners(document: dict[str, Any], errors: list[str]) -> None:
    """Ensure every coverage edge points to a declared, independently checked Rust test."""
    declared = {identifier(row) for row in rows(document, "rust_tests", errors)}
    owners: set[str] = set()
    for collection in ("requirements", "routes", "families"):
        owners.update(test for row in rows(document, collection, errors) if (test := coverage_test(row)))
    for row in rows(document, "phpts", errors):
        mapping = row.get("mapping")
        if isinstance(mapping, dict) and isinstance(mapping.get("rust_test"), str):
            owners.add(mapping["rust_test"])
    for owner in sorted(owners - declared):
        errors.append(f"RUST_TEST_OWNER_UNKNOWN:{owner}")


def validate_atomic_reuse(requirements: list[dict[str, Any]], errors: list[str]) -> None:
    """Forbid an atomic requirement test from satisfying an independent cell."""
    owners: dict[str, list[str]] = {}
    for row in requirements:
        if row.get("atomic") is True and (test := coverage_test(row)) is not None:
            owners.setdefault(test, []).append(identifier(row))
    for test, requirement_ids in sorted(owners.items()):
        if len(requirement_ids) > 1:
            errors.append(f"DECEPTIVE_TEST_REUSE:{test}")


def validate_anchors(repo_root: Path, document: dict[str, Any], errors: list[str]) -> None:
    """Ensure every family and route anchor names a live bridge dispatch anchor."""
    anchors = dispatcher_anchors(repo_root)
    for kind, collection in (("family", "families"), ("route", "routes")):
        for row in rows(document, collection, errors):
            if row.get("dispatcher_anchor") not in anchors:
                errors.append(f"DISPATCHER_ANCHOR_MISMATCH:{kind}:{identifier(row)}")


def validate_reports(repo_root: Path, document: dict[str, Any], errors: list[str]) -> None:
    """Require complete same-build binary evidence for every supported target."""
    targets = document.get("supported_targets")
    reports = rows(document, "reports", errors)
    if not isinstance(targets, list):
        errors.append("SCHEMA_ERROR:supported_targets")
        targets = []
    else:
        for index, target in enumerate(targets):
            if not isinstance(target, str):
                errors.append(f"SCHEMA_ROW_INVALID:targets:{index}")
    if tuple(targets) != TARGETS:
        errors.append("SUPPORTED_TARGETS_MISMATCH")
    by_id = {identifier(report): report for report in reports}
    build = document.get("build")
    build_commit = build.get("commit") if isinstance(build, dict) else None
    attestations = validate_build_attestation(repo_root, build, errors)
    for target in TARGETS:
        report = by_id.get(target)
        if report is None:
            errors.append(f"TARGET_EVIDENCE_MISSING:{target}")
            continue
        if report.get("status") != "complete":
            errors.append(f"REPORT_PARTIAL:{target}")
        if report.get("target") != target:
            errors.append(f"REPORT_TARGET_MISMATCH:{target}")
        binary = report.get("binary")
        path = binary.get("path") if isinstance(binary, dict) else None
        try:
            binary_path = repo_path(repo_root, path, f"binary:{target}") if isinstance(path, str) else None
        except UnsafePathError as error:
            errors.append(str(error))
            binary_path = None
        if (
            not isinstance(binary, dict)
            or binary_path is None
            or not binary_path.is_file()
            or binary.get("sha256") != sha256_file(binary_path)
            or binary.get("build_commit") != build_commit
        ):
            errors.append(f"BINARY_PROVENANCE_MISMATCH:{target}")
        elif attestations.get(target) != {"path": path, "sha256": binary.get("sha256"), "commit": build_commit}:
            errors.append(f"BUILD_PROVENANCE_MISMATCH:{target}")


def validate_build_attestation(
    repo_root: Path,
    build: Any,
    errors: list[str],
) -> dict[str, dict[str, str]]:
    """Load a separately hashed build attestation instead of trusting manifest claims."""
    if not isinstance(build, dict):
        errors.append("BUILD_PROVENANCE_UNVERIFIED")
        return {}
    evidence = build.get("evidence")
    if not isinstance(evidence, dict):
        errors.append("BUILD_PROVENANCE_UNVERIFIED")
        return {}
    path = evidence.get("path")
    sha256 = evidence.get("sha256")
    try:
        evidence_path = repo_path(repo_root, path, "build_evidence") if isinstance(path, str) else None
    except UnsafePathError as error:
        errors.append(str(error))
        evidence_path = None
    if not isinstance(sha256, str) or evidence_path is None or not evidence_path.is_file():
        errors.append("BUILD_PROVENANCE_UNVERIFIED")
        return {}
    if sha256_file(evidence_path) != sha256:
        errors.append("BUILD_PROVENANCE_UNVERIFIED")
        return {}
    try:
        evidence_document = json.loads(evidence_path.read_text(encoding="utf-8"))
        entries = evidence_document.get("binaries")
        if evidence_document.get("commit") != build.get("commit") or not isinstance(entries, list):
            raise ValueError("attestation shape")
        attested: dict[str, dict[str, str]] = {}
        for entry in entries:
            if not isinstance(entry, dict):
                raise ValueError("attestation entry")
            target = entry.get("target")
            binary_path = entry.get("path")
            digest = entry.get("sha256")
            if not all(isinstance(value, str) and value for value in (target, binary_path, digest)):
                raise ValueError("attestation field")
            attested[target] = {"path": binary_path, "sha256": digest, "commit": build["commit"]}
        return attested
    except (OSError, ValueError, json.JSONDecodeError, KeyError):
        errors.append("BUILD_PROVENANCE_UNVERIFIED")
        return {}


def validate(repo_root: Path, document: dict[str, Any]) -> list[str]:
    """Return deterministic strict-gate diagnostics for the supplied manifest."""
    errors: list[str] = []
    if document.get("schema") != 1:
        return ["SCHEMA_ERROR:schema"]
    ledger_phpts = validate_authorities(repo_root, document, errors)
    requirements, routes = validate_coverage_rows(document, errors)
    validate_phpt_mapping_owners(document, errors)
    validate_inventory(document, errors)
    validate_route_inventory(requirements, routes, errors)
    validate_route_authority(repo_root, routes, errors)
    validate_phpts(repo_root, rows(document, "phpts", errors), ledger_phpts, errors)
    validate_rust_tests(repo_root, document, errors)
    validate_referenced_test_owners(document, errors)
    validate_atomic_reuse(requirements, errors)
    validate_anchors(repo_root, document, errors)
    validate_reports(repo_root, document, errors)
    return sorted(set(errors))


def parse_args() -> argparse.Namespace:
    """Parse the repository-scoped strict coverage-check command-line interface."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", required=True, type=Path)
    parser.add_argument("--manifest", required=True, type=Path)
    parser.add_argument("--strict", action="store_true")
    return parser.parse_args()


def main() -> int:
    """Run the gate and print only stable machine-readable diagnostics on failure."""
    arguments = parse_args()
    if not arguments.strict:
        print("STRICT_REQUIRED:--strict", file=sys.stderr)
        return 2
    try:
        document = json.loads(arguments.manifest.read_text(encoding="utf-8"))
        if not isinstance(document, dict):
            raise ValueError("manifest must be a JSON object")
        errors = validate(arguments.repo_root.resolve(), document)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"SCHEMA_ERROR:{error}", file=sys.stderr)
        return 1
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
