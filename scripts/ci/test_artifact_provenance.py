#!/usr/bin/env python3
"""Check inner nextest identity and byte integrity independently of zstd decoding."""

from pathlib import Path
import tempfile
import unittest

import test_archive_manifest as manifest


class ArchiveProvenanceTests(unittest.TestCase):
    """Consumers must reject another attempt or altered bytes before execution."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="elephc-provenance-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.identity = {"sha": "a" * 40, "variant": "workspace", "source": {"version": "original"}}
        for name in ("nextest-archive.tar.zst", "cargo-nextest", "cargo-nextest.sha256"):
            (self.root / name).write_bytes(b"complete fixture")
        manifest.write(self.root, self.identity, "42", "1")

    def test_correct_bundle_survives_relocation(self):
        destination = self.root / "relocated"
        destination.mkdir()
        for path in list(self.root.iterdir()):
            if path.is_file():
                path.rename(destination / path.name)
        manifest.verify(destination, self.identity, "42", "1")

    def test_changed_version_sha_variant_run_and_attempt_are_rejected(self):
        for identity in ({**self.identity, "sha": "b" * 40},
                         {**self.identity, "variant": "codegen"},
                         {**self.identity, "source": {"version": "stamped"}}):
            with self.assertRaises(ValueError):
                manifest.verify(self.root, identity, "42", "1")
        for run, attempt in (("43", "1"), ("42", "2")):
            with self.assertRaises(ValueError):
                manifest.verify(self.root, self.identity, run, attempt)

    def test_truncated_archive_and_replaced_runner_are_rejected(self):
        for name in ("nextest-archive.tar.zst", "cargo-nextest"):
            path = self.root / name
            original = path.read_bytes()
            path.write_bytes(b"different")
            with self.assertRaises(ValueError):
                manifest.verify(self.root, self.identity, "42", "1")
            path.write_bytes(original)


if __name__ == "__main__":
    unittest.main()
