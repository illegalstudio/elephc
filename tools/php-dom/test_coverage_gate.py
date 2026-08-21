#!/usr/bin/env python3
"""Contract tests for the final PHP DOM coverage manifest and strict verifier.

Expected CLI/API contract (intentionally written before the implementation):

* ``generate_coverage_manifest.py --repo-root ROOT --input INPUT --output OUTPUT``
  writes schema-1 JSON.  It scans the PHP source roots declared by ``INPUT`` and
  reports the exact ``inventory.requirements``, ``inventory.routes``, and
  ``inventory.phpts`` counts.  Its generated PHPT rows retain the SHA-256 of
  their byte-for-byte source.
* ``check_coverage.py --repo-root ROOT --manifest MANIFEST --strict`` returns
  zero only for a fully mapped DOM evidence manifest.  A rejected manifest
  returns one and emits stable ``CODE:subject`` diagnostics on stderr.  The
  verifier must not downgrade any strict failure to a warning.

The tiny fixture tree is a schema example, not an alternate final inventory.
It deliberately has two route families and one PHPT so individual bad rows are
easy to isolate.  The bootstrap test creates the full 603-route/1,056-PHPT
synthetic source inventory at runtime without committing thousands of files.
"""

from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from typing import Any


TOOLS_ROOT = Path(__file__).resolve().parent
FIXTURE_ROOT = TOOLS_ROOT / "testdata" / "coverage"
GENERATOR = TOOLS_ROOT / "generate_coverage_manifest.py"
CHECKER = TOOLS_ROOT / "check_coverage.py"
PHP_COMMIT = "26b97507444c4fbda072f57dda1820f7b7d5e467"
BUILD_COMMIT = "50347ae2eeb1b77e386c114ca00daab2c6c2e5d7"
TARGETS = ("macos-aarch64", "linux-aarch64", "linux-x86_64")
ALPHA_PHPT = "php-src/ext/dom/tests/alpha.phpt"


def sha256_file(path: Path) -> str:
    """Return the real SHA-256 digest of one fixture artifact."""
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


