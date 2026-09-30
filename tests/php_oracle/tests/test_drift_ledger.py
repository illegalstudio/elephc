"""Offline tests for the classified Elephc/php-src stream drift ledger."""

from __future__ import annotations

import importlib.util
import json
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
MODULE_PATH = ROOT / "tools" / "php_oracle" / "export_elephc_drift.py"
SPEC = importlib.util.spec_from_file_location("export_elephc_drift", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
export_elephc_drift = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(export_elephc_drift)

LEDGER_PATH = (
    ROOT
    / "tests"
    / "php_oracle"
    / "drift"
    / "streams"
    / "php-8.5.6"
    / "macos-aarch64"
    / "streams-full.json"
)
MANIFEST_PATH = (
    ROOT
    / "tests"
    / "php_oracle"
    / "manifests"
    / "streams"
    / "php-8.5.6"
    / "macos-aarch64"
    / "streams-full.json"
)
SUPPORTED_TARGETS = ("macos-aarch64", "linux-aarch64", "linux-x86_64")


class DriftLedgerTests(unittest.TestCase):
    """Verify complete classification and source-verified blocker retention."""

    @classmethod
    def setUpClass(cls) -> None:
        """Load the canonical ledger and normative source-built PHP profile."""
        cls.content = LEDGER_PATH.read_bytes()
        cls.ledger = json.loads(cls.content)
        cls.manifest = json.loads(MANIFEST_PATH.read_bytes())

    def test_ledger_is_canonical_and_binds_the_php_surface(self) -> None:
        """The ledger compares the exact checked-in PHP surface, not copied values."""
        self.assertEqual(
            self.content,
            export_elephc_drift.canonical_bytes(self.ledger),
        )
        self.assertEqual(
            self.ledger["profile"]["php_surface_sha256"],
            export_elephc_drift.sha256_bytes(
                export_elephc_drift.canonical_bytes(self.manifest["surface"])
            ),
        )

    def test_every_observed_difference_is_classified(self) -> None:
        """Gate 0 cannot hide additions, removals, or signature/value drift."""
        self.assertEqual(self.ledger["gate"]["status"], "classified")
        self.assertEqual(self.ledger["gate"]["unclassified_drift"], 0)
        self.assertEqual(
            self.ledger["summary"]["total"],
            len(self.ledger["drifts"]),
        )
        self.assertTrue(
            all(
                drift["classification"] == "known-incompatibility"
                for drift in self.ledger["drifts"]
            )
        )

    def test_supported_targets_have_zero_unclassified_drift(self) -> None:
        """Every supported-target ledger binds its PHP surface and classifies all drift."""
        ledger_root = LEDGER_PATH.parents[1]
        manifest_root = MANIFEST_PATH.parents[1]
        for target in SUPPORTED_TARGETS:
            with self.subTest(target=target):
                ledger_path = ledger_root / target / "streams-full.json"
                manifest_path = manifest_root / target / "streams-full.json"
                ledger = json.loads(ledger_path.read_bytes())
                manifest = json.loads(manifest_path.read_bytes())
                self.assertEqual(
                    ledger_path.read_bytes(),
                    export_elephc_drift.canonical_bytes(ledger),
                )
                self.assertEqual(ledger["profile"]["target"], target)
                self.assertEqual(ledger["gate"]["unclassified_drift"], 0)
                self.assertEqual(ledger["summary"]["total"], len(ledger["drifts"]))
                self.assertEqual(
                    ledger["profile"]["php_surface_sha256"],
                    export_elephc_drift.sha256_bytes(
                        export_elephc_drift.canonical_bytes(manifest["surface"])
                    ),
                )

    def test_audited_constant_and_capability_blockers_remain_visible(self) -> None:
        """The ledger retains the PR's known constant and registry contradictions."""
        by_symbol = {
            (drift["category"], drift["symbol"]): drift
            for drift in self.ledger["drifts"]
        }
        for name in (
            "STREAM_CLIENT_PERSISTENT",
            "STREAM_CLIENT_ASYNC_CONNECT",
            "STREAM_CLIENT_CONNECT",
        ):
            self.assertIn(("constant-value", name), by_symbol)
        for name in (
            "STREAM_FROM_START",
            "STREAM_META_MODIFIED",
            "STREAM_OPTION_CHUNK_SIZE",
        ):
            self.assertIn(("extra-constant", name), by_symbol)
        for capability in ("wrappers", "transports", "filters"):
            self.assertIn(("configured-capability", capability), by_symbol)

    def test_missing_protocol_classes_are_explicit(self) -> None:
        """Directory, StreamBucket, and ZipArchive absence cannot pass as class parity."""
        missing = {
            drift["symbol"]
            for drift in self.ledger["drifts"]
            if drift["category"] == "missing-class"
        }
        self.assertEqual(missing, {"Directory", "StreamBucket", "ZipArchive"})

    def test_alias_and_complete_class_metadata_are_classified(self) -> None:
        """Alias targets and full method/property/constant shapes cannot disappear."""
        by_key = {
            (drift["category"], drift["symbol"]): drift
            for drift in self.ledger["drifts"]
        }
        self.assertEqual(
            by_key[("missing-function", "fputs")]["php"]["alias_of"],
            "fwrite",
        )
        spl_file = by_key[("class-surface", "SplFileObject")]
        self.assertEqual(
            spl_file["php"]["methods"]["getcurrentline"]["alias_of"],
            "SplFileObject::fgets",
        )
        self.assertIn("signature", spl_file["php"]["methods"]["fgetcsv"])
        self.assertIn("value", spl_file["php"]["constants"]["READ_CSV"])


if __name__ == "__main__":
    unittest.main()
