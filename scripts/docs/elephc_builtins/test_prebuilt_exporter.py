"""CI exporter selection must never start an implicit compiler build."""

import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from scripts.docs.elephc_builtins.extract import _gen_builtins_command


class PrebuiltExporterTests(unittest.TestCase):
    """Use isolated executable stand-ins, without invoking Cargo or the exporter."""

    def test_missing_prebuilt_exporter_fails_instead_of_building(self):
        with tempfile.TemporaryDirectory(prefix="elephc-exporter-test-") as temporary:
            with patch.dict(os.environ, {"ELEPHC_DOCS_PREBUILT": "1"}):
                with self.assertRaisesRegex(RuntimeError, "Restore"):
                    _gen_builtins_command(Path(temporary))

    def test_prebuilt_exporter_does_not_depend_on_checkout_timestamps(self):
        with tempfile.TemporaryDirectory(prefix="elephc-exporter-test-") as temporary:
            root = Path(temporary)
            executable = root / "target/debug/examples/gen_builtins"
            executable.parent.mkdir(parents=True)
            executable.write_text("#!/bin/sh\nexit 0\n")
            executable.chmod(0o755)
            os.utime(executable, (1, 1))
            with patch.dict(os.environ, {"ELEPHC_DOCS_PREBUILT": "1"}):
                self.assertEqual(_gen_builtins_command(root), [str(executable), "--include-internal"])