class CoverageFixture(unittest.TestCase):
    """Build an isolated, byte-addressable repo fixture for every contract test."""

    def setUp(self) -> None:
        """Copy the minimal deterministic fixture into a disposable repository."""
        self.temporary = tempfile.TemporaryDirectory()
        self.repo_root = Path(self.temporary.name) / "repo"
        shutil.copytree(FIXTURE_ROOT, self.repo_root)

    def tearDown(self) -> None:
        """Delete the isolated fixture repository after one test."""
        self.temporary.cleanup()

    def manifest(self) -> dict[str, Any]:
        """Return one complete schema-1 manifest that the future gate accepts."""
        phpt_sha = sha256_file(self.repo_root / ALPHA_PHPT)
        reports = []
        for target in TARGETS:
            binary_path = Path("target") / target / "elephc-dom"
            reports.append(
                {
                    "id": target,
                    "target": target,
                    "status": "complete",
                    "binary": {
                        "path": binary_path.as_posix(),
                        "sha256": sha256_file(self.repo_root / binary_path),
                        "build_commit": BUILD_COMMIT,
                    },
                    "phpts": [
                        {
                            "path": ALPHA_PHPT,
                            "sha256": phpt_sha,
                            "status": "passed",
                        }
                    ],
                }
            )
        return {
            "schema": 1,
            "source": {"php_commit": PHP_COMMIT, "php_version": "8.5.8"},
            "build": {"commit": BUILD_COMMIT},
            "supported_targets": list(TARGETS),
            "inventory": {
                "requirements": 2,
                "routes": 2,
                "families": 2,
                "phpts": 1,
            },
            "requirements": [
                {
                    "id": "REQ-DOCUMENT-CREATE",
                    "atomic": True,
                    "coverage": {
                        "rust_test": "dom_document_create_element",
                        "route": "DOMDocument.createElement",
                    },
                },
                {
                    "id": "REQ-NODE-NAME",
                    "atomic": True,
                    "coverage": {
                        "rust_test": "dom_node_name",
                        "route": "DOMNode.nodeName",
                    },
                },
            ],
            "routes": [
                {
                    "id": "DOMDocument.createElement",
                    "family": "document",
                    "dispatcher_anchor": "document.dispatch",
                    "coverage": {"rust_test": "dom_document_create_element"},
                },
                {
                    "id": "DOMNode.nodeName",
                    "family": "node",
                    "dispatcher_anchor": "node.dispatch",
                    "coverage": {"rust_test": "dom_node_name"},
                },
            ],
            "families": [
                {
                    "id": "document",
                    "dispatcher_anchor": "document.dispatch",
                    "coverage": {"rust_test": "dom_document_create_element"},
                },
                {
                    "id": "node",
                    "dispatcher_anchor": "node.dispatch",
                    "coverage": {"rust_test": "dom_node_name"},
                },
            ],
            "rust_tests": [
                {
                    "id": "dom_document_create_element",
                    "path": "tests/codegen/dom_contract.rs",
                    "function": "dom_document_create_element",
                    "registered_in": "tests/codegen/mod.rs",
                },
                {
                    "id": "dom_node_name",
                    "path": "tests/codegen/dom_contract.rs",
                    "function": "dom_node_name",
                    "registered_in": "tests/codegen/mod.rs",
                },
            ],
            "phpts": [
                {
                    "path": ALPHA_PHPT,
                    "sha256": phpt_sha,
                    "status": "passed",
                    "mapping": {
                        "requirement": "REQ-DOCUMENT-CREATE",
                        "route": "DOMDocument.createElement",
                        "family": "document",
                        "rust_test": "dom_document_create_element",
                    },
                }
            ],
            "reports": reports,
        }

    def write_manifest(self, document: dict[str, Any]) -> Path:
        """Write one canonical fixture manifest and return its absolute path."""
        path = self.repo_root / "coverage-manifest.json"
        path.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n")
        return path

    def run_generator(self, source: Path, output: Path) -> subprocess.CompletedProcess[str]:
        """Run the future generator with its public, repository-scoped CLI."""
        if not GENERATOR.is_file():
            self.fail(f"missing TDD implementation target: {GENERATOR.name}")
        return subprocess.run(
            [
                sys.executable,
                str(GENERATOR),
                "--repo-root",
                str(self.repo_root),
                "--input",
                str(source),
                "--output",
                str(output),
            ],
            text=True,
            capture_output=True,
            check=False,
        )

    def run_checker(self, document: dict[str, Any]) -> subprocess.CompletedProcess[str]:
        """Run the strict future verifier against one isolated manifest mutation."""
        if not CHECKER.is_file():
            self.fail(f"missing TDD implementation target: {CHECKER.name}")
        return subprocess.run(
            [
                sys.executable,
                str(CHECKER),
                "--repo-root",
                str(self.repo_root),
                "--manifest",
                str(self.write_manifest(document)),
                "--strict",
            ],
            text=True,
            capture_output=True,
            check=False,
        )

    def assert_gate_fails(self, document: dict[str, Any], diagnostic: str) -> None:
        """Require the strict verifier to reject one named integrity failure."""
        result = self.run_checker(document)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn(diagnostic, result.stderr)


