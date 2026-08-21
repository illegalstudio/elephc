#!/usr/bin/env python3
"""Regression tests for the authority-derived, intentionally incomplete DOM bootstrap."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
BOOTSTRAP = REPO_ROOT / "tools/php-dom/generate_coverage_bootstrap.py"
CHECKER = REPO_ROOT / "tools/php-dom/check_coverage.py"
INPUT = REPO_ROOT / "tests/php_dom/coverage/bootstrap-input.json"
EXPECTED = REPO_ROOT / "tests/php_dom/coverage/expected-fail-closed-diagnostics.txt"
EXPECTED_STRICT = REPO_ROOT / "tests/php_dom/coverage/bootstrap-strict.stderr"
COMMITTED_MANIFEST = REPO_ROOT / "tests/php_dom/coverage/bootstrap-manifest.json"
COMMITTED_GAPS = REPO_ROOT / "tests/php_dom/coverage/bootstrap-gaps.json"


class CoverageBootstrapTests(unittest.TestCase):
    """Pin that the bootstrap is mechanically complete yet has no fabricated evidence."""

    def test_bootstrap_derives_the_frozen_authorities_and_remains_fail_closed(self) -> None:
        """Require exact inventories and expected strict rejection diagnostics."""
        with tempfile.TemporaryDirectory(dir=REPO_ROOT / "tests/php_dom/coverage") as directory:
            temporary = Path(directory)
            manifest = temporary / "manifest.json"
            gaps = temporary / "gaps.json"
            generated = subprocess.run(
                [
                    sys.executable,
                    str(BOOTSTRAP),
                    "--repo-root",
                    str(REPO_ROOT),
                    "--input",
                    str(INPUT.relative_to(REPO_ROOT)),
                    "--manifest",
                    str(manifest.relative_to(REPO_ROOT)),
                    "--gaps",
                    str(gaps.relative_to(REPO_ROOT)),
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(generated.returncode, 0, generated.stderr)

            manifest_document = json.loads(manifest.read_text(encoding="utf-8"))
            gaps_document = json.loads(gaps.read_text(encoding="utf-8"))
            self.assertEqual(manifest.read_bytes(), COMMITTED_MANIFEST.read_bytes())
            self.assertEqual(gaps.read_bytes(), COMMITTED_GAPS.read_bytes())
            self.assertEqual(manifest_document["inventory"]["requirements"], 603)
            self.assertEqual(manifest_document["inventory"]["routes"], 603)
            self.assertEqual(manifest_document["inventory"]["phpts"], 1056)
            self.assertEqual(len(gaps_document["pending_phpts"]), 1056)
            self.assertEqual(len(gaps_document["unmapped_routes"]), 603)

            checked = subprocess.run(
                [
                    sys.executable,
                    str(CHECKER),
                    "--repo-root",
                    str(REPO_ROOT),
                    "--manifest",
                    str(manifest),
                    "--strict",
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertNotEqual(checked.returncode, 0)
            self.assertEqual(checked.stderr, EXPECTED_STRICT.read_text(encoding="utf-8"))
            expected = [
                line
                for line in EXPECTED.read_text(encoding="utf-8").splitlines()
                if line and not line.startswith("#")
            ]
            for diagnostic in expected:
                self.assertIn(diagnostic, checked.stderr)


if __name__ == "__main__":
    unittest.main()
