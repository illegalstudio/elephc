"""Offline tests for binary-safe PHP stream oracle artifacts."""

from __future__ import annotations

import base64
import hashlib
import importlib.util
import json
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
MODULE_PATH = ROOT / "tools" / "php_oracle" / "run_case.py"
SPEC = importlib.util.spec_from_file_location("run_case", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
run_case = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(run_case)

PROFILE_PATH = (
    ROOT
    / "tests"
    / "php_oracle"
    / "manifests"
    / "streams"
    / "php-8.5.6"
    / "macos-aarch64"
    / "homebrew-no-ini.json"
)
ORACLE_DIR = (
    ROOT
    / "tests"
    / "php_oracle"
    / "oracles"
    / "streams"
    / "php-8.5.6"
    / "macos-aarch64"
    / "homebrew-no-ini"
)
SOURCE_PROFILE_PATH = PROFILE_PATH.with_name("streams-full.json")
SOURCE_ORACLE_DIR = ORACLE_DIR.with_name("streams-full")
SUPPORTED_TARGETS = ("macos-aarch64", "linux-aarch64", "linux-x86_64")


def decoded(record: dict[str, object]) -> bytes:
    """Decode and verify one binary byte record."""
    content = base64.b64decode(str(record["base64"]), validate=True)
    assert len(content) == record["length"]
    assert hashlib.sha256(content).hexdigest() == record["sha256"]
    return content


class OracleArtifactTests(unittest.TestCase):
    """Verify canonical encoding and raw/instrumented evidence separation."""

    def load(self, case_id: str) -> dict[str, object]:
        """Load one canonical artifact and verify its source/profile digests."""
        path = ORACLE_DIR / f"{case_id}.json"
        content = path.read_bytes()
        artifact = json.loads(content)
        self.assertEqual(content, run_case.canonical_bytes(artifact))
        self.assertEqual(
            artifact["profile"]["sha256"],
            hashlib.sha256(PROFILE_PATH.read_bytes()).hexdigest(),
        )
        case_dir = ROOT / artifact["case"]["source"]
        self.assertEqual(
            artifact["case"]["source_sha256"],
            run_case.case_source_digest(case_dir),
        )
        for mode in ("raw", "instrumented"):
            decoded(artifact[mode]["stdout"])
            decoded(artifact[mode]["stderr"])
        return artifact

    def test_raw_binary_diagnostics_and_side_effects_are_exact(self) -> None:
        """Raw mode preserves non-UTF-8 bytes, status, stdin, and file writes."""
        artifact = self.load("binary-diagnostics")
        self.assertEqual(decoded(artifact["raw"]["stdout"]), b"out\x00\xff")
        self.assertTrue(decoded(artifact["raw"]["stderr"]).startswith(b"err\x00\xfe"))
        self.assertEqual(
            artifact["raw"]["exit"],
            {"code": 0, "signal": None, "timeout": False},
        )
        created = artifact["filesystem_diff"]["created"]["result.bin"]
        self.assertEqual(decoded(created["bytes"]), b"in\x00\xff\x00done")
        self.assertEqual(
            artifact["filesystem_diff"]["created"]["result.link"],
            # Darwin applies the process umask to the symlink mode reported by lstat.
            {"kind": "symlink", "mode": "0755", "target": "result.bin"},
        )

    def test_instrumented_run_adds_ordered_telemetry_only(self) -> None:
        """Instrumentation retains raw bytes while adding structured observations."""
        artifact = self.load("binary-diagnostics")
        self.assertEqual(artifact["raw"]["stdout"], artifact["instrumented"]["stdout"])
        self.assertEqual(artifact["raw"]["stderr"], artifact["instrumented"]["stderr"])
        telemetry = artifact["instrumented"]["telemetry"]
        self.assertEqual(telemetry["events"][0]["sequence"], 0)
        self.assertEqual(telemetry["events"][0]["severity_name"], "E_USER_WARNING")
        self.assertEqual(telemetry["events"][1]["sequence"], 1)
        self.assertEqual(
            telemetry["events"][1]["severity_name"],
            "E_USER_DEPRECATED",
        )
        self.assertIsNone(telemetry["exception"])
        self.assertEqual(telemetry["return"], {"type": "bool", "value": False})

    def test_exception_telemetry_does_not_leak_checkout_paths(self) -> None:
        """Exception mode records the same fatal plus stable structured exception data."""
        artifact = self.load("exception")
        self.assertEqual(artifact["raw"]["exit"]["code"], 255)
        self.assertEqual(artifact["instrumented"]["exit"]["code"], 255)
        telemetry = artifact["instrumented"]["telemetry"]
        self.assertEqual(telemetry["exception"]["class"], "RuntimeException")
        self.assertEqual(telemetry["exception"]["code"], 23)
        self.assertNotIn(
            b"/Users/",
            decoded(artifact["instrumented"]["stderr"]),
        )

    def test_exit_status_remains_authoritative_without_telemetry(self) -> None:
        """A direct exit remains valid evidence even when shutdown skips telemetry."""
        artifact = self.load("nonzero-exit")
        self.assertEqual(decoded(artifact["raw"]["stdout"]), b"before-exit\n")
        self.assertEqual(artifact["raw"]["exit"]["code"], 7)
        self.assertEqual(artifact["instrumented"]["exit"]["code"], 7)
        self.assertIsNone(artifact["instrumented"]["telemetry"])

    def test_timeout_kills_the_process_group_and_preserves_prior_output(self) -> None:
        """Timeout mode records SIGKILL while retaining bytes emitted before blocking."""
        artifact = self.load("timeout")
        for mode in ("raw", "instrumented"):
            self.assertEqual(decoded(artifact[mode]["stdout"]), b"before-timeout\n")
            self.assertEqual(
                artifact[mode]["exit"],
                {"code": None, "signal": 9, "timeout": True},
            )

    def test_source_built_profile_has_a_complete_reproducible_harness_corpus(self) -> None:
        """Every Gate 0 case is captured against each attested source-built PHP binary."""
        profile_root = SOURCE_PROFILE_PATH.parents[1]
        oracle_root = SOURCE_ORACLE_DIR.parents[1]
        for target in SUPPORTED_TARGETS:
            profile_path = profile_root / target / "streams-full.json"
            source_profile = json.loads(profile_path.read_bytes())
            self.assertEqual(
                source_profile["build"]["binary_source_attestation"],
                "source-build",
            )
            profile_sha = hashlib.sha256(profile_path.read_bytes()).hexdigest()
            for case_id in (
                "binary-diagnostics",
                "exception",
                "nonzero-exit",
                "timeout",
            ):
                with self.subTest(target=target, case_id=case_id):
                    path = oracle_root / target / "streams-full" / f"{case_id}.json"
                    content = path.read_bytes()
                    artifact = json.loads(content)
                    self.assertEqual(content, run_case.canonical_bytes(artifact))
                    self.assertEqual(artifact["profile"]["sha256"], profile_sha)
                    self.assertEqual(
                        artifact["profile"]["php_binary_sha256"],
                        source_profile["oracle"]["php_binary_sha256"],
                    )
                    decoded(artifact["raw"]["stdout"])
                    decoded(artifact["raw"]["stderr"])


if __name__ == "__main__":
    unittest.main()
