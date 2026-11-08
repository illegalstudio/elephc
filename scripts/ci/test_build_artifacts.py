#!/usr/bin/env python3
"""Exercise relocated CI outputs, identity rejection and hostile archive members."""

import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest

import build_artifacts as artifacts


class BuildArtifactTests(unittest.TestCase):
    """Consumers must validate complete outputs before executing or restoring them."""

    def setUp(self):
        """Create a small compiler stand-in in one automatically cleaned directory."""
        self.temporary = tempfile.TemporaryDirectory(prefix="elephc-artifact-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source = self.root / "source"
        self.source.mkdir()
        binary = self.source / "target/debug/elephc"
        binary.parent.mkdir(parents=True)
        binary.write_bytes(b"#!/bin/sh\nprintf 'compiler\\n'\n")
        binary.chmod(0o755)
        self.expected = artifacts.identity("a" * 40, "linux-x86_64", "debug", "", "compiler", "o/r")
        self.artifact = self.root / "artifact"
        artifacts.create(self.source, self.artifact, ["target/debug/elephc"],
                         self.expected, {"run_id": "123", "attempt": "2"})

    def test_relocated_compiler_retains_executable_mode(self):
        """A consumer on another runner can use the same bytes and executable mode."""
        output = self.root / "consumer"
        artifacts.restore(self.artifact, output, self.expected, "123", "2")
        binary = output / "target/debug/elephc"
        self.assertEqual(binary.read_bytes(), (self.source / "target/debug/elephc").read_bytes())
        self.assertEqual(binary.stat().st_mode & 0o777, 0o755)

    def test_wrong_sha_platform_features_and_run_are_rejected(self):
        """Outputs cannot satisfy a different commit, configuration or producer."""
        for field, value in (("sha", "b" * 40), ("target", "linux-aarch64"),
                             ("profile", "release"), ("features", ["curl"]),
                             ("repository", "other/repository")):
            with self.subTest(field=field), self.assertRaises(ValueError):
                artifacts.verify(self.artifact, {**self.expected, field: value}, "123", "2")
        with self.assertRaises(ValueError):
            artifacts.verify(self.artifact, self.expected, "456", "2")
        with self.assertRaises(ValueError):
            artifacts.verify(self.artifact, self.expected, "123", "3")

    def test_corrupt_archive_does_not_write_outputs(self):
        """A truncated upload fails before creating any destination file."""
        archive = self.artifact / "payload.tar.gz"
        archive.write_bytes(archive.read_bytes()[:20])
        output = self.root / "consumer"
        with self.assertRaises(ValueError):
            artifacts.restore(self.artifact, output, self.expected)
        self.assertFalse(output.exists())

    def rewrite_archive(self, info, contents=b"malicious"):
        """Give a hostile archive matching outer checksums to test inner validation."""
        archive = self.artifact / "payload.tar.gz"
        with tarfile.open(archive, "w:gz") as bundle:
            if info.isfile():
                info.size = len(contents)
                bundle.addfile(info, io.BytesIO(contents))
            else:
                bundle.addfile(info)
        path = self.artifact / "manifest.json"
        manifest = json.loads(path.read_text())
        manifest["archive_sha256"] = artifacts.digest(archive)
        path.write_text(json.dumps(manifest))

    def test_path_traversal_and_links_are_rejected(self):
        """Even matching outer checksums do not permit writes through archive links."""
        for name, kind in (("../escape", tarfile.REGTYPE),
                           ("/absolute", tarfile.REGTYPE),
                           ("target/debug/elephc", tarfile.SYMTYPE),
                           ("target/debug/elephc", tarfile.LNKTYPE)):
            with self.subTest(name=name, kind=kind):
                info = tarfile.TarInfo(name)
                info.type = kind
                info.linkname = "../../escape" if kind != tarfile.REGTYPE else ""
                self.rewrite_archive(info)
                with self.assertRaises(ValueError):
                    artifacts.verify(self.artifact, self.expected)

    def test_existing_destination_symlink_is_rejected(self):
        """A valid artifact must not overwrite files outside its destination."""
        output = self.root / "consumer"
        output.mkdir()
        (output / "target").symlink_to(self.source / "target", target_is_directory=True)
        with self.assertRaises(ValueError):
            artifacts.restore(self.artifact, output, self.expected)

    def test_missing_output_and_incomplete_archive_fail(self):
        """A producer and a consumer both require every declared output."""
        with self.assertRaises(ValueError):
            artifacts.create(self.source, self.root / "missing", ["missing.a"], self.expected, {})
        path = self.artifact / "manifest.json"
        manifest = json.loads(path.read_text())
        manifest["files"]["target/debug/missing.a"] = manifest["files"]["target/debug/elephc"]
        path.write_text(json.dumps(manifest))
        with self.assertRaises(ValueError):
            artifacts.verify(self.artifact, self.expected)


if __name__ == "__main__":
    unittest.main()
