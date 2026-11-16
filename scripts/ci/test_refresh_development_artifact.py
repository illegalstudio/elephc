"""Check byte-preserving renewal without executing downloaded code or trusting archive paths."""

import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile

from build_artifacts import digest
from refresh_development_artifact import refresh, unpack


class RefreshTests(unittest.TestCase):
    """Treat the original upload digest and its package manifest as one retained identity."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="elephc-retention-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.expected = dict(id=123, target="linux-x86_64", sha="a" * 40)
        self.expected["name"] = f"elephc-dev-linux-x86_64-{self.expected['sha']}-1"
        identity = dict(sha=self.expected["sha"], target=self.expected["target"], profile="debug")
        self.manifest = dict(identity, schema=1, **{
            kind: {"identity": dict(identity, kind=kind, repository="o/r")}
            for kind in ("compiler", "bridges")})

    def archive(self, extra=None, bad_checksum=False):
        """Create a tiny package with no executable, using the production container shape."""
        name = f"{self.expected['name'].rsplit('-', 1)[0]}.tar.gz"
        tarball = self.root / name
        with tarfile.open(tarball, "w:gz") as bundle:
            data = json.dumps(self.manifest).encode()
            item = tarfile.TarInfo("build-manifest.json")
            item.size = len(data)
            bundle.addfile(item, io.BytesIO(data))
        archive = self.root / "artifact.zip"
        with zipfile.ZipFile(archive, "w") as bundle:
            bundle.write(tarball, name)
            checksum = "0" * 64 if bad_checksum else digest(tarball)
            bundle.writestr(name + ".sha256", f"{checksum}  {name}\n")
            if extra:
                bundle.writestr(extra, "unexpected")
        self.expected["digest"] = "sha256:" + digest(archive)
        return archive, tarball

    def test_copies_identical_package_bytes(self):
        archive, tarball = self.archive()
        with patch("refresh_development_artifact.subprocess.run", side_effect=AssertionError("execution")):
            unpack(archive, self.root / "output", self.expected, "o/r")
        self.assertEqual((self.root / "output" / tarball.name).read_bytes(), tarball.read_bytes())

    def test_corrupt_download_and_inner_checksum_are_rejected(self):
        archive, _ = self.archive()
        archive.write_bytes(archive.read_bytes() + b"corruption")
        with self.assertRaisesRegex(ValueError, "artifact digest"):
            unpack(archive, self.root / "output", self.expected, "o/r")
        archive, _ = self.archive(bad_checksum=True)
        with self.assertRaisesRegex(ValueError, "checksum mismatch"):
            unpack(archive, self.root / "output", self.expected, "o/r")

    def test_unexpected_paths_and_mismatched_source_provenance_are_rejected(self):
        for extra in ("../escape", "/absolute", "run.sh"):
            archive, _ = self.archive(extra=extra)
            with self.assertRaisesRegex(ValueError, "Unexpected files"):
                unpack(archive, self.root / "output", self.expected, "o/r")
        for kind in ("compiler", "bridges"):
            self.manifest[kind]["identity"]["sha"] = "b" * 40
            archive, _ = self.archive()
            with self.assertRaisesRegex(ValueError, "provenance mismatch"):
                unpack(archive, self.root / "output", self.expected, "o/r")
            self.manifest[kind]["identity"]["sha"] = self.expected["sha"]

    def test_refresh_rechecks_exact_live_metadata_before_downloading(self):
        self.archive()
        metadata = dict(self.expected, expired=True)
        with patch("refresh_development_artifact.gh", return_value=metadata), \
                patch("refresh_development_artifact.subprocess.run") as download:
            with self.assertRaisesRegex(ValueError, "changed or expired"):
                refresh("o/r", self.expected, self.root / "output")
            download.assert_not_called()


if __name__ == "__main__":
    unittest.main()
