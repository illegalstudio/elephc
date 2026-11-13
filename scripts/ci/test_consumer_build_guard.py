"""The build guard catches swallowed rebuild failures without invoking a real compiler."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("consumer_cargo.sh")


class ConsumerBuildGuardTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="elephc-build-guard-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.log = self.root / "unexpected-builds"
        self.cargo = self.root / "fake-cargo"
        self.cargo.write_text("#!/bin/sh\nexit 0\n")
        self.cargo.chmod(0o755)
        self.env = {**os.environ, "ELEPHC_CI_BUILD_GUARD_LOG": str(self.log), "ELEPHC_CI_REAL_CARGO": str(self.cargo)}

    def invoke(self, *arguments):
        return subprocess.run(["bash", str(SCRIPT), *arguments], env=self.env, capture_output=True).returncode

    def test_build_test_run_and_archive_creation_are_rejected_and_recorded(self):
        for arguments in (("build",), ("test",), ("run",), ("check",), ("nextest", "archive"), ("nextest", "run")):
            self.assertEqual(self.invoke(*arguments), 86)
        self.assertEqual(len(self.log.read_text().splitlines()), 6)

    def test_metadata_and_prebuilt_test_execution_do_not_trigger_the_guard(self):
        for arguments in (("--version",), ("metadata", "--no-deps"),
                          ("nextest", "run", "--archive-file", "tests.tar.zst"),
                          ("nextest", "list", "--binaries-metadata", "metadata.json")):
            self.assertEqual(self.invoke(*arguments), 0)
        self.assertFalse(self.log.exists())