class CoverageManifestContractTests(CoverageFixture):
    """Pin the complete coverage-gate schema and its non-bypassable failures."""

    def test_bootstrap_inventory_is_exactly_603_routes_and_1056_phpts(self) -> None:
        """Generate the frozen-size inventory from 603 routes and all PHP components."""
        source_root = self.repo_root / "php-src"
        (source_root / "ext/dom/tests/alpha.phpt").unlink()
        counts = {"dom": 868, "libxml": 32, "simplexml": 156}
        for component, count in counts.items():
            directory = source_root / "ext" / component / "tests"
            directory.mkdir(parents=True, exist_ok=True)
            for index in range(count):
                (directory / f"case-{index:04d}.phpt").write_bytes(
                    f"--TEST--\n{component}-{index}\n--FILE--\n<?php ?>\n--EXPECT--\n\n".encode()
                )
        source = self.repo_root / "coverage-input.json"
        source.write_text(
            json.dumps(
                {
                    "schema": 1,
                    "php_src_root": "php-src",
                    "requirements": [{"id": f"REQ-{index:03d}"} for index in range(603)],
                    "routes": [{"id": f"ROUTE-{index:03d}"} for index in range(603)],
                    "components": counts,
                },
                indent=2,
                sort_keys=True,
            )
            + "\n"
        )
        output = self.repo_root / "generated-manifest.json"
        result = self.run_generator(source, output)
        self.assertEqual(result.returncode, 0, result.stderr)
        generated = json.loads(output.read_text())
        self.assertEqual(generated["schema"], 1)
        self.assertEqual(generated["inventory"]["requirements"], 603)
        self.assertEqual(generated["inventory"]["routes"], 603)
        self.assertEqual(generated["inventory"]["phpts"], 1056)
        self.assertEqual(
            generated["inventory"]["components"],
            {"dom": 868, "libxml": 32, "simplexml": 156},
        )

    def test_generator_rejects_source_roots_that_escape_the_repository(self) -> None:
        """Refuse a symlinked php-src root whose resolved location is outside the repo."""
        external = self.repo_root.parent / "outside-php-src"
        (external / "ext/dom/tests").mkdir(parents=True)
        (external / "ext/dom/tests/outside.phpt").write_text("--TEST--\noutside\n")
        escaped = self.repo_root / "escaped-php-src"
        escaped.symlink_to(external, target_is_directory=True)
        source = self.repo_root / "escaped-input.json"
        source.write_text(
            json.dumps(
                {
                    "schema": 1,
                    "php_src_root": escaped.name,
                    "requirements": [],
                    "routes": [],
                    "components": {"dom": 1},
                }
            )
        )
        result = self.run_generator(source, self.repo_root / "escaped-output.json")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("UNSAFE_PATH:php_src_root", result.stderr)

    def test_generator_uses_atomic_output_replacement(self) -> None:
        """Require a temp-file replacement rather than a truncating direct output write."""
        implementation = GENERATOR.read_text(encoding="utf-8")
        self.assertIn("os.replace", implementation)
        self.assertNotIn("arguments.output.write_text", implementation)

    def test_strict_rejects_empty_or_nonfrozen_inventory_and_target_sets(self) -> None:
        """Pin the campaign's 603/1,056 component counts and all three target reports."""
        empty = {
            "schema": 1,
            "source": {"php_commit": PHP_COMMIT, "php_version": "8.5.8"},
            "build": {"commit": BUILD_COMMIT},
            "supported_targets": [],
            "inventory": {"requirements": 0, "routes": 0, "families": 0, "phpts": 0},
            "requirements": [],
            "routes": [],
            "families": [],
            "rust_tests": [],
            "phpts": [],
            "reports": [],
        }
        self.assert_gate_fails(empty, "FROZEN_INVENTORY_MISMATCH:requirements")
        self.assert_gate_fails(empty, "FROZEN_COMPONENT_COUNT_MISMATCH:dom")
        self.assert_gate_fails(empty, "SUPPORTED_TARGETS_MISMATCH")

    def test_strict_uses_locked_source_and_checked_in_ledgers(self) -> None:
        """Reject source claims that disagree with source-lock or lack all PHPT ledgers."""
        wrong_source = self.manifest()
        wrong_source["source"]["php_commit"] = "unlocked-commit"
        self.assert_gate_fails(wrong_source, "SOURCE_LOCK_MISMATCH:php_commit")
        self.assert_gate_fails(self.manifest(), "LEDGER_MISSING:dom")

    def test_strict_pins_the_entire_source_lock_provenance(self) -> None:
        """Reject a lock whose PHP archive digest changes despite an unchanged commit."""
        lock_path = self.repo_root / "tools/php-dom/source-lock.json"
        lock_path.parent.mkdir(parents=True)
        lock = json.loads((TOOLS_ROOT / "source-lock.json").read_text())
        lock["php"]["archive_sha256"] = "0" * 64
        lock_path.write_text(json.dumps(lock, indent=2, sort_keys=True) + "\n")
        self.assert_gate_fails(self.manifest(), "SOURCE_LOCK_INVALID:provenance")

    def test_strict_compares_the_exact_route_id_set_to_generated_opcodes(self) -> None:
        """Reject a two-row route set even when it can claim a matching row count."""
        opcode_path = self.repo_root / "tests/php_dom/surface/opcodes-php-8.5.8.json"
        opcode_path.parent.mkdir(parents=True)
        shutil.copy2(
            TOOLS_ROOT.parents[1] / "tests/php_dom/surface/opcodes-php-8.5.8.json",
            opcode_path,
        )
        self.assert_gate_fails(self.manifest(), "ROUTE_SET_MISMATCH")

    def test_strict_requires_closed_ledgers_without_pending_authority_entries(self) -> None:
        """Reject the checked-in upstream ledger itself until every PHPT is closed."""
        upstream = self.repo_root / "tests/php_dom/upstream"
        upstream.mkdir(parents=True)
        for component in ("dom", "libxml", "simplexml"):
            shutil.copy2(
                TOOLS_ROOT.parents[1] / "tests/php_dom/upstream" / f"{component}-php-8.5.8.json",
                upstream / f"{component}-php-8.5.8.json",
            )
        self.assert_gate_fails(self.manifest(), "LEDGER_NOT_CLOSED:dom")

    def test_strict_rejects_symlinked_source_lock_and_ledgers(self) -> None:
        """Treat authority symlinks as unsafe even when their target has valid JSON."""
        lock_path = self.repo_root / "tools/php-dom/source-lock.json"
        lock_path.parent.mkdir(parents=True)
        lock_path.symlink_to(TOOLS_ROOT / "source-lock.json")
        self.assert_gate_fails(self.manifest(), "UNSAFE_AUTHORITY:source-lock")

        ledger_path = self.repo_root / "tests/php_dom/upstream/dom-php-8.5.8.json"
        ledger_path.parent.mkdir(parents=True)
        ledger_path.symlink_to(
            TOOLS_ROOT.parents[1] / "tests/php_dom/upstream/dom-php-8.5.8.json"
        )
        self.assert_gate_fails(self.manifest(), "UNSAFE_AUTHORITY:ledger:dom")

    def test_strict_enforces_closed_ledger_status_evidence_semantics(self) -> None:
        """Reject invalid status values and status-specific fixture, reason, and observations."""
        upstream = self.repo_root / "tests/php_dom/upstream"
        upstream.mkdir(parents=True)
        ledgers = {}
        for component in ("dom", "libxml", "simplexml"):
            source = TOOLS_ROOT.parents[1] / "tests/php_dom/upstream" / f"{component}-php-8.5.8.json"
            ledger = json.loads(source.read_text())
            ledger["closed"] = True
            for entry in ledger["entries"]:
                entry["status"] = "direct"
                entry["fixture"] = None
                entry["reason"] = None
                entry["observations"] = []
            ledgers[component] = ledger

        ledgers["dom"]["entries"][0]["status"] = "unsupported"
        (upstream / "dom-php-8.5.8.json").write_text(json.dumps(ledgers["dom"], indent=2) + "\n")
        for component in ("libxml", "simplexml"):
            (upstream / f"{component}-php-8.5.8.json").write_text(json.dumps(ledgers[component], indent=2) + "\n")
        self.assert_gate_fails(self.manifest(), "LEDGER_STATUS_INVALID:dom:unsupported")

        translated = ledgers["dom"]
        translated["entries"][0]["status"] = "translated"
        translated["entries"][0]["fixture"] = None
        (upstream / "dom-php-8.5.8.json").write_text(json.dumps(translated, indent=2) + "\n")
        self.assert_gate_fails(self.manifest(), "LEDGER_TRANSLATED_FIXTURE_MISSING:dom")

        translated["entries"][0]["fixture"] = "tests/dom/translated.php"
        translated["entries"][0]["reason"] = None
        (upstream / "dom-php-8.5.8.json").write_text(json.dumps(translated, indent=2) + "\n")
        self.assert_gate_fails(self.manifest(), "LEDGER_TRANSLATED_REASON_MISSING:dom")

        translated["entries"][0]["reason"] = "Compiler-equivalent fixture."
        translated["entries"][0]["observations"] = None
        (upstream / "dom-php-8.5.8.json").write_text(json.dumps(translated, indent=2) + "\n")
        self.assert_gate_fails(self.manifest(), "LEDGER_OBSERVATIONS_INVALID:dom")

        translated["entries"][0]["observations"] = []
        translated["entries"][0]["status"] = "not-applicable"
        translated["entries"][0]["fixture"] = None
        translated["entries"][0]["reason"] = None
        (upstream / "dom-php-8.5.8.json").write_text(json.dumps(translated, indent=2) + "\n")
        self.assert_gate_fails(self.manifest(), "LEDGER_NOT_APPLICABLE_REASON_MISSING:dom")

    def test_strict_pins_opcode_authority_beyond_a_matching_route_set(self) -> None:
        """Reject a forged opcode authority that supplies the right 603 IDs but wrong provenance."""
        opcode_path = self.repo_root / "tests/php_dom/surface/opcodes-php-8.5.8.json"
        opcode_path.parent.mkdir(parents=True)
        operations = [{"key": f"route-{index:03d}"} for index in range(603)]
        opcode_path.write_text(
            json.dumps(
                {
                    "schema": 1,
                    "php_version": "8.5.8",
                    "manifest_sha256": "forged",
                    "surface_sha256": "forged",
                    "operations": operations,
                },
                indent=2,
                sort_keys=True,
            )
            + "\n"
        )
        document = self.manifest()
        document["routes"] = [
            {
                "id": operation["key"],
                "family": "document",
                "dispatcher_anchor": "document.dispatch",
                "coverage": {"rust_test": "dom_document_create_element"},
            }
            for operation in operations
        ]
        document["inventory"]["routes"] = 603
        self.assert_gate_fails(document, "OPCODE_AUTHORITY_INVALID:provenance")

    def test_strict_resolves_every_phpt_mapping_owner(self) -> None:
        """Reject PHPT mappings that name absent requirement, route, family, or Rust-test rows."""
        mutations = (
            ("requirement", "REQ-NOT-DECLARED", "UNKNOWN_REQUIREMENT:REQ-NOT-DECLARED"),
            ("route", "ROUTE-NOT-DECLARED", "UNKNOWN_ROUTE:ROUTE-NOT-DECLARED"),
            ("family", "FAMILY-NOT-DECLARED", "UNKNOWN_FAMILY:FAMILY-NOT-DECLARED"),
            ("rust_test", "test_not_declared", "UNKNOWN_RUST_TEST:test_not_declared"),
        )
        for field, value, diagnostic in mutations:
            with self.subTest(field=field):
                document = self.manifest()
                document["phpts"][0]["mapping"][field] = value
                self.assert_gate_fails(document, diagnostic)

    def test_strict_rejects_non_object_rows_in_every_collection(self) -> None:
        """Fail closed instead of silently dropping malformed collection rows."""
        collections = ("requirements", "routes", "families", "phpts", "rust_tests")
        for collection in collections:
            with self.subTest(collection=collection):
                document = self.manifest()
                document[collection][0] = "not-an-object"
                self.assert_gate_fails(document, f"SCHEMA_ROW_INVALID:{collection}:0")

        targets = self.manifest()
        targets["supported_targets"][0] = {"target": "not-a-string"}
        self.assert_gate_fails(targets, "SCHEMA_ROW_INVALID:targets:0")

    def test_strict_rejects_self_declared_build_provenance_and_escaped_paths(self) -> None:
        """Require an independent build attestation and reject binary paths outside the repo."""
        self.assert_gate_fails(self.manifest(), "BUILD_PROVENANCE_UNVERIFIED")

        escaped = self.repo_root.parent / "outside-elephc-dom"
        escaped.write_bytes(b"outside binary")
        document = self.manifest()
        document["reports"][0]["binary"]["path"] = "../outside-elephc-dom"
        document["reports"][0]["binary"]["sha256"] = sha256_file(escaped)
        self.assert_gate_fails(document, "UNSAFE_PATH:binary:macos-aarch64")

    def test_strict_rejects_each_unmapped_requirement_route_family_and_phpt(self) -> None:
        """Reject every coverage-cell type that is present but lacks a named owner."""
        mutations = (
            ("requirement", "UNMAPPED_REQUIREMENT:REQ-DOCUMENT-CREATE"),
            ("route", "UNMAPPED_ROUTE:DOMDocument.createElement"),
            ("family", "UNMAPPED_FAMILY:document"),
            ("phpt", f"UNMAPPED_PHPT:{ALPHA_PHPT}"),
        )
        for kind, diagnostic in mutations:
            with self.subTest(kind=kind):
                document = self.manifest()
                if kind == "requirement":
                    document["requirements"][0]["coverage"] = {}
                elif kind == "route":
                    document["routes"][0]["coverage"] = {}
                elif kind == "family":
                    document["families"][0]["coverage"] = {}
                else:
                    document["phpts"][0]["mapping"] = {}
                self.assert_gate_fails(document, diagnostic)

    def test_strict_rejects_missing_and_duplicate_route_rows(self) -> None:
        """Treat absent and duplicate route inventory rows as separate hard errors."""
        missing = self.manifest()
        missing["routes"] = [missing["routes"][0]]
        missing["inventory"]["routes"] = 1
        self.assert_gate_fails(missing, "ROUTE_MISSING:DOMNode.nodeName")

        duplicate = self.manifest()
        duplicate["routes"].append(dict(duplicate["routes"][0]))
        duplicate["inventory"]["routes"] = 3
        self.assert_gate_fails(duplicate, "ROUTE_DUPLICATE:DOMDocument.createElement")

    def test_strict_rejects_missing_duplicate_and_stale_phpt_rows(self) -> None:
        """Require one unique ledger row with a current source SHA for each PHPT."""
        missing = self.manifest()
        missing["phpts"] = []
        missing["inventory"]["phpts"] = 0
        self.assert_gate_fails(missing, f"PHPT_MISSING:{ALPHA_PHPT}")

        duplicate = self.manifest()
        duplicate["phpts"].append(dict(duplicate["phpts"][0]))
        duplicate["inventory"]["phpts"] = 2
        self.assert_gate_fails(duplicate, f"PHPT_DUPLICATE:{ALPHA_PHPT}")

        stale = self.manifest()
        (self.repo_root / ALPHA_PHPT).write_bytes(b"--TEST--\nstale\n")
        self.assert_gate_fails(stale, f"PHPT_SHA_MISMATCH:{ALPHA_PHPT}")

    def test_strict_rejects_pending_ledger_entries(self) -> None:
        """Forbid unresolved upstream work from being counted as coverage evidence."""
        document = self.manifest()
        document["phpts"][0]["status"] = "pending"
        self.assert_gate_fails(document, f"PENDING_PHPT:{ALPHA_PHPT}")

    def test_strict_rejects_oracle_skip_ledger_entries(self) -> None:
        """Forbid oracle skips even when a report otherwise looks complete."""
        document = self.manifest()
        document["phpts"][0]["status"] = "oracle_skip"
        self.assert_gate_fails(document, f"FORBIDDEN_PHPT_STATUS:{ALPHA_PHPT}:oracle_skip")

    def test_strict_rejects_partial_wrong_target_and_wrong_binary_provenance_reports(self) -> None:
        """Require complete reports for their declared target and exact build binary."""
        partial = self.manifest()
        partial["reports"][0]["status"] = "partial"
        self.assert_gate_fails(partial, "REPORT_PARTIAL:macos-aarch64")

        wrong_target = self.manifest()
        wrong_target["reports"][0]["target"] = "linux-aarch64"
        self.assert_gate_fails(wrong_target, "REPORT_TARGET_MISMATCH:macos-aarch64")

        wrong_binary = self.manifest()
        wrong_binary["reports"][1]["binary"]["build_commit"] = "wrong-build"
        self.assert_gate_fails(wrong_binary, "BINARY_PROVENANCE_MISMATCH:linux-aarch64")

    def test_strict_rejects_missing_renamed_and_unregistered_rust_tests(self) -> None:
        """Resolve every referenced test to a real registered Rust test function."""
        missing = self.manifest()
        missing["rust_tests"][0]["path"] = "tests/codegen/missing.rs"
        self.assert_gate_fails(missing, "RUST_TEST_MISSING:dom_document_create_element")

        renamed = self.manifest()
        path = self.repo_root / "tests/codegen/dom_contract.rs"
        path.write_text(path.read_text().replace("fn dom_node_name", "fn dom_node_name_renamed"))
        self.assert_gate_fails(renamed, "RUST_TEST_RENAMED:dom_node_name")

        unregistered = self.manifest()
        (self.repo_root / "tests/codegen/mod.rs").write_text("//! Detached fixture module.\n")
        self.assert_gate_fails(unregistered, "RUST_TEST_UNREGISTERED:dom_document_create_element")

    def test_strict_requires_rust_test_attributes_and_referenced_owners(self) -> None:
        """Reject a plain Rust function even when its name and module still match."""
        document = self.manifest()
        path = self.repo_root / "tests/codegen/dom_contract.rs"
        path.write_text(path.read_text().replace("#[test]\nfn dom_node_name", "fn dom_node_name"))
        self.assert_gate_fails(document, "RUST_TEST_NOT_TEST:dom_node_name")

        unregistered_owner = self.manifest()
        unregistered_owner["requirements"][0]["coverage"]["rust_test"] = "not_registered"
        self.assert_gate_fails(unregistered_owner, "RUST_TEST_OWNER_UNKNOWN:not_registered")

    def test_strict_rejects_deceptive_reuse_for_independent_atomic_cells(self) -> None:
        """Prevent one broad Rust test from falsely satisfying two atomic requirements."""
        document = self.manifest()
        document["requirements"][1]["coverage"]["rust_test"] = "dom_document_create_element"
        self.assert_gate_fails(
            document,
            "DECEPTIVE_TEST_REUSE:dom_document_create_element",
        )

    def test_strict_rejects_family_or_route_dispatcher_anchor_mismatch(self) -> None:
        """Bind semantic family and route rows to the actual native dispatcher anchors."""
        family = self.manifest()
        family["families"][0]["dispatcher_anchor"] = "document.missing"
        self.assert_gate_fails(family, "DISPATCHER_ANCHOR_MISMATCH:family:document")

        route = self.manifest()
        route["routes"][1]["dispatcher_anchor"] = "node.missing"
        self.assert_gate_fails(route, "DISPATCHER_ANCHOR_MISMATCH:route:DOMNode.nodeName")

    def test_strict_rejects_missing_supported_target_evidence(self) -> None:
        """Require a complete report for macOS ARM64 and both supported Linux targets."""
        document = self.manifest()
        document["reports"] = document["reports"][:-1]
        self.assert_gate_fails(document, "TARGET_EVIDENCE_MISSING:linux-x86_64")

    def test_strict_rejects_translated_fixture_without_original_mapping(self) -> None:
        """Require translated fixtures to name their locked upstream PHPT source mapping."""
        document = self.manifest()
        document["phpts"][0]["fixture_kind"] = "translated"
        self.assert_gate_fails(document, f"TRANSLATED_FIXTURE_UNMAPPED:{ALPHA_PHPT}")


if __name__ == "__main__":
    unittest.main()
