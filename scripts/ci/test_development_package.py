"""Keep installed-package modes and timestamps independent of the producer checkout."""

import os
from pathlib import Path
import tarfile
import tempfile
import time
import unittest
from unittest.mock import patch

import development_artifact as subject


class DevelopmentPackageTests(unittest.TestCase):
    def test_package_retimestamps_verified_bridges_and_preserves_executable_mode(self):
        with tempfile.TemporaryDirectory(prefix="elephc-package-test-") as directory:
            root = Path(directory)
            compiler = root / "target/debug/elephc"
            compiler.parent.mkdir(parents=True)
            compiler.write_bytes(b"compiler")
            compiler.chmod(0o755)
            bridge = compiler.with_name("libelephc_crypto.a")
            bridge.write_bytes(b"archive")
            os.utime(bridge, (1, 1))
            manifest = root / "manifest.json"
            manifest.write_text('{}')
            before = int(time.time())
            with patch.object(subject, "NOTICES", {}), patch.object(subject, "host_files", side_effect=lambda root, profile, kind:
                    ["target/debug/elephc" if kind == "compiler" else "target/debug/libelephc_crypto.a"]):
                result = subject.package(root, root / "package", "macos-aarch64", "a" * 40, manifest, manifest)
            with tarfile.open(result) as archive:
                self.assertEqual(archive.getmember("elephc").mode & 0o777, 0o755)
                self.assertGreaterEqual(archive.getmember("libelephc_crypto.a").mtime, before)
                self.assertEqual(archive.extractfile("libelephc_crypto.a").read(), b"archive")
            self.assertEqual(bridge.stat().st_mtime, 1)


if __name__ == "__main__":
    unittest.main()
