#!/usr/bin/env python3
"""Exercise Apple leak-runner failure reporting with real child processes."""

from pathlib import Path
from contextlib import nullcontext
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import test_embed


class AppleLeakRunnerTests(unittest.TestCase):
    """A diagnostic checkpoint must never turn a failed lifecycle into success."""

    def run_child(self, source, leaks="0 leaks\n", leak_status=0, checkpoint_timeout=False):
        """Replace only Apple tools, retaining real process I/O and exit status."""
        root = Path(__file__).resolve().parents[2]
        (root / "target").mkdir(exist_ok=True)
        real_select = test_embed.select.select

        def expire_checkpoint(readers, writers, errors, timeout):
            """Expire after the child writes, without consuming its diagnostics."""
            self.assertTrue(real_select(readers, writers, errors, timeout)[0])
            return [], [], []

        with tempfile.TemporaryDirectory(prefix="phpext-runner-", dir=root / "target") as work:
            command = [sys.executable, "-c", source]
            timeout_patch = patch.object(test_embed.select, "select", side_effect=expire_checkpoint)
            with (timeout_patch if checkpoint_timeout else nullcontext()), patch.object(test_embed, "checked"), patch.object(
                test_embed.subprocess, "run",
                return_value=subprocess.CompletedProcess(["leaks"], leak_status, leaks, ""),
            ):
                test_embed.check_apple_leaks(command, dict(test_embed.os.environ), Path(work))

    def test_early_failure_keeps_stderr(self):
        """An exited child still reports the original lifecycle error."""
        with self.assertRaisesRegex(RuntimeError, "original fatal detail"):
            self.run_child("import sys; print('original fatal detail', file=sys.stderr); sys.exit(1)")

    def test_failed_checkpoint_keeps_status_and_leak_report(self):
        """A completed failed request is inspected but cannot pass even with no leaks."""
        for checkpoint, status in [("failed", 1), ("failed", 0), ("ok", 1)]:
            with self.subTest(checkpoint=checkpoint, status=status):
                with self.assertRaisesRegex(RuntimeError, "lifecycle failure detail") as error:
                    self.run_child(
                        "import sys; print('lifecycle failure detail', file=sys.stderr); "
                        f"print('request isolation {checkpoint}', flush=True); input(); sys.exit({status})"
                    )
                self.assertIn("0 leaks", str(error.exception))

    def test_checkpoint_timeout_keeps_child_diagnostics(self):
        """Killing a child that missed readiness preserves both output streams."""
        with self.assertRaisesRegex(RuntimeError, "did not reach.*checkpoint.*-9") as error:
            self.run_child(
                "import sys; print('blocked teardown detail', file=sys.stderr, flush=True); "
                "print('before checkpoint', flush=True); input()", checkpoint_timeout=True,
            )
        self.assertIn("blocked teardown detail", str(error.exception))
        self.assertIn("before checkpoint", str(error.exception))

    def test_leak_failure_keeps_lifecycle_diagnostics(self):
        """Leak findings and request failures remain visible together."""
        with self.assertRaisesRegex(RuntimeError, "retained native checkpoint") as error:
            self.run_child(
                "import sys; print('request failure detail', file=sys.stderr); "
                "print('request isolation failed', flush=True); input(); sys.exit(1)",
                leaks="retained native checkpoint\n", leak_status=1,
            )
        self.assertIn("request failure detail", str(error.exception))


if __name__ == "__main__":
    unittest.main()
